use commons::dlmm::accounts::*;
use commons::dlmm::types::*;
use commons::{
    BASIS_POINT_MAX,
    *,
};
use commons::{
    fetch_lb_pair_state, get_bytemuck_account,
    conversions::fetch_positions,
    get_bytemuck_account_ref,
    get_matching_positions,
    derive_event_authority_pda,
    price_math::get_price_from_id,
};
// use commons::u64x64_math::pow;

use crate::position_manager::*;
use crate::pair_config::*;
use crate::pricing;
use crate::errors::CustomError;
use crate::{Maint, StateMap, StableState};
use std::collections::HashMap;
use std::str::FromStr;

use anchor_lang::prelude::*;
use anchor_lang::prelude::instruction::Instruction;
use anchor_lang::solana_program::{
    clock::Clock,
    program::invoke,
    sysvar::Sysvar,
    // system_instruction,
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

const MAX_POSITIONS: usize = 2; // allow only 2 positions per pair
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
        _payer: &mut Signer,
        remaining_accounts: &[AccountInfo],
        instructions: Vec<Instruction>,
    ) -> Result<()> {
        // Pre-validate that all required accounts are available
        // Use iterator instead of collecting into Vec to save memory
        for instruction in instructions.iter() {
            for account_meta in &instruction.accounts {
                if remaining_accounts.iter().all(|acc| acc.key != &account_meta.pubkey) {
                    msg!("Missing required account: {}", account_meta.pubkey);
                    return Err(error!(CustomError::MissingRequiredAccount));
                }
            }
        }
        
        for instruction in instructions.iter() {
            // All DLMM operations should be called without program signing
            // Required signers (user, position keypairs) should be provided by the client
            // msg!("Invoking DLMM instruction without program signing");
            invoke(&instruction, remaining_accounts)?;
        }
        Ok(())
    }


    /// Refresh internal state by fetching positions and bin arrays using provided accounts
    pub fn refresh_position_data_with_accounts<'a>(
        &self, // must be immutable
        state: &mut Account<StateMap>,
        pools: &mut Vec<SinglePosition>, // SinglePosition pertains to pool (LBPair)
        remaining_accounts: &'a [AccountInfo<'a>],
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

        if pools.len() != 1 {
            return Err(error!(CustomError::InconsistentPositionsFound));
        }
        let pair = &mut pools[0];
        msg!("==> position_pks.len(): {}", pair.position_pks.len());

        // NO refresh_position_data() because this is done already in check_shift_price_ranges()
        // let lb_pair_state = fetch_lb_pair_state(
        //     remaining_accounts, &pair.lb_pair
        // )?; // .ok_or(error!(CustomError::MissingLbPairState))?;
        // let owner = &lb_pair_state.creator;

        // // Call the core pair refresh logic without needing a full context
        // self.refresh_position_data(owner, remaining_accounts, pair, is_sale)?;
        
        Ok(())
    }
    
    /// Refresh internal state by fetching positions and bin arrays.
    /// This function is called from sale_trade_event and buy_trade_event.
    /// Instead of parameters, it uses context to fetch necessary accounts
    /// and the config Vec in Core to go through the pairs.
    pub fn refresh_position_data<'a>(
        &self, // must be immutable
        owner: &Pubkey, // owner of the positions is also the creator of the pool
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &mut SinglePosition, // for particular lb_pair with this token
        is_sale: bool,
    ) -> Result<()> {
        msg!("==> Refreshing: {}", is_sale);
        // all_positions contains SinglePosition entries, one for each reserve stablecoin.
        // One of these therefore represents the current mint or redemption SinglePosition.
        // The sequence number of the position in all_positions should be the same

        // minting should always come first, so this is when we clear all existing keys
        if is_sale {
            state.position_pks.clear();
            state.bin_array_pks.clear();
        }

        let pair_address = state.lb_pair; // DLMM LbPair address

        // msg!("==> Refreshing state for owner: {}", owner.to_string());

        // get all DLMM PositionV2's by the same user, for this trade pair.
        // there should only be two positions, at most - one for minting, one for redemption
        let mut position_keys_with_states = get_matching_positions(
            remaining_accounts,
            owner, // must be owner of SinglePosition position_pks
            &pair_address
        ).or_else(|error| Err(error)).unwrap();

        require!(position_keys_with_states.len() <= MAX_POSITIONS, CustomError::TooManyPositions);

        // let mut position_pks: Vec<&Pubkey> = vec![];
        // Note: We'll fetch PositionV2 positions and bin_arrays dynamically when needed
        // let mut positions = vec![];
        let mut min_bin_id = 0;
        let mut max_bin_id = 0;
        // let mut bin_arrays_vec = Vec::<(Pubkey, BinArray)>::new();

        // msg!("    Found {} PositionV2 positions", position_keys_with_states.len());
        if position_keys_with_states.len() > 0 {
            // sort position by bin id
            position_keys_with_states
                .sort_by(|(_, a), (_, b)| a.lower_bin_id.cmp(&b.lower_bin_id));

            min_bin_id = position_keys_with_states
                .first()
                .map(|(_key, state)| state.lower_bin_id)
                .unwrap();

            max_bin_id = position_keys_with_states
                .last()
                .map(|(_key, state)| state.upper_bin_id)
                .unwrap();

            // msg!("    PositionV2 bin id range: {} - {}", min_bin_id, max_bin_id);

            if is_sale { // mint, we just cleared the position_pks
                    state.position_pks.push(*position_keys_with_states[0].0);
            }
            else { // redemptions
                if state.position_pks.len() == 0 {
                    return Err(error!(CustomError::MintPositionNotFound));
                }
                if state.position_pks.len() == 1 {
                    state.position_pks.push(*position_keys_with_states[0].0);
                }
                else if state.position_pks.len() >= 2 {
                    state.position_pks[1] = *position_keys_with_states[0].0;
                }
            }
            // for (key, _state) in position_keys_with_states.iter() {
            //     state.position_pks.push(**key);
            //     // Don't store the position data - fetch dynamically when needed
            //     // positions.push(state.to_owned());
            // }

            // msg!("    Total position_pks count: {}", state.position_pks.len());

            let pos_v2 = match position_keys_with_states.len() {
                1 => match is_sale {
                    true => &mut position_keys_with_states[0].1, // mint position comes first
                    false => return Err(error!(CustomError::AdditionalPositionRequired)), // only one position, and it's not the one we want
                },
                2 => match is_sale {
                    true => &mut position_keys_with_states[0].1,
                    false => &mut position_keys_with_states[1].1, // second one is redemption
                },
                _ => return Err(error!(CustomError::InconsistentPositionsFound)),
            };
            // msg!("    Selected PositionV2 with bin range: {} - {}", 
            //     pos_v2.lower_bin_id, pos_v2.upper_bin_id);

            // from here on we should have to deal only with a single PositionV2
            let _ = pos_v2.get_bin_array_keys_coverage(&mut state.bin_array_pks)?;

        }
        // msg!("   bin array keys count: {}", state.bin_array_pks.len());

        state.lb_pair = pair_address;
        // Don't store non-serializable types - they will be fetched dynamically
        // state.bin_arrays = bin_arrays_vec; // fetch dynamically
        // state.positions = positions; // fetch dynamically
        // state.bin_array_pks = bin_array_keys; // already done, see above
        // state.position_pks = position_pks.iter().map(|k| **k).collect(); // already done above
        state.min_bin_id = min_bin_id;
        state.max_bin_id = max_bin_id;
        state.last_update_timestamp = Self::get_epoch_sec()?.max(0) as u64;
        state.inc_rebalance_time();

        Ok(())
    }

    /// Fetch token info for all tokens in the positions
    pub fn fetch_token_info<'a>(&mut self, remaining_accounts: &'a [AccountInfo<'a>]) -> Result<()> {
        msg!("==> Fetching token info for all tokens in positions");
        let token_mints_with_program: Vec<(Pubkey, Pubkey)> = 
            self.get_all_token_mints_with_program_id(remaining_accounts)?;

        let token_keys = token_mints_with_program
            .iter()
            .map(|(key, _program_id)| *key)
            .collect::<Vec<Pubkey>>();
        msg!("==> Token keys count: {}", token_keys.len());

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
            msg!("    Fetching token mints for position on pair: {}, rem count: {}", 
                position_entry.lb_pair, remaining_accounts.len());
            if remaining_accounts.iter().all(|acc| acc.key != &position_entry.lb_pair) {
                msg!("    Missing LB pair state for position on pair");
                continue;
            }
            let lb_pair_state = fetch_lb_pair_state(
                remaining_accounts, &position_entry.lb_pair
            )?; // .ok_or(error!(CustomError::MissingLbPairState))?;
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

            msg!("Creating ATA: {}", ata_address);
            // Execute the instruction
            Core::execute_meteora_instruction(payer, remaining_accounts, vec![create_ata_ix])?;
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
        state: &mut SinglePosition,
        old_position_key: Pubkey, // we should not withdraw from the new position
    ) -> Result<()> {
        if state.position_pks.len() == 0 {
            return Ok(());
        }
        msg!("==> Withdrawing and closing old position: {}", old_position_key);

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

        // Check who owns this position to determine the correct authority
        let position_state = vec_positions
            .get(0)
            .ok_or(error!(CustomError::PositionNotFound))?;
            
        // Determine the correct sender/authority based on position ownership
        let position_owner = position_state.owner;
        // let is_pda_owned = {
        //     let (irma_authority, _) = Pubkey::find_program_address(
        //         &[b"irma_authority"],
        //         &IRMA_ID,
        //     );
        //     position_owner == irma_authority
        // };

        // msg!("Position owner: {}, is PDA owned: {}", position_owner, is_pda_owned);

        let main_accounts = dlmm::client::accounts::RemoveLiquidityByRange2 {
            position: old_position_key,
            lb_pair,
            bin_array_bitmap_extension: None,
            user_token_x,
            user_token_y,
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            sender: position_owner,
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

        let mut instructions = vec![];

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

        // msg!("    Executing withdraw and close instructions...");

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, instructions)?;
        // msg!("Close old_position_key: {}, result: {:?}", old_position_key, result);

        Ok(())
    }



    /// Swap tokens on the DLMM.
    /// We may need this to overcome AMM behavior, in case off-chain swap is too slow.
    /// If not, according to Taha, we can use withdraw() above instead.
    pub fn swap<'a>(
        &self,
        payer: &mut Signer<'a>,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool
    ) -> Result<()> {

        msg!("==> Swapping on pair: {}", state.lb_pair);

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &state.lb_pair)?;

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        msg!("    event authority: {}", event_authority);

        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair, &dlmm::ID);

        // let accounts = dlmm::client::accounts::InitializeBinArrayBitmapExtension {
        //     lb_pair,
        //     bin_array_bitmap_extension,
        //     program: DLMM_ID,
        // }
        // .to_account_metas(None);

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

        // let bitmap_extension = get_bytemuck_account::<BinArrayBitmapExtension>(
        //     remaining_accounts,
        //     &bin_array_bitmap_extension,
        // );
        // msg!("    bin array bitmap extension: {:?}", bitmap_extension);

        let bin_arrays_account_meta = get_bin_array_pubkeys_for_swap(
            lb_pair,
            &lb_pair_state,
            Some(bitmap_extension),
            swap_for_y,
            3,
        )?
        .into_iter()
        .map(|key| AccountMeta::new(key, false))
        .collect::<Vec<_>>();

        msg!("    bin arrays account meta: {:?}", bin_arrays_account_meta[0]);

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
        msg!("    user token in: {}", user_token_in);
        msg!("    user token out: {}", user_token_out);

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
            bin_array_bitmap_extension: None, // bitmap_extension.lb_pair),
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

        // msg!("    main accounts.token_y_mint: {:?}", main_accounts.token_y_mint);

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

        msg!("    Executing swap instruction...");

        let result = Core::execute_meteora_instruction(payer, remaining_accounts, instructions.to_vec())?;
        msg!("Swap amount_in: {}, swap_for_y: {}, result: {:?}", amount_in, swap_for_y, result);

        Ok(())
    }

    /// Deposit tokens into the position (add liquidity)
    /// At any given time, we have at most two single-bin positions per pair
    /// (one for each side of the stablecoin pair).
    /// This function deposits liquidity into one of those positions.
    /// This changes the price. It also adds liquidity.
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
        state: &mut SinglePosition, // modify only position_pks, bin_array_pks
        amount_x: u64, // must be zero if amount_y > 0 and vice versa
        amount_y: u64, // must be zero if amount_x > 0 and vice versa
        new_price_bin_id: i32, // this is not the lb_pair active bin id; this is the bin we want to deposit to
        position: &'a Pubkey, // position account to initialize and deposit into
    ) -> Result<&'a Pubkey> {
        msg!("==> Depositing liquidity into position for bin: {}", new_price_bin_id);
        // enforce exclusive OR condition
        require!(
            (amount_x == 0) != (amount_y == 0),
            CustomError::InvalidDepositAmounts
        );

        if state.position_pks.len() > 2 {
            return Err(error!(CustomError::TooManyPositionsForPair));
        }

        // for IRMA, the lower bin id is always equal to the upper bin id
        // since we only provide liquidity in one bin at any time;
        // we don't really care where the market is, so we don't use active_id.
        let bin_array_idx = BinArray::bin_id_to_bin_array_index(new_price_bin_id)?;

        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        // Initialize bin array where the bin belongs, if not exists
        let (bin_array, _bump) = derive_bin_array_pda(lb_pair, bin_array_idx.into());

        // msg!("    Checking bin array at index: {}", bin_array_idx);

        // it's possible that the new price bin is within the current bin array.
        let acct_info = remaining_accounts.iter()
            .find(|acc| acc.key == &bin_array);
        // if let Some(acct_info) = acct_info {
        //     let _bin_array_ref = match get_bytemuck_account_ref::<BinArray>(acct_info) {
        //         Some(bin_array_state) => Some(bin_array_state),
        //         None => None, // Err(error!(CustomError::InvalidBinArrayState))?,
        //     };
        // }
        // msg!("    Bin array account: {}", bin_array.to_string());
        require!(
            !acct_info.is_none(),
            CustomError::MissingBinArrayState
        );
        if !state.bin_array_pks.contains(&bin_array) {
            // msg!("    Bin array account not found, initializing...");
            let accounts = dlmm::client::accounts::InitializeBinArray {
                bin_array, // derived
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
            // msg!("    Initializing bin array {}.", bin_array);

            let _result = Core::execute_meteora_instruction(payer, remaining_accounts, vec![instruction])?;
            // msg!("    Bin array initialized");
        }
        // else {
        //     msg!("    Bin array already exists, skipping initialization.");
        // }

        // no matter what, we need to create a new DLMM position because price has changed.

        // don't push into bin_array_pks yet, do this during refresh
        // state.bin_array_pks.push(bin_array);

        // msg!("    BinArray updated, initializing new position...");

        let acct_info = remaining_accounts.iter()
            .find(|acc| acc.key == position);
        // msg!("    Position account: {}", position.to_string());
        require!(
            !acct_info.is_none(),
            CustomError::MissingPositionState
        );

        if !state.position_pks.contains(&position) {

            let accounts = dlmm::client::accounts::InitializePosition {
                payer: payer.key(), // Base for the PDA derivation
                position: *position, // derived
                lb_pair,
                owner: payer.key(), // User owns the position (for compatibility)
                system_program: system_program::ID,
                rent: rent::ID,
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
            // msg!("    Initializing position: {}", position);

            // DLMM program handles PDA creation and signing internally
            // No program signing needed from IRMA side
            let _result = Core::execute_meteora_instruction(payer, remaining_accounts, vec![instruction])?;
            // msg!("    Position initialized: {}", position);
        }
        // else {
        //     msg!("    Position account exists, skipping initialization.");
        // }

        // TODO implement bitmap extension fetching
        let bin_array_bitmap_extension = None;
        // let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
        // let bin_array_bitmap_extension = get_account(&bin_array_bitmap_extension)
        //     .map(|_| bin_array_bitmap_extension)
        //     .unwrap_or(DLMM_ID);
        let account_info = if let Some(acc) = remaining_accounts.iter().find(|acc| acc.key == &lb_pair) {
            acc
        } else {
            return Err(error!(CustomError::MissingLbPairState));
        };

        let lb_pair_state = get_bytemuck_account_ref::<LbPair>(account_info)
            .ok_or(error!(CustomError::MissingLbPairState))?;
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
            [bin_array, *position]
                .into_iter()
                .map(|k| AccountMeta::new(k, false)),
        );

        let main_accounts = dlmm::client::accounts::AddLiquidityByStrategy2 {
            lb_pair,
            position: *position, // pubkey for position
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

        // Optimize memory usage: avoid large vector concatenation
        let mut accounts = main_accounts.to_vec();
        accounts.extend_from_slice(&remaining_accounts_vec);

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };
        msg!("    Adding liquidity instruction created: x {} y {}", amount_x, amount_y );

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts, vec![instruction])?;
        // msg!("deposit result: {:?}", result);

        state.position_pks.push(*position);
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
        let lb_pair_state = get_bytemuck_account::<LbPair>(context.remaining_accounts, &position.lb_pair)
            .ok_or(error!(CustomError::MissingLbPairState))?;

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


    /// Shift mint position
    /// For IRMA, we should deposit first, then withdraw from the old, single bin position (NO).
    /// Note: this can involve shifting to the right or left, depending on the new_price_bin_id.
    /// Note: the "state" (SinglePosition) stays the same, but state.position_pks can change
    /// and must be updated accordingly.
    pub fn shift_mint_position<'a>(
        &self, // must be immutable
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &mut SinglePosition, // modify but not replace (tied to lb_pair)
        new_price_bin_id: i32, // new mint price bin id
        position: &'a Pubkey,
    ) -> Result<()> {

        let positions = &state.position_pks;
        msg!("    shift mint position_pks len = {}", positions.len());
        // determine whether this position is for minting or redeeming
        if positions.len() == 1 {
            // if there's only one position, assume that it's the minting position, withdraw
            let poskey = state.position_pks[0];
            // msg!("mint position {} withdraw", poskey.to_string());
            self.withdraw(payer, remaining_accounts, state, poskey)?;
        }
        else if positions.len() == 2 {
            // if there are two positions, find the one with max_bin_id
            let two_positions = fetch_positions(remaining_accounts, positions)?;
            // the minting position is the one with higher bin id
            if two_positions[0].lower_bin_id > two_positions[1].lower_bin_id {
                self.withdraw(payer, remaining_accounts, state, state.position_pks[0])?;
            } else if two_positions[0].lower_bin_id < two_positions[1].lower_bin_id {
                self.withdraw(payer, remaining_accounts, state, state.position_pks[1])?;
            } else {
                // there should alwaays be at least a bin difference between the two positions
                return Err(Error::from(CustomError::DuplicatePositions));
            }
        }
        else {
            return Err(Error::from(CustomError::InvalidNumberOfPositions));
        }


        // retry if error, amount_y should be zero
        // this also creates a new position and returns its key
        // msg!("mint deposit for {}", state.lb_pair);
        let _new_position_key = match self
            .deposit(payer, remaining_accounts, state, MINTING_POSITION_AMOUNT, 0, new_price_bin_id, position)
        {
            Err(_) => {
                self.deposit(payer, remaining_accounts, state, MINTING_POSITION_AMOUNT, 0, new_price_bin_id, position)?
            }
            Ok(pos_key) => pos_key,
        };

        // msg!("mint position created: {}", new_position_key.to_string());
        
        Ok(())
    }


    /// Shift redeem position
    /// For IRMA, we deposit first, then withdraw from the old bin.
    pub fn shift_redeem_position<'a>(
        &self, // must be immutable
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &mut SinglePosition,
        new_price_bin_id: i32, // new redemption price bin id
        position: &'a Pubkey, // new redemption position account
    ) -> Result<()> {
        // let pair_config = get_pair_config(&self.config, state.lb_pair);
        // msg!("shift redeem position {}", state.lb_pair);

        let positions = &state.position_pks;
        msg!("==> shift redeem position_pks len = {}", positions.len());
        // determine which position is for minting or redeeming
        if positions.len() == 1 {
            // if there's only one position, assume that it's the minting position, leave it alone
            let _poskey = positions[0];
            // msg!("mint position {}", poskey.to_string());
        }
        else if positions.len() == 2 {
            // if there are two positions, find the one with greater bin id
            let two_positions = fetch_positions(remaining_accounts, positions)?;
            msg!("    withdraw from redeem position: ");
            // the minting position is the one with higher bin id
            if two_positions[0].lower_bin_id > two_positions[1].lower_bin_id {
                self.withdraw(payer, remaining_accounts, state, state.position_pks[1])?;
                msg!("    {}", state.position_pks[1]);
            } else if two_positions[0].lower_bin_id < two_positions[1].lower_bin_id {
                self.withdraw(payer, remaining_accounts, state, state.position_pks[0])?;
                msg!("    {}", state.position_pks[0]);
            } else {
                // there should always be at least a bin difference between the two positions
                msg!("   lower bin ids are equal! {}", two_positions[0].lower_bin_id);
                return Err(Error::from(CustomError::DuplicatePositions));
            }
        }
        else {
            return Err(Error::from(CustomError::InvalidNumberOfPositions));
        }

        // sanity check with real balances
        // let (amount_x, amount_y) = self.get_deposit_amount(context, state, amount_x, amount_y)?;
        // msg!("redemption deposit for {}", state.lb_pair);
        let new_position_key = match self
            .deposit(payer, remaining_accounts, state, 0, REDEMPTION_POSITION_AMOUNT, new_price_bin_id, position)
        {
            Err(_) => {
                self.deposit(payer, remaining_accounts, state, 0, REDEMPTION_POSITION_AMOUNT, new_price_bin_id, position)?
            }
            Ok(pos_key) => pos_key,
        };

        // msg!("redemption position created: {}", new_position_key.to_string());

        Ok(())
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

    pub fn clean_up_config_and_positions(&mut self) -> Result<()> {
        let bad_pair = "11111111111111111111111111111111";
        if self.config.iter().any(|pair| bad_pair == pair.pair_address) {
            // remove extraneous dummy entry
            for i in (0..self.config.len()).rev() {
                let pair_config = &self.config[i];
                if pair_config.pair_address == bad_pair {
                    self.config.remove(i);
                }
            }
            for i in (0..self.position_data.all_positions.len()).rev() {
                let position_entry = &self.position_data.all_positions[i];
                if position_entry.lb_pair.to_string() == bad_pair {
                    self.position_data.all_positions.remove(i);
                }
            }
        }
        Ok(())
    }
}
