use commons::dlmm::accounts::*;
use commons::dlmm::types::*;
use commons::{
    BASIS_POINT_MAX,
    *,
};
use commons::{
    fetch_lb_pair_state,
    conversions::fetch_positions,
    get_matching_positions,
    get_bytemuck_account_ref,
    derive_event_authority_pda
};

use crate::position_manager::*;
use crate::pair_config::*;
use crate::pricing;
use crate::errors::CustomError;
use crate::IRMA_ID;
use crate::{Maint, StateMap, StableState};
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

const MAX_POSITIONS: usize = 3; // usually 2, but allow 3 during shifts
const MINTING_POSITION_AMOUNT: u64 = 1_100_000_000; // 1.1 billion units for minting positions
const REDEMPTION_POSITION_AMOUNT: u64 = 100_000_000; // 100 million units for redemption positions

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

    fn get_multiple_anchor_accounts<T: anchor_lang::AccountDeserialize + std::fmt::Debug>(
        remaining_accounts: &[AccountInfo],
        pubkeys: &Vec<Pubkey>
    ) -> Result<HashMap<Pubkey, Option<T>>> {
        msg!("==> get_multiple_anchor_accounts: {} pubkeys", pubkeys.len());
        let mut data = HashMap::new();
        for pubkey in pubkeys.iter() {
            let account_info = remaining_accounts.iter()
                .find(|acc| acc.key == pubkey);
            if let Some(account_info) = account_info {
                // Check if account has enough data
                if account_info.data.borrow().len() < 8 {
                    data.insert(*pubkey, None);
                    continue;
                }
                
                // Check discriminator
                let _discriminator = &account_info.data.borrow()[0..8];
                
                // For Mint accounts, we expect no discriminator (SPL Token accounts don't use discriminators)
                // Let's try to deserialize directly without skipping 8 bytes
                let borrowed_data = account_info.data.borrow();
                match T::try_deserialize(&mut &borrowed_data[..]) {
                    Ok(account_data) => {
                        data.insert(*pubkey, Some(account_data));
                    }
                    Err(_error) => {
                        // Also try skipping discriminator in case it's an Anchor account
                        if borrowed_data.len() > 8 {
                            match T::try_deserialize(&mut &borrowed_data[8..]) {
                                Ok(account_data) => {
                                    data.insert(*pubkey, Some(account_data));
                                }
                                Err(_error) => {
                                    data.insert(*pubkey, None);
                                }
                            }
                        } else {
                            data.insert(*pubkey, None);
                        }
                    }
                }
            } else {
                data.insert(*pubkey, None);
            }
        }
        msg!("==> Finished get_multiple_anchor_accounts");
        Ok(data)
    }

    fn execute_meteora_instruction(
        payer: &mut Signer,
        remaining_accounts: &[AccountInfo],
        instructions: Vec<Instruction>,
        sign: bool
    ) -> Result<()> {
        let key = payer.key();
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
                invoke_signed(&instruction, remaining_accounts, &[seeds])?;
            }
            else {
                invoke(&instruction, remaining_accounts)?;
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
    pub fn fetch_token_info<'a>(&mut self, remaining_accounts: &'a [AccountInfo<'a>]) -> Result<()> {
        let token_mints_with_program: Vec<(Pubkey, Pubkey)> = 
            self.get_all_token_mints_with_program_id(remaining_accounts)?;

        let token_keys = token_mints_with_program
            .iter()
            .map(|(key, _program_id)| *key)
            .collect::<Vec<Pubkey>>();

        let accounts: HashMap<Pubkey, Option<Mint>> = Core::get_multiple_anchor_accounts::<Mint>(
            remaining_accounts, &token_keys)?;

        msg!("==> Mints count: {}", accounts.len());

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

    fn get_all_token_mints_with_program_id<'a>(
        &self,
        remaining_accounts: &'a [AccountInfo<'a>]
    ) -> Result<Vec<(Pubkey, Pubkey)>> {
        let state = &self.position_data;
        let mut token_mints_with_program = vec![];

        for position_entry in state.all_positions.iter() {
            let lb_pair_state = fetch_lb_pair_state(
                remaining_accounts, &position_entry.lb_pair
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

    pub fn get_mut_position_state(&mut self, lp_pair: Pubkey) -> &mut SinglePosition {
        let state = &mut self.position_data;
        let position = state.get_position_mut(&lp_pair).unwrap();
        position
    }

    // Helper function to get or create associated token account (ATA) on-chain
    fn get_or_create_ata(
        &self,
        remaining_accounts: &[AccountInfo],
        token_mint: Pubkey,
        token_program: Pubkey,
        owner: &Pubkey,
        payer: &mut Signer,
    ) -> Result<Pubkey> {
        let ata_address = get_associated_token_address_with_program_id(
            owner,
            &token_mint,
            &token_program,
        );

        // Check if ATA already exists in remaining_accounts
        let ata_exists = remaining_accounts.iter()
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
            Core::execute_meteora_instruction(payer, remaining_accounts, vec![create_ata_ix], true)?;
        }

        Ok(ata_address)
    }

    /// Initialize user associated token accounts for all tokens in the position
    pub fn init_user_ata<'a>(
        &self, 
        wallet: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
    ) -> Result<()> {
        for (token_mint, program_id) in self.get_all_token_mints_with_program_id(
            remaining_accounts
        )?.iter() {
            self.get_or_create_ata(
                remaining_accounts,
                *token_mint,
                *program_id,
                &wallet.key(),
                wallet,
            )?;
        }

        Ok(())
    }

    /// Withdraw a position and close it in a single transaction.
    /// Liquidity must have been deposited to the new price single-bin position, so the old 
    /// single-bin position can be safely withdrawn and closed.
    pub fn withdraw<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        state: &SinglePosition,
        old_position_key: Pubkey, // we should not withdraw from the new position
    ) -> Result<()> {
        if state.position_pks.len() == 0 {
            return Ok(());
        }

        let (event_authority, _bump) = derive_event_authority_pda();

        let lb_pair = state.lb_pair;
        let lb_pair_state = fetch_lb_pair_state(remaining_accounts_in, &state.lb_pair)?;

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;

        let mut remaining_account_info = RemainingAccountsInfo { slices: vec![] };
        let mut transfer_hook_remaining_accounts = vec![];

        if let Some((slices, remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                remaining_accounts_in,
                ActionType::Liquidity,
            )?
        {
            remaining_account_info.slices = slices;
            transfer_hook_remaining_accounts = remaining_accounts;
        }

        let vec_positions = fetch_positions(remaining_accounts_in, &[old_position_key])?;
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
            position: old_position_key,
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
            position: old_position_key,
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

        // Close single bin position
        let accounts = dlmm::client::accounts::ClosePosition2 {
            position: old_position_key,
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

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, instructions, true)?;
        msg!("Close old_position_key {old_position_key} {result}");

        Ok(())
    }



    /// Swap tokens on the DLMM.
    /// We may need this to overcome AMM behavior, in case off-chain swap is too slow.
    /// If not, according to Taha, we can use withdraw() above instead.
    pub fn swap<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool
    ) -> Result<()> {

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &state.lb_pair)?;

        msg!("==> Swapping on pair: {}", state.lb_pair);

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        msg!("    event authority: {}", event_authority);

        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);

        let bitmap_extension: &BinArrayBitmapExtension;
        let default_bitmap_extension = BinArrayBitmapExtension::default();
        let acct_info = remaining_accounts.iter()
            .find(|acc| acc.key == &bin_array_bitmap_extension);
        if let Some(acct_info) = acct_info {
            bitmap_extension = match get_bytemuck_account_ref::<BinArrayBitmapExtension>(acct_info) {
                Some(bitmap_ext) => bitmap_ext,
                None => &default_bitmap_extension,
            };
        } else {
            bitmap_extension = &default_bitmap_extension;
        }

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
        let mut remaining_accounts_vec = vec![];

        msg!("    Preparing Token 2022 related accounts...");

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts_vec.extend(transfer_hook_remaining_accounts);
        }

        msg!("    transfer hook remaining accounts: {}", remaining_accounts_vec.len());

        remaining_accounts_vec.extend(bin_arrays_account_meta);

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

        let accounts = [main_accounts.to_vec(), remaining_accounts_vec].concat();

        msg!("    total accounts for swap: {}", accounts.len());

        let swap_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        // let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        let instructions = [swap_ix];

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts, instructions.to_vec(), true)?;
        msg!("Swap {amount_in} {swap_for_y} {result:?}");

        Ok(())
    }

    /// Deposit tokens into the position (add liquidity)
    /// At any given time, we have at most two single-bin positions per pair
    /// (one for each side of the stablecoin pair).
    /// This function deposits liquidity into one of those positions.
    /// This is not about changing the price. It's all about adding liquidity.
    /// Note that "SinglePosition" here refers to the position state for a given LbPair,
    /// which may contain one or two actual position data accounts.
    /// 1. Initialize bin arrays if they do not exist.
    /// 2. Initialize position if it does not exist. (For IRMA, it should not exist yet.)
    /// 3. Add liquidity to the position.
    /// 4. Output the position pubkey for reference.
    pub fn deposit<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &mut SinglePosition,
        amount_x: u64, // must zero if amount_y > 0 and vice versa
        amount_y: u64, // must zero if amount_x > 0 and vice versa
        new_price_bin_id: i32 // this is not the lb_pair active bin id; this is the bin we want to deposit to
    ) -> Result<Pubkey> {
        // enforce exclusive OR condition
        require!(
            (amount_x == 0) != (amount_y == 0),
            CustomError::InvalidDepositAmounts
        );

        // for IRMA, the lower bin id is always equal to the upper bin id
        // since we only provide liquidity in one bin at any time;
        // we don't really care where the market is, so we don't use active_id.
        let bin_array_idx = BinArray::bin_id_to_bin_array_index(new_price_bin_id)?;

        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        let mut instructions = vec![/* ComputeBudgetInstruction::set_compute_unit_limit(1_400_000) */];

        // Initialize bin array if not exists
        let (bin_array, _bump) = derive_bin_array_pda(lb_pair, bin_array_idx.into());

        let dummy_pubkey = Pubkey::default();

        let bin_array_instance = BinArray {
            lb_pair: dummy_pubkey,
            version: 0u8,
            index: 0i64,
            bins: [Bin::default(); 70],
            _padding: [0u8; 7],
        };

        let bin_array_ref: &BinArray;
        let acct_info = remaining_accounts.iter()
            .find(|acc| acc.key == &bin_array);
        if let Some(acct_info) = acct_info {
            bin_array_ref = match get_bytemuck_account_ref::<BinArray>(acct_info) {
                Some(bin_array_state) => bin_array_state,
                None => &bin_array_instance,
            };
        } else {
            bin_array_ref = &bin_array_instance;
        }

        if bin_array_ref.lb_pair == dummy_pubkey {
            let accounts = dlmm::client::accounts::InitializeBinArray {
                bin_array,
                funder: payer.key(),
                lb_pair,
                system_program: system_program::ID,
            }
            .to_account_metas(None);

            let data = dlmm::client::args::InitializeBinArray { index: bin_array_idx.into() }.data();

            let instruction = Instruction {
                program_id: DLMM_ID,
                accounts: accounts.to_vec(),
                data,
            };

            instructions.push(instruction)
        }

        if state.position_pks.len() > 2 {
            return Err(error!(CustomError::TooManyPositionsForPair));
        }

        // we only have two positions per pair at any time
        // and we need to determine which one to deposit into 
        // let position = *state.position_pks.first().ok_or(
        //         Error::from(CustomError::PositionNotFound)
        //     )?;

        // Initialize new position
        let (position, _bump) = derive_position_pda(
            payer.key(),
            lb_pair,
            new_price_bin_id,
            new_price_bin_id,
        );

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
            lower_bin_id: new_price_bin_id,
            width: 1i32, // single bin position
        }
        .data();

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts: accounts.to_vec(),
            data,
        };

        instructions.push(instruction);

        // TODO implement bitmap extension fetching
        let bin_array_bitmap_extension = None;
        // let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
        // let bin_array_bitmap_extension = get_account(&bin_array_bitmap_extension)
        //     .map(|_| bin_array_bitmap_extension)
        //     .unwrap_or(DLMM_ID);

        let (bin_array, _bump) = derive_bin_array_pda(lb_pair, bin_array_idx.into());

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &lb_pair)?;
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
        let mut remaining_accounts_vec = vec![];

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts_vec.extend(transfer_hook_remaining_accounts);
        }

        remaining_accounts_vec.extend(
            [bin_array, position]
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
            bin_array_bitmap_extension,
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
                active_id: lb_pair_state.active_id, // current market price bin id
                max_active_bin_slippage: 3,
                strategy_parameters: StrategyParameters {
                    min_bin_id: new_price_bin_id,
                    max_bin_id: new_price_bin_id,
                    strategy_type: StrategyType::SpotBalanced,
                    parameteres: [0u8; 64],
                },
            },
            remaining_accounts_info,
        }
        .data();

        let accounts = [main_accounts.to_vec(), remaining_accounts_vec].concat();

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        instructions.push(instruction);

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts, instructions, true)?;
        msg!("deposit {amount_x} {amount_y} {_result}");

        state.position_pks.push(position);
        let bin_id = state.max_bin_id;
        state.max_bin_id = new_price_bin_id;
        // if previously there was only one position, min_bin_id == max_bin_id,
        // keep it that way
        if state.min_bin_id == bin_id {
            state.min_bin_id = new_price_bin_id;
        }

        Ok(position)
    }

    /// get_deposit_amount:
    /// Get the maximum depositable amount based on user's current token balance.
    /// Do we need this routine? Maybe not.
    pub fn get_deposit_amount<'a>(
        &self,
        context: &'a Context<'a, 'a, 'a, 'a, Maint<'a>>,
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
                = Core::get_multiple_anchor_accounts::<TokenAccount>(
                    context.remaining_accounts, &vec![user_token_x, user_token_y])?;

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

    /// get_all_positions:
    /// We should have at most only two positions per lb_pair at any one time.
    /// At the beginning of a  new pair, there will be only one position.
    /// The first swap that arrives will cause the buyback position to be created.
    /// After that, both positions will be rebalanced as needed.
    pub fn get_all_positions(&self) -> Vec<SinglePosition> {
        let state = &self.position_data;
        let mut positions = vec![];
        for position_entry in &state.all_positions {
            positions.push(position_entry.clone());
        }
        positions
    }

    pub fn get_all_tokens(&self) -> Vec<TokenEntry> {
        let state = &self.position_data;
        state.tokens.clone()
    }

    /// For each reserve coin, check how far the two current prices are from those set by pricing.rs
    /// If the difference is at least a bin away, we shift to another bin.
    /// Note that each position in IRMA is single-sided and single-bin.
    /// In other words, min_bin_id == max_bin_id for each position, and 
    /// there are two positions: one for each side of the stablecoin pair.
    pub fn check_shift_price_range<'a>(
        core: &mut Core,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        reserves: &Vec<StableState>,
        core_position: &mut SinglePosition,
    ) -> Result<()> {
        // ensure that this position is single-bin
        require!(core_position.min_bin_id == core_position.max_bin_id, CustomError::PositionNotSingleBin);

        // Find the reserve coin for this position
        let (reserve_symbol, backing_decimals) = {
            let reserve_coin = reserves.iter().find(|stablecoin| stablecoin.pool_id == core_position.lb_pair);
            require!(reserve_coin.is_some(), CustomError::ReserveListPositionListMismatch);
            let reserve_coin = reserve_coin.unwrap();
            (reserve_coin.symbol.clone(), reserve_coin.backing_decimals)
        };
        
        let (mint_price, redemption_price) = pricing::get_prices(
            reserves, &reserve_symbol)?;

        // convert prices from f64 to u128 using token decimals
        let mint_price_u128 = (mint_price * 10.0f64.powi(backing_decimals as i32)) as u128;
        let redemption_price_u128 = (redemption_price * 10.0f64.powi(backing_decimals as i32)) as u128;

        let lb_pair_state = fetch_lb_pair_state(
            remaining_accounts, 
            &core_position.lb_pair
        )?;
        let mut mint_price_bin_id = SinglePosition::search_bin_given_price(&lb_pair_state, mint_price_u128)?;
        let redemption_price_bin_id = SinglePosition::search_bin_given_price(&lb_pair_state, redemption_price_u128)?;
        // ensure that mint bin id is higher than redemption bin id
        if mint_price_bin_id <= redemption_price_bin_id {
            // adjust mint price bin id by one to ensure they are different
            mint_price_bin_id = redemption_price_bin_id.saturating_add(1);
        }
        
        // check whether out of price range
        if mint_price_bin_id != core_position.max_bin_id {
            core.shift_mint_position(payer, remaining_accounts, reserves, core_position, mint_price_bin_id)?;
            core.inc_rebalance_time(core_position.lb_pair);
        }
        // else if equal, it's ok, do nothing

        if redemption_price_bin_id != core_position.min_bin_id {
            core.shift_redeem_position(payer, remaining_accounts, reserves, core_position, redemption_price_bin_id)?;
            core.inc_rebalance_time(core_position.lb_pair);
        }
        // else if equal, it's ok, do nothing

        Ok(())
    }


    /// Shift mint position
    /// For IRMA, we should deposit first, then withdraw from the old, single bin position.
    /// Note: this can involve shifting to the right or left, depending on the new_price_bin_id.
    fn shift_mint_position<'a>(
        &mut self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        reserves: &Vec<StableState>,
        state: &mut SinglePosition,
        new_price_bin_id: i32, // new mint price bin id
    ) -> Result<()> {
        // validate that y amount is zero because this position must be for x:
        // there should be no y deposit in any position
        msg!("shift mint position {}", state.lb_pair);
        let position_raw = state.get_positions_total(remaining_accounts)?;
        let amount_y = position_raw.amount_y;
        if amount_y != 0 {
            return Err(Error::from(CustomError::AmountYNotZero));
        }

        // retry if error, amount_y should be zero
        // this also creates a new position and returns its key
        msg!("mint deposit for {}", state.lb_pair);
        let new_position_key = match self
            .deposit(payer, remaining_accounts, state, MINTING_POSITION_AMOUNT, amount_y, new_price_bin_id)
        {
            Err(_) => {
                self.deposit(payer, remaining_accounts, state, MINTING_POSITION_AMOUNT, amount_y, new_price_bin_id)?
            }
            Ok(pos_key) => pos_key,
        };

        msg!("redemption position created: {}", new_position_key.to_string());

        if position_raw != PositionRaw::default() {
    
            let positions = fetch_positions(remaining_accounts, &state.position_pks)?;

            // withdraw from previous position
            for (i, pos) in positions.iter().enumerate() {
                if pos.lower_bin_id == position_raw.min_bin_id {
                    let poskey = state.position_pks[i];
                    msg!("mint position {} withdraw", poskey.to_string());
                    self.withdraw(payer, remaining_accounts, state, poskey)?;
                    break;
                }
            }
        }

        msg!("refresh state {}", state.lb_pair);
        // fetch positions again (Note: token y is the reserve stablecoin)

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &state.lb_pair)?;

        let stablecoin = reserves.iter().find(|r| r.mint_address == lb_pair_state.token_y_mint)
            .ok_or(Error::from(CustomError::ReserveNotFound))?;
        let symbol = stablecoin.symbol.to_string();
        self.refresh_position_data(reserves, remaining_accounts, symbol)?;
        Ok(())
    }


    /// Shift redeem position
    /// For IRMA, we deposit first, then withdraw from the old bin.
    fn shift_redeem_position<'a>(
        &mut self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        reserves: &Vec<StableState>,
        state: &mut SinglePosition,
        new_price_bin_id: i32, // new redemption price bin id
    ) -> Result<()> {
        // let pair_config = get_pair_config(&self.config, state.lb_pair);
        msg!("shift redeem position {}", state.lb_pair);

        // validate that x amount is zero
        let position_raw = state.get_positions_total(remaining_accounts)?;
        if position_raw.amount_x != 0 {
            return Err(Error::from(CustomError::AmountXNotZero));
        }

        // sanity check with real balances
        // let (amount_x, amount_y) = self.get_deposit_amount(context, state, amount_x, amount_y)?;
        msg!("redemption deposit for {}", state.lb_pair);
        let new_position_key = match self
            .deposit(payer, remaining_accounts, state, 0, REDEMPTION_POSITION_AMOUNT, new_price_bin_id)
        {
            Err(_) => {
                self.deposit(payer, remaining_accounts, state, 0, REDEMPTION_POSITION_AMOUNT, new_price_bin_id)?
            }
            Ok(pos_key) => pos_key,
        };

        msg!("redemption position created: {}", new_position_key.to_string());

        if position_raw != PositionRaw::default() {
    
            let positions = fetch_positions(remaining_accounts, &state.position_pks)?;

            // withdraw from previous position
            for (i, pos) in positions.iter().enumerate() {
                if pos.lower_bin_id == position_raw.min_bin_id {
                    let poskey = state.position_pks[i];
                    msg!("mint position {} withdraw", poskey.to_string());
                    self.withdraw(payer, remaining_accounts, state, poskey)?;
                    break;
                }
            }
        }

        msg!("refresh state {}", state.lb_pair);
        // fetch positions again (Note: token y is the reserve stablecoin)

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &state.lb_pair)?;

        let stablecoin = reserves.iter().find(|r| r.mint_address == lb_pair_state.token_y_mint)
            .ok_or(Error::from(CustomError::ReserveNotFound))?;
        let symbol = stablecoin.symbol.to_string();
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
    /// (Renamed from its original misnomer "get_positions")
    pub fn calc_all_positions<'a>(&self, context: &Context<'a, 'a, 'a, 'a, Maint<'a>>) -> Result<Vec<PositionInfo>> {
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

