use commons::dlmm::accounts::*;
use commons::dlmm::types::*;
use commons::{
    BASIS_POINT_MAX, 
    DEFAULT_BIN_PER_POSITION, 
    MAX_BIN_PER_ARRAY,
    *,
};
use commons::{
    fetch_lb_pair_state,
    fetch_positions,
    get_matching_positions,
    get_bytemuck_account,
    derive_event_authority_pda
};

use crate::position_manager::*;
use crate::pair_config::*;
use crate::pricing;
use crate::errors::CustomError;
use crate::IRMA_ID;
use crate::{MarketMakingMode, Maint, StateMap, StableState};
use std::collections::HashMap;
use std::str::FromStr;

use anchor_lang::prelude::*;
use anchor_lang::prelude::instruction::Instruction;
use anchor_lang::solana_program::{
    clock::Clock,
    program::{invoke, invoke_signed},
    sysvar::Sysvar,
    // compute_budget::ComputeBudgetInstruction
};
use anchor_lang::InstructionData;
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use anchor_spl::token_interface::{Mint, TokenAccount};
const DLMM_ID: Pubkey = commons::dlmm::ID;

// Enum to represent either type of deserializable account
#[derive(Debug, Clone)]
pub enum AccountData<T> {
    Bytemuck(T),
    Anchor(T),
}

impl<T> AccountData<T> {
    pub fn into_inner(self) -> T {
        match self {
            AccountData::Bytemuck(data) => data,
            AccountData::Anchor(data) => data,
        }
    }
}

// Meteora Core (taken from Meteora DLMM SDK and adapted for IRMA)
// Removed all RPC stuff because this is going to run on-chain.
#[account]
#[repr(C)]
#[derive(Debug)]
pub struct Core {
    pub owner: Pubkey,               // Owner of this Core instance, which should be Rock Stable
    pub config: Vec<PairConfig>,     // There should be an LbPair for each reserve stablecoin
    pub position_data: AllPosition,  // Renamed from "state" to avoid IDL conflicts
}

impl Core {
    /// Create a new instance of Core
    /// params: 
    /// ctx: Context<Init>, 
    /// owner: Pubkey, 
    /// config_keys: Vec<Pubkey> == the token pair config account pubkeys, 
    /// state_key: Pubkey == the position state account pubkey
    pub fn create_core(owner: Pubkey, config: Vec<Pubkey>) -> Result<Core> {
        // Core initialization logic here
        require!(config.is_empty(), CustomError::ConfigMustBeEmpty);

        // we will eventually have six trading pairs (one each for popular stablecoins)
        // for now, let's just initialize with zero pairs
        
        Ok(Core {
            owner,
            config: vec![],
            position_data: AllPosition::new(&vec![]).unwrap(),
        })
    }

    // Helper function to get current epoch time in seconds (on-chain version)
    fn get_epoch_sec() -> Result<i64> {
        let clock = Clock::get()?;
        Ok(clock.unix_timestamp)
    }

    fn get_multiple_anchor_accounts<T: anchor_lang::AccountDeserialize>(
        context: &Context<Maint>,
        pubkeys: &Vec<Pubkey>
    ) -> Result<HashMap<Pubkey, Option<T>>> {
        let mut data = HashMap::new();
        for pubkey in pubkeys.iter() {
            let account_info = context.remaining_accounts.iter()
                .find(|acc| acc.key == pubkey);
            if let Some(account_info) = account_info {
                let account_data = T::try_deserialize(&mut &account_info.data.borrow()[8..])?;
                data.insert(*pubkey, Some(account_data));
            } else {
                data.insert(*pubkey, None);
            }
        }
        Ok(data)
    }

    fn execute_meteora_instruction(
        context: &Context<Maint>,
        instructions: Vec<Instruction>,
        sign: bool
    ) -> Result<()> {
        let key = context.accounts.irma_admin.key();
        for instruction in instructions.iter() {
            if sign {
                // If PDA signing needed - manually derive bump
                let (_pda, bump) = Pubkey::find_program_address(
                    &[b"irma", key.as_ref()],
                    &IRMA_ID,
                );
                let seeds = &[
                    b"irma",
                    key.as_ref(),
                    &[bump],
                ];
                invoke_signed(&instruction, context.remaining_accounts, &[seeds])?;
            }
            else {
                invoke(&instruction, context.remaining_accounts)?;
            }
        }
        Ok(())
    }

    /// Refresh internal state by fetching positions and bin arrays using provided accounts
    pub fn refresh_position_data_with_accounts(
        &mut self,
        state: &mut Account<StateMap>,
        remaining_accounts: &[AccountInfo],
        token: String, // symbol of the stablecoin
        amount: u64,
        is_sale: bool
    ) -> Result<()> {
        // Call pricing functions directly on the state first
        if is_sale {
            pricing::mint_irma(state, &token, amount)?;
        } else {
            pricing::redeem_irma(state, &token, amount)?;
        }

        // Call the core position refresh logic without needing a full context
        self.refresh_position_data(&state.reserves, remaining_accounts, token)?;
        
        Ok(())
    }
    
    /// Refresh internal state by fetching positions and bin arrays.
    /// This function is called from sale_trade_event and buy_trade_event.
    /// Instead of parameters, it uses context to fetch necessary accounts
    /// and the config Vec in Core to go through the pairs.
    pub fn refresh_position_data(
        &mut self,
        reserves: &[StableState], // Changed to slice reference
        remaining_accounts: &[AccountInfo],
        token: String // symbol of the stablecoin
    ) -> Result<()> {

        // search for lbpair matching the token
        let quote_token = reserves.iter().find(|stablecoin| stablecoin.symbol == token);
        require!(quote_token.is_some(), CustomError::InvalidReserveList);

        let pair_address = quote_token.unwrap().pool_id; // DLMM LbPair address

        msg!("==> Refreshing state for pair: {}", pair_address.to_string());

        // all_positions should contain all relevant position accounts
        // for the current mint or redemption swap

        // get all positions by the same user, for this trade pair.
        let mut position_key_with_state = get_matching_positions(
            remaining_accounts,
            &self.owner, 
            &pair_address
        ).or_else(|error| Err(error)).unwrap();

        let mut position_pks = vec![];
        // Note: We'll fetch positions and bin_arrays dynamically when needed
        // let mut positions = vec![];
        let mut min_bin_id = 0;
        let mut max_bin_id = 0;
        // let mut bin_arrays_vec = Vec::<(Pubkey, BinArray)>::new();

        msg!("    Found {} positions", position_key_with_state.len());
        let mut bin_array_keys = vec![];
        if position_key_with_state.len() > 0 {
            // sort position by bin id
            position_key_with_state
                .sort_by(|(_, a), (_, b)| a.lower_bin_id.cmp(&b.lower_bin_id));

            min_bin_id = position_key_with_state
                .first()
                .map(|(_key, state)| state.lower_bin_id)
                .unwrap();

            max_bin_id = position_key_with_state
                .last()
                .map(|(_key, state)| state.upper_bin_id)
                .unwrap();

            for (key, _state) in position_key_with_state.iter() {
                position_pks.push(*key);
                // Don't store the position data - fetch dynamically when needed
                // positions.push(state.to_owned());
            }

            bin_array_keys = position_key_with_state
                .iter()
                .filter_map(|(_key, state)| state.get_bin_array_keys_coverage().ok())
                .flatten()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            // Note: We'll fetch bin arrays dynamically when needed, not store them
            // let bin_arrays_raw: HashMap::<Pubkey, Option<BinArray>> 
            //                 = Core::get_multiple_bytemuck_accounts(context, &bin_array_keys)?;

            // msg!("    Found {} bin arrays", bin_arrays_raw.len());

            // for (key, bin_array_option) in bin_arrays_raw.iter() {
            //     if let Some(bin_array_state) = bin_array_option {
            //         bin_arrays_vec.push((*key, *bin_array_state));
            //     }
            // }
        }

        let all_state = &mut self.position_data;
        let state = all_state.get_position_mut(&pair_address).ok_or_else(|| {
            error!(CustomError::PositionNotFound)
        })?;

        state.lb_pair = pair_address;
        // Don't store non-serializable types - they will be fetched dynamically
        // state.bin_arrays = bin_arrays_vec;
        state.bin_array_pks = bin_array_keys; // keep just the keys and fetch dynamically
        state.position_pks = position_pks;
        // state.positions = positions;
        state.min_bin_id = min_bin_id;
        state.max_bin_id = max_bin_id;
        state.last_update_timestamp = Self::get_epoch_sec()?.max(0) as u64;

        Ok(())
    }

    /// Fetch token info for all tokens in the positions
    pub fn fetch_token_info(&mut self, context: &Context<Maint>) -> Result<()> {
        let token_mints_with_program = self.get_all_token_mints_with_program_id(context)?;

        let token_mint_keys = token_mints_with_program
            .iter()
            .map(|(key, _program_id)| *key)
            .collect::<Vec<_>>();

        let accounts: HashMap<Pubkey, Option<Mint>> = Core::get_multiple_anchor_accounts(context, &token_mint_keys)?;
        let mut tokens = Vec::<TokenEntry>::new();

        for ((_key, program_id), account) in token_mints_with_program.iter().zip(accounts) {
            if let (pubkey, Some(mint)) = account {
                let mint_info = MintInfo::from(&mint);
                let mint_with_program = MintWithProgramId {
                    mint_info,
                    program_id: *program_id,
                };
                let token_entry = TokenEntry {
                    pubkey,
                    mint_with_program,
                };
                tokens.push(token_entry);
            }
        }
        let state = &mut self.position_data;
        state.tokens = tokens;

        Ok(())
    }

    fn get_all_token_mints_with_program_id(&self, context: &Context<Maint>) -> Result<Vec<(Pubkey, Pubkey)>> {
        let state = &self.position_data;
        let mut token_mints_with_program = vec![];

        for position_entry in state.all_positions.iter() {
            let lb_pair_state = fetch_lb_pair_state(
                context.remaining_accounts, &position_entry.position.lb_pair
            )?;
            let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
            token_mints_with_program.push((lb_pair_state.token_x_mint, token_x_program));
            token_mints_with_program.push((lb_pair_state.token_y_mint, token_y_program));
        }

        token_mints_with_program.sort_unstable();
        token_mints_with_program.dedup();
        Ok(token_mints_with_program)
    }

    pub fn get_position_state(&self, lp_pair: Pubkey) -> SinglePosition {
        let state = &self.position_data;
        let position = state.get_position(&lp_pair).unwrap();
        position.clone()
    }

    pub fn get_mut_state(&mut self, lp_pair: Pubkey) -> &mut SinglePosition {
        let state = &mut self.position_data;
        let position = state.get_position_mut(&lp_pair).unwrap();
        position
    }

    // Helper function to get or create associated token account (ATA) on-chain
    fn get_or_create_ata(
        &self,
        context: &Context<Maint>,
        token_mint: Pubkey,
        token_program: Pubkey,
        owner: &Pubkey,
        payer: &Signer,
    ) -> Result<Pubkey> {
        let ata_address = get_associated_token_address_with_program_id(
            owner,
            &token_mint,
            &token_program,
        );

        // Check if ATA already exists in remaining_accounts
        let ata_exists = context.remaining_accounts.iter()
            .any(|acc| acc.key == &ata_address);

        if !ata_exists {
            // Create ATA instruction manually
            let create_ata_ix = Instruction {
                program_id: anchor_spl::associated_token::ID,
                accounts: vec![
                    AccountMeta::new(payer.key(), true),          // payer
                    AccountMeta::new(ata_address, false),         // associated_token
                    AccountMeta::new_readonly(*owner, false),     // owner
                    AccountMeta::new_readonly(token_mint, false), // mint
                    AccountMeta::new_readonly(system_program::ID, false), // system_program
                    AccountMeta::new_readonly(token_program, false),      // token_program
                ],
                data: vec![], // No data needed for ATA creation
            };

            // Execute the instruction
            Core::execute_meteora_instruction(context, vec![create_ata_ix], true)?;
        }

        Ok(ata_address)
    }

    /// Initialize user associated token accounts for all tokens in the position
    pub fn init_user_ata(
        &self, context: &Context<Maint>,
    ) -> Result<()> {
        let wallet = &context.accounts.irma_admin;
        for (token_mint, program_id) in self.get_all_token_mints_with_program_id(context)?.iter() {
            self.get_or_create_ata(
                context,
                *token_mint,
                *program_id,
                &wallet.key(),
                wallet,
            )?;
        }

        Ok(())
    }

    /// withdraw all positions and close them
    /// we need a version of this that just withdraws from one side without closing
    pub fn withdraw(
        &self,
        context: &Context<Maint>,
        state: &SinglePosition
    ) -> Result<()> {
        if state.position_pks.len() == 0 {
            return Ok(());
        }

        // let rpc_client = self.rpc_client();

        let payer = context.accounts.irma_admin.clone();

        let (event_authority, _bump) = derive_event_authority_pda();

        let lb_pair = state.lb_pair;
        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &state.lb_pair)?;

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;

        let mut remaining_account_info = RemainingAccountsInfo { slices: vec![] };
        let mut transfer_hook_remaining_accounts = vec![];

        if let Some((slices, remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                context.remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_account_info.slices = slices;
            transfer_hook_remaining_accounts = remaining_accounts;
        }

        for &position_key in state.position_pks.iter() {
            let vec_positions = fetch_positions(context.remaining_accounts, &[position_key])?;
            let position_state = vec_positions
                .get(0)
                .ok_or(error!(CustomError::PositionNotFound))?;

            let bin_arrays_account_meta = position_state.get_bin_array_accounts_meta_coverage()?;

            let user_token_x = get_associated_token_address_with_program_id(
                &payer.key(),
                &lb_pair_state.token_x_mint,
                &token_x_program,
            );

            let user_token_y = get_associated_token_address_with_program_id(
                &payer.key(),
                &lb_pair_state.token_y_mint,
                &token_y_program,
            );

            let mut instructions = vec![];

            let main_accounts = dlmm::client::accounts::RemoveLiquidityByRange2 {
                position: position_key,
                lb_pair,
                bin_array_bitmap_extension: Some(DLMM_ID),
                user_token_x,
                user_token_y,
                reserve_x: lb_pair_state.reserve_x,
                reserve_y: lb_pair_state.reserve_y,
                token_x_mint: lb_pair_state.token_x_mint,
                token_y_mint: lb_pair_state.token_y_mint,
                sender: payer.key(),
                token_x_program,
                token_y_program,
                memo_program: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
                event_authority,
                program: DLMM_ID,
            }
            .to_account_metas(None);

            let remaining_accounts = [
                transfer_hook_remaining_accounts.clone(),
                bin_arrays_account_meta.clone(),
            ]
            .concat();

            let data = dlmm::client::args::RemoveLiquidityByRange2 {
                from_bin_id: position_state.lower_bin_id,
                to_bin_id: position_state.upper_bin_id,
                bps_to_remove: BASIS_POINT_MAX as u16,
                remaining_accounts_info: remaining_account_info.clone(),
            }
            .data();

            let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

            let remove_all_ix = Instruction {
                program_id: DLMM_ID,
                accounts,
                data,
            };

            instructions.push(remove_all_ix);

            let main_accounts = dlmm::client::accounts::ClaimFee2 {
                lb_pair,
                position: position_key,
                sender: payer.key(),
                event_authority,
                program: DLMM_ID,
                reserve_x: lb_pair_state.reserve_x,
                reserve_y: lb_pair_state.reserve_y,
                token_x_mint: lb_pair_state.token_x_mint,
                token_y_mint: lb_pair_state.token_y_mint,
                token_program_x: token_x_program,
                token_program_y: token_y_program,
                memo_program: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
                user_token_x,
                user_token_y,
            }
            .to_account_metas(None);

            let remaining_accounts = [
                transfer_hook_remaining_accounts.clone(),
                bin_arrays_account_meta.clone(),
            ]
            .concat();

            let data = dlmm::client::args::ClaimFee2 {
                min_bin_id: position_state.lower_bin_id,
                max_bin_id: position_state.upper_bin_id,
                remaining_accounts_info: remaining_account_info.clone(),
            }
            .data();

            let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

            let claim_fee_ix = Instruction {
                program_id: DLMM_ID,
                accounts,
                data,
            };

            instructions.push(claim_fee_ix);

            let accounts = dlmm::client::accounts::ClosePosition2 {
                position: position_key,
                sender: payer.key(),
                rent_receiver: payer.key(),
                event_authority,
                program: DLMM_ID,
            }
            .to_account_metas(None);

            let data = dlmm::client::args::ClosePosition2 {}.data();

            let close_position_ix = Instruction {
                program_id: DLMM_ID,
                accounts: accounts.to_vec(),
                data,
            };

            instructions.push(close_position_ix);

            let _result = Core::execute_meteora_instruction(context, instructions, true)?;
            msg!("Close position_key {position_key} {result}");
        }

        Ok(())
    }



    /// Swap tokens on the DLMM.
    /// We may need this to overcome AMM behavior, in case off-chain swap is too slow.
    /// If not, according to Taha, we can use withdraw() above instead.
    pub fn swap(
        &self,
        context: &Context<Maint>,
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool
    ) -> Result<()> {

        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &state.lb_pair)?;

        msg!("==> Swapping on pair: {}", state.lb_pair);

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
        let lb_pair = state.lb_pair;

        let payer = context.accounts.irma_admin.clone();

        let (event_authority, _bump) = derive_event_authority_pda();

        msg!("    event authority: {}", event_authority);

        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);

        let bitmap_extension = match get_bytemuck_account::<BinArrayBitmapExtension>(context.remaining_accounts, &bin_array_bitmap_extension) {
            Some(bitmap_extension) => bitmap_extension,
            None => BinArrayBitmapExtension::default(),
        };

        // msg!("    bin array bitmap extension: {}", bitmap_extension);

        let bin_arrays_account_meta = get_bin_array_pubkeys_for_swap(
            lb_pair,
            &lb_pair_state,
            Some(&bitmap_extension),
            swap_for_y,
            3,
        )?
        .into_iter()
        .map(|key| AccountMeta::new(key, false))
        .collect::<Vec<_>>();

        msg!("    bin arrays account meta: {}", bin_arrays_account_meta.len());

        let (user_token_in, user_token_out) = if swap_for_y {
            (
                get_associated_token_address_with_program_id(
                    &payer.key(),
                    &lb_pair_state.token_x_mint,
                    &token_x_program,
                ),
                get_associated_token_address_with_program_id(
                    &payer.key(),
                    &lb_pair_state.token_y_mint,
                    &token_y_program,
                ),
            )
        } else {
            (
                get_associated_token_address_with_program_id(
                    &payer.key(),
                    &lb_pair_state.token_y_mint,
                    &token_y_program,
                ),
                get_associated_token_address_with_program_id(
                    &payer.key(),
                    &lb_pair_state.token_x_mint,
                    &token_x_program,
                ),
            )
        };

        let mut remaining_accounts_info = RemainingAccountsInfo { slices: vec![] };
        let mut remaining_accounts = vec![];

        msg!("    Preparing Token 2022 related accounts...");

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                context.remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts.extend(transfer_hook_remaining_accounts);
        }

        msg!("    transfer hook remaining accounts: {}", remaining_accounts.len());

        remaining_accounts.extend(bin_arrays_account_meta);

        let main_accounts = dlmm::client::accounts::Swap2 {
            lb_pair,
            bin_array_bitmap_extension: Some(bin_array_bitmap_extension),
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            token_x_program,
            token_y_program,
            user: payer.key(),
            user_token_in,
            user_token_out,
            oracle: lb_pair_state.oracle,
            host_fee_in: Some(DLMM_ID),
            event_authority,
            program: DLMM_ID,
            memo_program: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
        }
        .to_account_metas(None);

        let data = dlmm::client::args::Swap2 {
            amount_in,
            min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y, &lb_pair_state)?,
            remaining_accounts_info,
        }
        .data();

        let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

        msg!("    total accounts for swap: {}", accounts.len());

        let swap_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        // let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        let instructions = [swap_ix];

        let _result = Core::execute_meteora_instruction(context, instructions.to_vec(), true)?;
        msg!("Swap {amount_in} {swap_for_y} {result:?}");

        Ok(())
    }

    /// Deposit tokens into the position (add liquidity)
    pub fn deposit(
        &self,
        context: &Context<Maint>,
        state: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
        active_id: i32
    ) -> Result<()> {
        let payer = context.accounts.irma_admin.clone();

        // let rpc_client = self.rpc_client();
        let lower_bin_id = active_id - (MAX_BIN_PER_ARRAY as i32).checked_div(2).unwrap();

        let upper_bin_id = lower_bin_id
            .checked_add(MAX_BIN_PER_ARRAY as i32)
            .unwrap()
            .checked_sub(1)
            .unwrap();

        let lower_bin_array_idx = BinArray::bin_id_to_bin_array_index(lower_bin_id)?;
        let upper_bin_array_idx = lower_bin_array_idx
            .checked_add(1)
            .unwrap();

        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        let mut instructions = vec![/* ComputeBudgetInstruction::set_compute_unit_limit(1_400_000) */];

        for idx in lower_bin_array_idx..=upper_bin_array_idx {
            // Initialize bin array if not exists
            let (bin_array, _bump) = derive_bin_array_pda(lb_pair, idx.into());

            if get_bytemuck_account::<BinArray>(context.remaining_accounts, &bin_array).is_none() {
                let accounts = dlmm::client::accounts::InitializeBinArray {
                    bin_array,
                    funder: payer.key(),
                    lb_pair,
                    system_program: system_program::ID,
                }
                .to_account_metas(None);

                let data = dlmm::client::args::InitializeBinArray { index: idx.into() }.data();

                let instruction = Instruction {
                    program_id: DLMM_ID,
                    accounts: accounts.to_vec(),
                    data,
                };

                instructions.push(instruction)
            }
        }

        // fake it for now; note position is a pubkey
        let position = *state.position_pks.first().ok_or(
                Error::from(CustomError::PositionNotFound)
            )?;

        let accounts = dlmm::client::accounts::InitializePosition {
            lb_pair,
            payer: payer.key(),
            position,
            owner: payer.key(),
            rent: rent::ID,
            system_program: system_program::ID,
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        let data = dlmm::client::args::InitializePosition {
            lower_bin_id,
            width: DEFAULT_BIN_PER_POSITION as i32,
        }
        .data();

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts: accounts.to_vec(),
            data,
        };

        instructions.push(instruction);

        // TODO implement bitmap extension fetching
        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
        // let bin_array_bitmap_extension = get_account(&bin_array_bitmap_extension)
        //     .map(|_| bin_array_bitmap_extension)
        //     .unwrap_or(DLMM_ID);

        let (bin_array_lower, _bump) = derive_bin_array_pda(lb_pair, lower_bin_array_idx.into());
        let (bin_array_upper, _bump) = derive_bin_array_pda(lb_pair, upper_bin_array_idx.into());

        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &lb_pair)?;
        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;

        let user_token_x = get_associated_token_address_with_program_id(
            &payer.key(),
            &lb_pair_state.token_x_mint,
            &token_x_program,
        );

        let user_token_y = get_associated_token_address_with_program_id(
            &payer.key(),
            &lb_pair_state.token_y_mint,
            &token_y_program,
        );

        let mut remaining_accounts_info = RemainingAccountsInfo { slices: vec![] };
        let mut remaining_accounts = vec![];

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                context.remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts.extend(transfer_hook_remaining_accounts);
        }

        remaining_accounts.extend(
            [bin_array_lower, bin_array_upper]
                .into_iter()
                .map(|k| AccountMeta::new(k, false)),
        );

        // fake it for now - position should not have changed
        // let position = *state.position_pks.first().ok_or(
        //         Error::from(CustomError::PositionNotFound)
        //     )?;

        let main_accounts = dlmm::client::accounts::AddLiquidityByStrategy2 {
            lb_pair,
            position, // pubkey for position
            bin_array_bitmap_extension: Some(bin_array_bitmap_extension),
            sender: payer.key(),
            event_authority,
            program: DLMM_ID,
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            user_token_x,
            user_token_y,
            token_x_program,
            token_y_program,
        }
        .to_account_metas(None);

        let data = dlmm::client::args::AddLiquidityByStrategy2 {
            liquidity_parameter: LiquidityParameterByStrategy {
                amount_x,
                amount_y,
                active_id: lb_pair_state.active_id,
                max_active_bin_slippage: 3,
                strategy_parameters: StrategyParameters {
                    min_bin_id: lower_bin_id,
                    max_bin_id: upper_bin_id,
                    strategy_type: StrategyType::SpotBalanced,
                    parameteres: [0u8; 64],
                },
            },
            remaining_accounts_info,
        }
        .data();

        let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        instructions.push(instruction);

        let _result = Core::execute_meteora_instruction(context, instructions, true)?;
        msg!("deposit {amount_x} {amount_y} {_result}");

        Ok(())
    }

    // Get the maximum depositable amount based on user's current token balance
    pub fn get_deposit_amount(
        &self,
        context: &Context<Maint>,
        position: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
    ) -> Result<(u64, u64)> {
        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &position.lb_pair)?;

        // let rpc_client = self.rpc_client();
        let payer = context.accounts.irma_admin.clone();

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;

        let user_token_x = get_associated_token_address_with_program_id(
            &payer.key(),
            &lb_pair_state.token_x_mint,
            &token_x_program,
        );

        let user_token_y = get_associated_token_address_with_program_id(
            &payer.key(),
            &lb_pair_state.token_y_mint,
            &token_y_program,
        );

        let accounts: HashMap<Pubkey, Option<TokenAccount>> 
                = Core::get_multiple_anchor_accounts(context, &vec![user_token_x, user_token_y])?;

        let user_token_x_state = accounts.get(&user_token_x).unwrap().as_ref().unwrap();
        let user_token_y_state = accounts.get(&user_token_y).unwrap().as_ref().unwrap();

        // compare with current balance
        let amount_x = if amount_x > user_token_x_state.amount {
            user_token_x_state.amount
        } else {
            amount_x
        };

        let amount_y = if amount_y > user_token_y_state.amount {
            user_token_y_state.amount
        } else {
            amount_y
        };

        Ok((amount_x, amount_y))
    }


    pub fn get_all_positions(&self) -> Vec<SinglePosition> {
        let state = &self.position_data;
        let mut positions = vec![];
        for position_entry in &state.all_positions {
            positions.push(position_entry.position.clone());
        }
        positions
    }

    pub fn get_all_tokens(&self) -> Vec<TokenEntry> {
        let state = &self.position_data;
        state.tokens.clone()
    }

    pub fn check_shift_price_range(
        &mut self,
        context: &mut Context<Maint>
    ) -> Result<()> {
        let all_positions = self.get_all_positions();
        for position in all_positions.iter() {
            let pair_config = get_pair_config(&self.config, position.lb_pair);
            // check whether out of price range
            let lb_pair = fetch_lb_pair_state(context.remaining_accounts, &position.lb_pair)?;
            if pair_config.mode == MarketMakingMode::ModeRight
                && lb_pair.active_id > position.max_bin_id
            {
                self.shift_right(context, &position)?;
                self.inc_rebalance_time(position.lb_pair);
            }

            if pair_config.mode == MarketMakingMode::ModeLeft
                && lb_pair.active_id < position.min_bin_id
            {
                self.shift_left(context, &position)?;
                self.inc_rebalance_time(position.lb_pair);
            }

            if pair_config.mode == MarketMakingMode::ModeBoth {
                if lb_pair.active_id < position.min_bin_id {
                    self.shift_left(context, &position)?;
                    self.inc_rebalance_time(position.lb_pair);
                } else if lb_pair.active_id > position.max_bin_id {
                    self.shift_right(context, &position)?;
                    self.inc_rebalance_time(position.lb_pair);
                }
            }
        }

        Ok(())
    }

    fn shift_right(
        &mut self,
        context: &mut Context<Maint>,
        state: &SinglePosition
    ) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        // validate that x amount is zero
        msg!("shift right {}", state.lb_pair);
        let position = state.get_positions_total(context.remaining_accounts)?;
        if position.amount_x != 0 {
            return Err(Error::from(CustomError::AmountXNotZero));
        }

        msg!("withdraw {}", state.lb_pair);
        // withdraw
        self.withdraw(context, state)?;

        // buy base
        let amount_y_for_buy = position
            .amount_y
            .checked_div(2)
            .unwrap();

        // let Some(lb_pair_state) = &state.lb_pair_state else {
        //     return Err(Error::from(CustomError::MissingLbPairState));
        // };
        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &state.lb_pair)?;

        let (amount_x, amount_y) = if amount_y_for_buy != 0 {
            msg!("swap {}", state.lb_pair);
            let swap_event = self.swap(context, state, amount_y_for_buy, false).unwrap();
            msg!("TODO: swap event processing {:?}", swap_event);
            (
                position.amount_x, // swap_event.map(|e| e.amount_out).unwrap_or_default(),
                position.amount_y - amount_y_for_buy
            )
        } else {
            (pair_config.x_amount, pair_config.y_amount)
        };

        // deposit again, just test with 1 position only
        msg!("deposit {}", state.lb_pair);
        match self
            .deposit(context, state, amount_x, amount_y, lb_pair_state.active_id)
        {
            Err(_) => {
                self.deposit(context, state, amount_x, amount_y, lb_pair_state.active_id)?;
            }
            _ => {}
        }
        msg!("refresh state {}", state.lb_pair);
        // fetch positions again (Note: token y is the reserve stablecoin)
        let reserves = &context.accounts.state.reserves;
        let remaining_accounts = context.remaining_accounts;
        let symbol = context.accounts.state.get_stablecoin_symbol(lb_pair_state.token_y_mint)
            .ok_or(Error::from(CustomError::ReserveNotFound))?
            .to_string();
        self.refresh_position_data(reserves, remaining_accounts, symbol)?;
        Ok(())
    }

    fn shift_left(
        &mut self,
        context: &mut Context<Maint>,
        state: &SinglePosition
    ) -> Result<()> {
        let pair_config = get_pair_config(&self.config, state.lb_pair);
        msg!("shift left {}", state.lb_pair);
        // validate that y amount is zero
        let position = state.get_positions_total(context.remaining_accounts)?;
        if position.amount_y != 0 {
            return Err(Error::from(CustomError::AmountYNotZero));
        }
        msg!("withdraw {}", state.lb_pair);
        // withdraw
        self.withdraw(context, state)?;

        // sell base
        let amount_x_for_sell = position
            .amount_x
            .checked_div(2)
            .unwrap();

        // let Some(lb_pair_state) = &state.lb_pair_state else {
        //     return Err(Error::from(CustomError::MissingLbPairState));
        // };
        let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &state.lb_pair)?;

        let (amount_x, amount_y) = if amount_x_for_sell != 0 {
                msg!("swap {}", state.lb_pair);
                let swap_event = self.swap(context, state, amount_x_for_sell, true).unwrap();
                msg!("TODO: swap event processing {:?}", swap_event);
                (
                    position.amount_x - amount_x_for_sell,
                    position.amount_y // swap_event.map(|e| e.amount_out).unwrap_or_default(),
                )
            } else {
                (pair_config.x_amount, pair_config.y_amount)
            };

        // sanity check with real balances
        let (amount_x, amount_y) = self.get_deposit_amount(context, state, amount_x, amount_y)?;
        msg!("deposit {}", state.lb_pair);
        match self
            .deposit(context, state, amount_x, amount_y, lb_pair_state.active_id)
        {
            Err(_) => {
                self.deposit(context, state, amount_x, amount_y, lb_pair_state.active_id)?;
            }
            _ => {}
        }

        msg!("refresh state {}", state.lb_pair);
        // fetch positions again (Note: token y is the reserve stablecoin)
        let reserves = &context.accounts.state.reserves;
        let remaining_accounts = context.remaining_accounts;

        let symbol = context.accounts.state.get_stablecoin_symbol(lb_pair_state.token_y_mint)
            .ok_or(Error::from(CustomError::ReserveNotFound))?
            .to_string();
        self.refresh_position_data(reserves, remaining_accounts, symbol)?;
        Ok(())
    }

    pub fn inc_rebalance_time(&mut self, lb_pair: Pubkey) {
        if let Some(state) = self.position_data.get_position_mut(&lb_pair) {
            state.inc_rebalance_time();
        }
    }

    /// Calculate total position amounts and fees for each position info, across all positions
    /// Note: this is not used anywhere and should probably be added to the main API.
    pub fn calc_all_positions(&self, context: &Context<Maint>) -> Result<Vec<PositionInfo>> {
        let all_positions = self.get_all_positions();
        let tokens = self.get_all_tokens();

        let mut position_infos = vec![];
        for position in all_positions.iter() {
            let lb_pair_state = fetch_lb_pair_state(context.remaining_accounts, &position.lb_pair)?;
            // Get decimals from token info
            let x_decimals = get_decimals(lb_pair_state.token_x_mint, &tokens);
            let y_decimals = get_decimals(lb_pair_state.token_y_mint, &tokens);
            
            // Now call get_positions_total which also fetches the data from DLMM
            let position_raw = position.get_positions_total(context.remaining_accounts)?;
            position_infos.push(position_raw.to_position_info(x_decimals, y_decimals)?);
        }
        return Ok(position_infos);
    }
}

