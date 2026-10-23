use commons::dlmm::accounts::*;
use commons::dlmm::types::*;
use commons::{
    // BASIS_POINT_MAX,
    *,
};
use commons::{
    fetch_lb_pair_state, get_bytemuck_account,
    conversions::fetch_positions,
    get_matching_positions,
    derive_event_authority_pda,
    derive_position_pda,
    get_multiple_bytemuck_account_refs,
    calculate_transfer_fee_included_amount,
};

use crate::position_manager::*;
use crate::pair_config::*;
use crate::pricing;
use crate::errors::CustomError;
use crate::{Maint, StateMap, IRMA_ID};
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
// use anchor_spl::token_2022_extensions::spl_pod::option::Nullable;
// use anchor_spl::token_2022::spl_token_2022;
// use anchor_spl::token::spl_token;
const DLMM_ID: Pubkey = commons::dlmm::ID;

pub use crate::position_manager::MAX_POSITIONS;

// Enum to represent either type of deserializable account
#[derive(Debug, Clone)]
pub enum AccountData<T> {
    Bytemuck(T),
    Anchor(T),
}

const MINTING_THRESHOLD: u64 = 1_000_000; // 1 million units for minting positions
const MINTING_TARGET: u64 = MINTING_THRESHOLD + 100_000;
const REDEMPTION_THRESHOLD: u64 = 100_000;
const REDEMPTION_TARGET: u64 = REDEMPTION_THRESHOLD + 10_000;

const WORKER_MEMO_STRING: &str = "IRMA_WORKER_SWAP";

impl<T> AccountData<T> {
    pub fn into_inner(self) -> T {
        match self {
            AccountData::Bytemuck(data) => data,
            AccountData::Anchor(data) => data,
        }
    }
}

/// What a change in Core's position totals (x = IRMA, y = stablecoin) means.
#[derive(Debug, PartialEq)]
pub enum Fill {
    None,
    Sale(u64),  // protocol sold IRMA; stablecoin received, in stablecoin atomic units
    Buy(u64),   // protocol bought IRMA back; IRMA received, in IRMA atomic units
    Other,      // not a trade: liquidity added or removed outside this program, or dust
}

/// A trade against Core's liquidity always moves the two totals in opposite directions.
pub fn classify_fill(prev: (u64, u64), now: (u64, u64)) -> Fill {
    let ((px, py), (nx, ny)) = (prev, now);
    if now == prev {
        Fill::None
    } else if nx < px && ny > py {
        Fill::Sale(ny - py)
    } else if nx > px && ny < py {
        Fill::Buy(nx - px)
    } else {
        Fill::Other
    }
}

/// Record a fill in pricing.rs. On a pricing error the reserves are restored and the error
/// is logged, so bookkeeping can never block a rebalance (before, the same amount sent by the
/// worker failed in its own trade-event transaction and was lost the same way).
fn book_fill(state: &mut Account<StateMap>, symbol: &str, amount: u64, is_sale: bool) {
    let backup = state.reserves.clone();
    let result = if is_sale {
        pricing::mint_irma(state, symbol, amount)
    } else {
        // redeem_irma takes at most MAX_REDEEM_AMOUNT whole IRMA per call
        let chunk = pricing::MAX_REDEEM_AMOUNT as u64 * 10u64.pow(pricing::IRMA.backing_decimals as u32);
        let mut left = amount;
        let mut result = Ok(());
        while left > 0 && result.is_ok() {
            let part = left.min(chunk);
            result = pricing::redeem_irma(state, symbol, part);
            left -= part;
        }
        result
    };
    let kind = if is_sale { "sale" } else { "buy-back" };
    match result {
        Ok(()) => msg!("Booked {} for {}: {}", kind, symbol, amount),
        Err(e) => {
            state.reserves = backup;
            msg!("Could not book {} for {} ({}): {}", kind, symbol, amount, e);
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


    /// Execute a series of Meteora (DLMM) instructions using provided accounts
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


    /// Book the protocol's own fills on one pair into pricing.rs.
    /// Core's position totals only change when a trade fills Core's own liquidity (other LPs
    /// in the pool hold their own shares), so the change since the snapshot kept in the
    /// pair's config entry is exactly what the protocol sold or bought back. Moves the snapshot.
    /// Returns false when the totals could not be read completely (positions or bin arrays
    /// missing from the accounts) and the snapshot is live; nothing is booked in that case.
    pub fn sync_protocol_fills<'a>(
        &mut self,
        state: &mut Account<StateMap>,
        acct_infos: &'a [AccountInfo<'a>],
        lb_pair: Pubkey,
        symbol: &str,
    ) -> Result<bool> {
        let amounts = self.get_position_state(lb_pair).get_position_amounts(acct_infos)?;
        let pair_address = lb_pair.to_string();
        let config = self.config.iter_mut()
            .find(|c| c.pair_address == pair_address)
            .ok_or(error!(CustomError::InvalidLbPairState))?;
        let prev = (config.x_amount, config.y_amount);
        let now = match amounts {
            Some(now) => now,
            None => {
                msg!("Fill sync skipped for {}: Core's positions or bin arrays not passed", symbol);
                return Ok(prev == (0, 0));
            }
        };
        if prev == (0, 0) {
            // first sync for this pair: take the current totals as the baseline
            msg!("Fill snapshot initialized for {}: x {} y {}", symbol, now.0, now.1);
        } else {
            match classify_fill(prev, now) {
                Fill::None => {}
                Fill::Sale(amount) => book_fill(state, symbol, amount, true),
                Fill::Buy(amount) => book_fill(state, symbol, amount, false),
                Fill::Other => msg!(
                    "Non-trade change in {} position: x {} -> {}, y {} -> {}; snapshot reset",
                    symbol, prev.0, now.0, prev.1, now.1),
            }
        }
        config.x_amount = now.0;
        config.y_amount = now.1;
        Ok(true)
    }

    /// Re-read Core's position totals for a pair and store them as the fill snapshot,
    /// after this program itself added or removed liquidity (shift, replenish, counter-swap).
    pub fn store_fill_snapshot<'a>(
        &mut self,
        acct_infos: &'a [AccountInfo<'a>],
        lb_pair: Pubkey,
    ) -> Result<()> {
        let (amount_x, amount_y) = match self.get_position_state(lb_pair).get_position_amounts(acct_infos)? {
            Some(amounts) => amounts,
            None => {
                msg!("Fill snapshot not stored for {}: Core's positions or bin arrays not passed", lb_pair);
                return Ok(());
            }
        };
        let pair_address = lb_pair.to_string();
        let config = self.config.iter_mut()
            .find(|c| c.pair_address == pair_address)
            .ok_or(error!(CustomError::InvalidLbPairState))?;
        config.x_amount = amount_x;
        config.y_amount = amount_y;
        Ok(())
    }
    
    /// Refresh internal state by fetching positions and bin arrays.
    /// This function is called from sale_trade_event and buy_trade_event.
    /// Instead of parameters, it uses context to fetch necessary accounts
    /// and the config Vec in Core to go through the pairs.
    /// NOTE: This is not being called from anywhere as of now
    pub fn refresh_position_data<'a>(
        &self, // must be mutable
        owner: &Pubkey, // owner of the positions is also the creator of the pool
        remaining_accounts: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition, // for particular lb_pair with this token
        is_sale: bool,
    ) -> Result<()> {
        msg!("==> Refreshing, is_sale: {}", is_sale);
        // all_positions contains SinglePosition entries, one for each reserve stablecoin.
        // One of these therefore represents the current mint or redemption SinglePosition.
        // The sequence number of the position in all_positions should be the same

        // bin_array_pks and position_pks in state should be set according to what's in DLMM
        // and never the other way around.

        let pair_address = mut_state.lb_pair; // DLMM LbPair address

        // msg!("==> Refreshing state for owner: {}", owner.to_string());

        // get all DLMM PositionV2's by the same user, for this trade pair.
        // there should only be two positions, at most - one for minting, one for redemption
        let position_keys_with_states = get_matching_positions(
            remaining_accounts,
            owner, // must be owner of SinglePosition position_pks
            &pair_address
        ).or_else(|error| Err(error)).unwrap();

        if position_keys_with_states.len() == 0 {
            return Ok(());
        }

        if position_keys_with_states.len() > MAX_POSITIONS {
            msg!("    Found too many PositionV2 positions: {}", position_keys_with_states.len());
        }

        let pos_index = if is_sale { 0 } else { 1 };

        // let mut_state = self.get_mut_position_state(pair_address);

        let poskey =
        if is_sale && mut_state.position_pks[pos_index] == Pubkey::default() {
            msg!("    Updating {} entry in position_pks", pos_index);
            mut_state.position_pks[pos_index] = *position_keys_with_states[pos_index].0;
            mut_state.position_pks[pos_index]
        } else if mut_state.position_pks[pos_index] == *position_keys_with_states[pos_index].0 {
            msg!("    Current position matches position fetched from DLMM");
            mut_state.position_pks[pos_index]
        } else {
            // must replace old position
            mut_state.position_pks[pos_index] = *position_keys_with_states[pos_index].0;
            mut_state.position_pks[pos_index]
        };

        // let mut position_pks: Vec<&Pubkey> = vec![];
        // Note: We'll fetch PositionV2 positions and bin_arrays dynamically when needed
        // let mut positions = vec![];
        // let mut min_bin_id = 0;
        // let mut max_bin_id = 0;
        // let mut bin_arrays_vec = Vec::<(Pubkey, BinArray)>::new();

        let (poskey, _position_state) =
            position_keys_with_states.iter()
                .find(|(key, _)| *key == &poskey)
                .unwrap();

        msg!("    Found position: {}", poskey);

        // let pos_v2 = if position_keys_with_states.len() == 1 {
        //     match is_sale {
        //         true => &mut position_keys_with_states[0].1, // mint position comes first
        //         false => return Err(error!(CustomError::AdditionalPositionRequired)), // only one position, and it's not the one we want
        //     }
        // } else if position_keys_with_states.len() > 1 {
        //     match is_sale {
        //         true => &mut position_keys_with_states.last().unwrap().1,
        //         false => &mut position_keys_with_states.first().unwrap().1, // second one is redemption
        //     }
        // }

        // msg!("    Selected PositionV2 with bin range: {} - {}", 
        //     pos_v2.lower_bin_id, pos_v2.upper_bin_id);

        // // from here on we should have to deal only with a single PositionV2
        // let _ = pos_v2.get_bin_array_keys_coverage(&mut state.bin_array_pks)?;

        // msg!("   bin array keys count: {}", state.bin_array_pks.len());

        mut_state.lb_pair = pair_address;
        // Don't store non-serializable types - they will be fetched dynamically
        // state.bin_arrays = bin_arrays_vec; // fetch dynamically
        // state.positions = positions; // fetch dynamically
        // state.bin_array_pks = bin_array_keys; // already done, see above
        // state.position_pks = position_pks.iter().map(|k| **k).collect(); // already done above
        // don't set min_bin_id and max_bin_id here because these now correspond to 
        // redemption price and mint price, respectively
        // state.min_bin_id = min_bin_id;
        // state.max_bin_id = max_bin_id;
        mut_state.last_update_timestamp = Self::get_epoch_sec()?.max(0) as u64;
        mut_state.inc_rebalance_time();

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
            )?;
            let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
            token_mints_with_program.push((lb_pair_state.token_x_mint, token_x_program));
            token_mints_with_program.push((lb_pair_state.token_y_mint, token_y_program));
        }

        token_mints_with_program.sort_unstable();
        token_mints_with_program.dedup();
        Ok(token_mints_with_program)
    }

    /// Get immutable position state for a given liquidity pair
    /// For testability
    pub fn get_position_state(&self, lp_pair: Pubkey) -> SinglePosition {
        let state = &self.position_data;
        let position = state.get_position(&lp_pair).unwrap();
        position.clone()
    }

    /// Get mutable position state for a given liquidity pair
    /// For testability
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

    /// Initialize a new position for the user.
    fn initialize_position<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition,
        new_price_bin_id: i32,
    ) -> Result<Pubkey> {
        let lb_pair: &Pubkey = &mut_state.lb_pair;
        msg!("==> Initializing new position for lb_pair: {}", lb_pair);
        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, lb_pair)?;

        let mut remaining_account_info = RemainingAccountsInfo { slices: vec![] };
        let mut transfer_hook_remaining_accounts = vec![];

        if let Some((slices, remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                remaining_accounts,
                ActionType::Liquidity,
            )?
        {
            remaining_account_info.slices = slices;
            transfer_hook_remaining_accounts = remaining_accounts;
        }

        // Create unique base for this bin position
        let bin_array_index = BinArray::bin_id_to_bin_array_index(new_price_bin_id)?;
        // let pos_seed = format!("irma_bin_{}", bin_array_index);
        // msg!("    pos_seed: {}", pos_seed);
        // let base = Pubkey::create_with_seed(&payer.key(), &pos_seed, &system_program::ID).unwrap();
        let base = payer.key();

        let lower_bin_id = BinArray::get_bin_array_lower_upper_bin_id(bin_array_index)?.0;
        msg!("    lower_bin_id: {}", lower_bin_id);

        // this should generate different keys for mint and redemption positions
        let new_position_key = derive_position_pda(
            *lb_pair,
            base, // unique base for this bin
            lower_bin_id,
            70i32, // max width
        ).0;
        msg!("    new_position_key: {}", new_position_key);

        if mut_state.position_pks[0] == new_position_key || mut_state.position_pks[1] == new_position_key {
            return Ok(new_position_key); // do not re-initialize
        }

        let (event_authority, _bump) = derive_event_authority_pda();

        // The bin array has changed, so we need to re-initialize the position.

        let main_accounts = dlmm::client::accounts::InitializePositionPda {
            payer: payer.key(), // Base for the PDA derivation
            base, // unique base for this bin
            position: new_position_key, // pda
            lb_pair: *lb_pair,
            owner: payer.key(), // User owns the position (for compatibility)
            system_program: system_program::ID,
            rent: rent::ID,
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        let accounts = [
            transfer_hook_remaining_accounts.clone(),
            main_accounts.to_vec(),
        ]
        .concat();

        let data = dlmm::client::args::InitializePositionPda {
            lower_bin_id: lower_bin_id,
            width: 70i32, // widest possible, but we will add liquidity only to the current bin
        }
        .data();

        let instruction = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        Core::execute_meteora_instruction(payer, remaining_accounts, vec![instruction])?;

        Ok(new_position_key)
    }

    /// Close a position after withdrawing all liquidity and claiming fees.
    /// Liquidity must have been withdrawn and fees claimed from the position before closing.
    fn close_position<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        state: &SinglePosition,
        position_key: &Pubkey,
    ) -> Result<()> {
        if state.position_pks.len() == 0 {
            return Ok(());
        }
        msg!("==> Closing old position: {}", position_key);

        let (event_authority, _bump) = derive_event_authority_pda();

        let main_accounts = dlmm::client::accounts::ClosePosition2 {
            position: *position_key,
            sender: payer.key(),
            rent_receiver: payer.key(), // receive rent lamports
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        let data = dlmm::client::args::ClosePosition2 {}.data();

        let accounts = main_accounts.to_vec();

        let close_position_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        Core::execute_meteora_instruction(payer, remaining_accounts_in, vec![close_position_ix])?;

        Ok(())
    }

    /// Withdraw a position and claim fees in a single transaction.
    /// Liquidity must have been deposited to the new price single-bin position, so the old
    /// single-bin position can be safely withdrawn and claimed.
    pub fn withdraw<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        lb_pair: &Pubkey,
        old_position_key: &Pubkey,
    ) -> Result<bool> {
        msg!("==> Withdrawing old position: {}", old_position_key);

        let (event_authority, _bump) = derive_event_authority_pda();

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts_in, lb_pair)?;

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

        let position_state: &PositionV2;
        let vec_positions = fetch_positions(remaining_accounts_in, &[*old_position_key])?;
        if vec_positions.len() == 0 {
            // nothing to do
            return Ok(false);
        }
        else {
            position_state = vec_positions
            .get(0)
            .ok_or(error!(CustomError::PositionNotFound))?;
        }

        let bin_arrays_account_meta = position_state.get_bin_array_accounts_meta_coverage()?;

        msg!("    bin_arrays_account_meta: {:?}", bin_arrays_account_meta);

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

        // // Check who owns this position to determine the correct authority
        // let position_state = vec_positions
        //     .get(0)
        //     .ok_or(error!(CustomError::PositionNotFound))?;
            
        // Determine the correct sender/authority based on position ownership
        let position_owner = position_state.owner;
        let is_pda_owned = {
            let (irma_authority, _) = Pubkey::find_program_address(
                &[b"irma_authority"],
                &IRMA_ID,
            );
            position_owner == irma_authority
        };

        msg!("    Position owner: {}, is PDA owned: {}", position_owner, is_pda_owned);

        let mut bin_array_keys: Vec<Pubkey> = Vec::new();
        let _ = position_state.get_bin_array_keys_coverage(&mut bin_array_keys);

        msg!("    bin_array_keys: {:?}", bin_array_keys);

        let bin_array_states = get_multiple_bytemuck_account_refs::<BinArray>(
            remaining_accounts_in,
            &bin_array_keys,
        )?;

        if bin_array_states.len() != bin_array_keys.len() {
            msg!("    Not all bin arrays found, cannot proceed with withdraw.");
            return Ok(false);
        }
        msg!("    bin_array_keys len: {}", bin_array_keys.len());

        // find out first whether there is anything to withdraw at all?
        // during production, there should always be liquidity to withdraw.

        let main_accounts = dlmm::client::accounts::RemoveLiquidity2 {
            position: *old_position_key,
            lb_pair: *lb_pair,
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

        let mut allbins = vec![];
        for bin_id in position_state.lower_bin_id..position_state.upper_bin_id + 1 {
            allbins.push(BinLiquidityReduction {
                bin_id,
                bps_to_remove: 10_000, // remove all liquidity from each bin
            });
        }

        msg!("   Remove all instruction data len: {}", allbins.len());

        let data = dlmm::client::args::RemoveLiquidity2 {
            bin_liquidity_removal: allbins,
            remaining_accounts_info: remaining_account_info.clone(),
        }.data();

        let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

        let remove_all_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        let mut instructions = vec![];

        instructions.push(remove_all_ix);

        let main_accounts = dlmm::client::accounts::ClaimFee2 {
            lb_pair: *lb_pair,
            position: *old_position_key,
            sender: payer.key(),
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            user_token_x,
            user_token_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            token_program_x: token_x_program,
            token_program_y: token_y_program,
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

        // we don't really need to close the position at all
        // just deposit to a different bin next

        msg!("==> Executing withdraw and ClaimFee instructions...");

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, instructions)?;
        // msg!("Close old_position_key: {}, result: {:?}", old_position_key, result);

        Ok(true)
    }



    /// Swap tokens on the DLMM.
    /// We may need this to overcome AMM behavior, in case off-chain swap is too slow.
    /// If not, according to Taha, we can use withdraw() above instead.
    /// Using this for counter-swaps.
    pub fn counter_swap<'a>(
        &self,
        payer: &mut Signer<'a>,
        remaining_accounts: &'a [AccountInfo<'a>],
        state: &SinglePosition,
        amount_in: u64,
        exact_out: u64,
        swap_for_y: bool
    ) -> Result<()> {

        msg!("==> Swapping on pair: {}", state.lb_pair);

        let lb_pair_state = fetch_lb_pair_state(remaining_accounts, &state.lb_pair)?;

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
        let lb_pair = state.lb_pair;

        let (event_authority, _bump) = derive_event_authority_pda();

        msg!("    event authority: {}", event_authority);

        let bin_arrays_account_meta = get_bin_array_pubkeys_for_swap(
            lb_pair,
            &lb_pair_state,
            None, // Some(bitmap_extension),
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

        let main_accounts = dlmm::client::accounts::SwapExactOut2 {
            lb_pair,
            bin_array_bitmap_extension: None, // bitmap_extension.lb_pair),
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            user_token_in,
            user_token_out,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            oracle: lb_pair_state.oracle,
            host_fee_in: None, // Some(host_fee_in), // for swap, host fee accounts are optional
            user: payer.key(),
            token_x_program,
            token_y_program,
            memo_program: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        // msg!("    main accounts.token_y_mint: {:?}", main_accounts.token_y_mint);

        let data = dlmm::client::args::SwapExactOut2 {
            max_in_amount: amount_in, // we are willing to pay up to this amount
            out_amount: exact_out, // min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y, &lb_pair_state)?,
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

        let memo_ix = Instruction {
            program_id: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
            accounts: vec![
                AccountMeta::new(payer.key(), true),
            ],
            data: WORKER_MEMO_STRING.try_to_vec()?,
        };

        // let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        let instructions = [swap_ix, memo_ix];

        msg!("    Executing exact out swap instruction...");

        let result = Core::execute_meteora_instruction(payer, remaining_accounts, instructions.to_vec())?;
        msg!("Swap amount_in: {}, swap_for_y: {}, result: {:?}", amount_in, swap_for_y, result);

        Ok(())
    }


    /// Check the health of a position, returning the amounts needed to replenish it.
    /// Takes the position directly (rather than an index into `self.position_data`)
    /// so callers can pass the live, just-mutated `SinglePosition` from the current
    /// instruction instead of a pre-shift snapshot of `self`.
    pub fn check_position_health<'a>(
        &self,
        acct_infos: &'a [AccountInfo<'a>],
        state: &SinglePosition,
    ) -> Result<(u64, u64)> {
        let (mut replenish_x_amount, mut replenish_y_amount) =
                    state.get_liquidity_in_position(acct_infos, &self.position_data)?;
        msg!("    --> amount_x remaining: {}, amount_y remaining: {}", 
                    replenish_x_amount, replenish_y_amount);
        // use data in self to calculate actual amounts

        if replenish_x_amount < MINTING_THRESHOLD {
            replenish_x_amount = MINTING_TARGET - replenish_x_amount;
        }
        else {
            replenish_x_amount = 0;
        }
        if replenish_y_amount < REDEMPTION_THRESHOLD {
            replenish_y_amount = REDEMPTION_TARGET - replenish_y_amount;
        }
        else {
            replenish_y_amount = 0;
        }
        Ok((replenish_x_amount, replenish_y_amount))
    }


    /// Replenish the minting position by depositing amount_x into it.
    pub fn replenish_minting_position<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition, // modify only position_pks, bin_array_pks
        amount_x: u64,
    ) -> Result<()> {
        // Ensure position_pks always has 2 initialized slots (legacy on-chain state may be empty).
        while mut_state.position_pks.len() < 2 {
            mut_state.position_pks.push(Pubkey::default());
        }
        // Treat a missing or unfindable position (e.g. a stale V1 keypair position
        // that predates V2 PDAs) the same as an uninitialized key: create a fresh V2 PDA.
        let needs_init = mut_state.position_pks[0] == Pubkey::default()
            || fetch_positions(remaining_accounts_in, &[mut_state.position_pks[0]])?.is_empty();
        if needs_init {
            let new_position_key = self.initialize_position(
                payer,
                remaining_accounts_in,
                mut_state,
                mut_state.max_bin_id,
            )?;
            mut_state.position_pks[0] = new_position_key;
        }
        // call deposit() from here
        let max_bin_id = mut_state.max_bin_id;
        let position_key = mut_state.position_pks[0];
        self.deposit(payer, remaining_accounts_in, mut_state, amount_x, 0, max_bin_id, &position_key)?;
        Ok(())
    }


    /// Replenish the redeeming position by depositing amount_y into it.
    pub fn replenish_redeeming_position<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition, // modify only position_pks, bin_array_pks
        amount_y: u64,
    ) -> Result<()> {
        // Ensure position_pks always has 2 initialized slots (legacy on-chain state may be empty).
        while mut_state.position_pks.len() < 2 {
            mut_state.position_pks.push(Pubkey::default());
        }
        let needs_init = mut_state.position_pks[1] == Pubkey::default()
            || fetch_positions(remaining_accounts_in, &[mut_state.position_pks[1]])?.is_empty();
        if needs_init {
            let new_position_key = self.initialize_position(
                payer,
                remaining_accounts_in,
                mut_state,
                mut_state.min_bin_id,
            )?;
            mut_state.position_pks[1] = new_position_key;
        }
        // call deposit() from here
        let min_bin_id = mut_state.min_bin_id;
        let position_key = mut_state.position_pks[1];
        self.deposit(payer, remaining_accounts_in, mut_state, 0, amount_y, min_bin_id, &position_key)?;
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
    /// 2. Initialize position if it does not exist. (For IRMA, skip this because we use the existing position.)
    /// 3. Add liquidity to the position.
    /// 4. Output the position pubkey for reference.
    pub fn deposit<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition, // mutable position_pks, bin_array_pks
        amount_x: u64, // must be zero if amount_y > 0 and vice versa
        amount_y: u64, // must be zero if amount_x > 0 and vice versa
        new_price_bin_id: i32, // this is not the lb_pair active bin id; this is the bin we want to deposit to
        poskey: &Pubkey,
    ) -> Result<()> {
        msg!("==> Depositing liquidity, new bin: {}", new_price_bin_id);
        // enforce exclusive OR condition
        require!(
            (amount_x == 0) != (amount_y == 0),
            CustomError::InvalidDepositAmounts
        );

        if mut_state.position_pks.len() > MAX_POSITIONS {
            return Err(error!(CustomError::TooManyPositionsForPair));
        }
        
        // for IRMA, the lower bin id is always equal to the upper bin id
        // since we only provide liquidity in one bin at any time;
        // we don't really care where the market is, so we don't use active_id.
        let bin_array_idx = BinArray::bin_id_to_bin_array_index(new_price_bin_id)?;

        let lb_pair = mut_state.lb_pair;
        let lb_pair_state = fetch_lb_pair_state(remaining_accounts_in, &lb_pair)?;

        let (event_authority, _bump) = derive_event_authority_pda();

        // Initialize bin array where the bin belongs, if not exists
        let (bin_array, _bump) = derive_bin_array_pda(lb_pair, bin_array_idx.into());

        // msg!("    Checking bin array at index: {}", bin_array_idx);
        let pos_index = if amount_x > 0 { 0 } else { 1 };

        if mut_state.bin_array_pks[pos_index] != bin_array {
            msg!("    Bin array account is new, initializing...");
            let accounts = dlmm::client::accounts::InitializeBinArray {
                lb_pair,
                bin_array, // derived
                funder: payer.key(),
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

            let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, vec![instruction])?;
            // msg!("    Bin array initialized");
            mut_state.bin_array_pks[pos_index] = bin_array;
        }
        msg!("    Bin array: {}", bin_array);

        // TODO implement bitmap extension fetching
        let bin_array_bitmap_extension = None;

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

        let tokens = self.get_all_tokens();
        let decimals_x = get_decimals(lb_pair_state.token_x_mint, &tokens);
        let decimals_y = get_decimals(lb_pair_state.token_y_mint, &tokens);

        // Now convert UI amount to internal amount
        let internal_amount = if amount_x > 0 {
            10_u64.checked_pow(decimals_x as u32)
                .ok_or(error!(CustomError::MathOverflow))
                .unwrap()
                .checked_mul(amount_x.into())
                .ok_or(error!(CustomError::MathOverflow))?
        } else {
            10_u64.checked_pow(decimals_y as u32)
                .ok_or(error!(CustomError::MathOverflow))
                .unwrap()
                .checked_mul(amount_y.into())
                .ok_or(error!(CustomError::MathOverflow))?
        };

        let token_mint_base_account = get_account_info(
            remaining_accounts_in,
            &lb_pair_state.token_x_mint,
        )?;
        let clock_account = get_account_info(
            remaining_accounts_in,
            &sysvar::clock::ID,
        )?;
        let clock = Clock::from_account_info(&clock_account)?;

        let internal_amount = calculate_transfer_fee_included_amount(
            token_mint_base_account,
            internal_amount,
            clock.epoch,
        )?
        .amount;

        let mut remaining_accounts_info = RemainingAccountsInfo { slices: vec![] };
        let mut remaining_accounts_vec = vec![];

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                remaining_accounts_in,
                ActionType::Liquidity,
            )?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts_vec.extend(transfer_hook_remaining_accounts);
        }

        if Some(bin_array) != None {
            msg!("    Adding bin array account to remaining accounts.");
            remaining_accounts_vec.extend(
                [bin_array]
                    .into_iter()
                    .map(|k| AccountMeta::new(k, false)),
            );
        }

        // Add liquidity one-sided
        
        // Debug: Validate position before adding liquidity
        msg!("==> Validating position: {}", poskey);
        let vec_positions = fetch_positions(remaining_accounts_in, &[*poskey])?;
        if vec_positions.is_empty() {
            msg!("ERROR: Position not found: {}", poskey);
            return Err(error!(CustomError::PositionNotFound));
        }
        
        let position_state = &vec_positions[0];
        // msg!("Position lb_pair: {}, expected: {}", position_state.lb_pair, lb_pair);
        // msg!("Position owner: {}, sender: {}", position_state.owner, payer.key());
        // msg!("Position bins: {} to {}", position_state.lower_bin_id, position_state.upper_bin_id);
        // msg!("Trying to add liquidity to bin: {}", new_price_bin_id);
        
        // Check if new bin is within position's range
        if new_price_bin_id < position_state.lower_bin_id || new_price_bin_id > position_state.upper_bin_id {
            msg!("WARN: New bin {} is outside position range {} to {}, will expand position", 
                new_price_bin_id, position_state.lower_bin_id, position_state.upper_bin_id);
        }
        
        if position_state.lb_pair != lb_pair {
            msg!("ERROR: Position belongs to different pair");
            return Err(error!(CustomError::PositionPairMismatch));
        }
        
        if position_state.owner != payer.key() {
            msg!("ERROR: Position not owned by payer");
            return Err(error!(CustomError::UnauthorizedPositionAccess));
        }
        
        // Use AddLiquidityByStrategy2 which can expand position range
        let main_accounts = dlmm::client::accounts::AddLiquidityByStrategy2 {
            position: *poskey,
            lb_pair,
            bin_array_bitmap_extension,
            user_token_x,
            user_token_y,
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            token_x_program,
            token_y_program,
            sender: payer.key(),
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        let data = dlmm::client::args::AddLiquidityByStrategy2 {
            liquidity_parameter: {
                LiquidityParameterByStrategy {
                    amount_x: if amount_x > 0 { internal_amount } else { 0u64 },
                    amount_y: if amount_y > 0 { internal_amount } else { 0u64 },
                    active_id: lb_pair_state.active_id, // new_price_bin_id, // <-- did not help
                    max_active_bin_slippage: 30_000, // large slippage to allow single bin deposit far from active
                    strategy_parameters: StrategyParameters {
                        min_bin_id: new_price_bin_id,
                        max_bin_id: new_price_bin_id,
                        strategy_type: StrategyType::SpotImBalanced,
                        parameteres: [0u8; 64],
                    },
                }
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

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, vec![instruction])?;
        // msg!("deposit result: {:?}", result);

        // note that we are using state.min_bin_id as the redemption bin id and
        // the state.max_bin_id as the mint bin id
        // if max_bin_id == min_bin_id, then the new_price_bin_id should be the active bin
        // however, we should ensure that max_bin_id is never less than min_bin_id
        if amount_x > 0 {
            mut_state.max_bin_id = new_price_bin_id;
        }
        else if amount_y > 0 {
            mut_state.min_bin_id = new_price_bin_id;
        }

        Ok(())
    }

    /// Rebalance position by depositing to a new bin.
    /// NOTE: This is not used in IRMA currently, as we use withdraw() + deposit() instead.
    pub fn rebalance<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts_in: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition,
        target_bin_id: i32,
        amount_x: u64,
        amount_y: u64,
        old_position_key: &Pubkey,
    ) -> Result<()> {
        msg!("==> Rebalancing position to bin id: {}", target_bin_id);
        msg!("==> Rebalance position: {}", old_position_key);

        let (event_authority, _bump) = derive_event_authority_pda();

        let lb_pair = mut_state.lb_pair;
        let lb_pair_state = fetch_lb_pair_state(remaining_accounts_in, &lb_pair)?;

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

        let vec_positions = fetch_positions(remaining_accounts_in, &[*old_position_key])?;
        let position_state = vec_positions
            .get(0)
            .ok_or(error!(CustomError::PositionNotFound))?;

        let bin_arrays_account_meta = position_state.get_bin_array_accounts_meta_coverage()?;

        msg!("   User token programs: x: {}, y: {}", token_x_program, token_y_program);

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
        let is_pda_owned = {
            let (irma_authority, _) = Pubkey::find_program_address(
                &[b"irma_authority"],
                &IRMA_ID,
            );
            position_owner == irma_authority
        };

        msg!("Position owner: {}, is PDA owned: {}", position_owner, is_pda_owned);

        let mut bin_array_keys: Vec<Pubkey> = Vec::new();
        let _ = position_state.get_bin_array_keys_coverage(&mut bin_array_keys);

        if bin_array_keys.len() == 0 {
            return Err(error!(CustomError::BinArrayNotFound)); // position found must come with bin_array
        }
        let pos_index = if amount_x > 0 { 0 } else { 1 };
        mut_state.bin_array_pks[pos_index] = bin_array_keys[0]; // at least one bin array must exist

        // let bin_array_states = get_multiple_bytemuck_account_refs::<BinArray>(
        //     remaining_accounts_in,
        //     &bin_array_keys,
        // )?;

        let main_accounts = dlmm::client::accounts::RebalanceLiquidity {
            position: *old_position_key,
            lb_pair: mut_state.lb_pair,
            bin_array_bitmap_extension: None,
            user_token_x,
            user_token_y,
            reserve_x: lb_pair_state.reserve_x,
            reserve_y: lb_pair_state.reserve_y,
            token_x_mint: lb_pair_state.token_x_mint,
            token_y_mint: lb_pair_state.token_y_mint,
            owner: position_owner,
            rent_payer: payer.key(),
            token_x_program,
            token_y_program,
            memo_program: Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap(),
            system_program: system_program::ID,
            event_authority,
            program: DLMM_ID,
        }
        .to_account_metas(None);

        let remaining_accounts = [
            transfer_hook_remaining_accounts.clone(),
            bin_arrays_account_meta.clone(),
        ]
        .concat();

        let data = dlmm::client::args::RebalanceLiquidity {
            params: {
                RebalanceLiquidityParams {
                    active_id: lb_pair_state.active_id,
                    max_active_bin_slippage: 30_000,
                    should_claim_fee: true,
                    should_claim_reward: true,
                    min_withdraw_x_amount: 1u64,
                    max_deposit_x_amount: u64::MAX,
                    min_withdraw_y_amount: 1u64,
                    max_deposit_y_amount: u64::MAX,
                    padding: [0u8; 31], // used to be 32, now getting compile error
                    shrink_mode: 0u8, // 0 for no shrink, 1 for shrink to new_price_bin_id
                    removes: vec![
                        RemoveLiquidityParams {
                            min_bin_id: Some(position_state.lower_bin_id),
                            max_bin_id: Some(position_state.upper_bin_id),
                            bps: 10_000, // remove all liquidity from each bin
                            padding: [0u8; 16],
                        }
                    ],
                    adds: vec![
                        AddLiquidityParams {
                            min_delta_id: 0, // TODO: understand all these parameters <--
                            max_delta_id: 0,
                            x0: 0u64,
                            y0: 0u64,
                            delta_x: if amount_x > 0 
                                { amount_x.try_into().unwrap() } 
                                else { 0u64 },
                            delta_y: if amount_y > 0 
                                { amount_y.try_into().unwrap() } 
                                else { 0u64 },
                            bit_flag: 0u8,
                            favor_x_in_active_id: false,
                            padding: [0u8; 16],
                        }
                    ],
                }
            },
            remaining_accounts_info: remaining_account_info.clone(),
        }.data();

        let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

        let rebalance_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };
        msg!("    Adding liquidity instruction created: x {} y {}", amount_x, amount_y );

        let _result = Core::execute_meteora_instruction(payer, remaining_accounts_in, vec![rebalance_ix])?;

        Ok(())
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

    /// Replace position if required.
    /// Do not modify position_pks and bin_array_pks
    fn replace_position_if_required<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition,
        new_price_bin_id: i32,
        old_position_key: &'a Pubkey,
        position_state: &PositionV2,
    ) -> Result<Pubkey> {
        // For IRMA, we always have only one bin position at any time.
        // So after withdrawing from the old position, we can close it.
        msg!("==> Closing old position if required: {}", old_position_key);

        if position_state.lower_bin_id <= new_price_bin_id &&
           position_state.upper_bin_id >= new_price_bin_id {
            msg!("    Position bin matches target bin, no need to replace.");
            return Ok(*old_position_key);
        }

        // we cannot close if the two positions are one and the same
        if mut_state.position_pks[0] != mut_state.position_pks[1] {
            msg!("    Two positions exist, need to close one first.");
            self.close_position(
                payer,
                remaining_accounts,
                mut_state,
                old_position_key,
            )?;
        }

        Ok(self.initialize_position(
            payer,
            remaining_accounts,
            mut_state,
            new_price_bin_id,
        )?)
    }

    /// Shift mint position
    /// For IRMA, we should deposit first, then withdraw from the old, single bin position (NO).
    /// Note: this can involve shifting to the right or left, depending on the new_price_bin_id.
    /// Note: the "state" (SinglePosition) stays the same, but state.position_pks can change
    /// and must be updated accordingly.
    pub fn shift_mint_position<'a>(
        &self, // must be mutable
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition, // modify but not replace (tied to lb_pair)
        new_price_bin_id: i32, // new mint price bin id
    ) -> Result<()> {

        let pair_address = mut_state.lb_pair;

        let position_keys_with_states = get_matching_positions(
            remaining_accounts,
            &payer.key(), // must be owner of SinglePosition position_pks
            &pair_address
        ).or_else(|error| Err(error)).unwrap();
        msg!("==> Fetched {} matching positions", position_keys_with_states.len());

        // Ensure position_pks always has 2 initialized slots (legacy on-chain state may be empty).
        while mut_state.position_pks.len() < 2 {
            mut_state.position_pks.push(Pubkey::default());
        }

        let new_position =
        if mut_state.position_pks[0] != Pubkey::default() {
            let poskey = mut_state.position_pks[0];
            msg!("    withdraw from mint position: {}", poskey.to_string());
            if !self.withdraw(payer, remaining_accounts, &pair_address, &poskey)? {
                msg!("    Initializing new minting position, overwriting old.");
                mut_state.position_pks[0] = self.initialize_position(
                    payer,
                    remaining_accounts,
                    mut_state,
                    new_price_bin_id,
                )?;
            }
            else {
                msg!("    Mint position still exists after withdrawal.");
                let (poskey, position_state) =
                    position_keys_with_states.iter()
                        .find(|(key, _)| *key == &mut_state.position_pks[0])
                        .unwrap();
                mut_state.position_pks[0] = self.replace_position_if_required(
                    payer,
                    remaining_accounts,
                    mut_state,
                    new_price_bin_id,
                    &poskey,
                    &position_state,
                )?;
            }
            mut_state.position_pks[0]
        }
        else if position_keys_with_states.len() > 0 {
            msg!("    position_pks[0] not initialized, storing first poskey.");
            // pick position that is for minting
            let (poskey, position_state) =
                position_keys_with_states
                    .first()
                    .unwrap();

            mut_state.position_pks[0] = self.replace_position_if_required(
                payer,
                remaining_accounts,
                mut_state,
                new_price_bin_id,
                &poskey,
                &position_state,
            )?;
            mut_state.position_pks[0]
        }
        else {
            msg!("    No mint position found, initialize new minting position.");
            mut_state.position_pks[0] = self.initialize_position(
                payer,
                remaining_accounts,
                mut_state,
                new_price_bin_id,
            )?;
            mut_state.position_pks[0]
        };
        self.deposit(payer, remaining_accounts, mut_state, MINTING_TARGET, 0, new_price_bin_id, &new_position)?;
        
        Ok(())
    }


    /// Shift redeem position
    /// For IRMA, we withdraw from the old bin, then deposit into the new bin.
    pub fn shift_redeem_position<'a>(
        &self,
        payer: &mut Signer,
        remaining_accounts: &'a [AccountInfo<'a>],
        mut_state: &mut SinglePosition,
        new_price_bin_id: i32, // new redemption price bin id
    ) -> Result<()> {
        let pair_address = mut_state.lb_pair;

        let position_keys_with_states = get_matching_positions(
            remaining_accounts,
            &payer.key(), // must be owner of SinglePosition position_pks
            &pair_address
        ).or_else(|error| Err(error)).unwrap();
        msg!("==> Redeem: Fetched {} matching positions", position_keys_with_states.len());

        // Ensure position_pks always has 2 initialized slots (legacy on-chain state may be empty).
        while mut_state.position_pks.len() < 2 {
            mut_state.position_pks.push(Pubkey::default());
        }

        let new_position =
        if mut_state.position_pks[1] != Pubkey::default() {
            let poskey = mut_state.position_pks[1];
            msg!("    withdraw from mint position: {}", poskey.to_string());
            if !self.withdraw(payer, remaining_accounts, &pair_address, &poskey)? {
                msg!("    Initializing new redemption position, overwriting old.");
                mut_state.position_pks[1] = self.initialize_position(
                    payer,
                    remaining_accounts,
                    mut_state,
                    new_price_bin_id,
                )?;
            }
            else {
                msg!("    Redeem position still exists after withdrawal.");
                let (poskey, position_state) =
                    position_keys_with_states.iter()
                        .find(|(key, _)| *key == &mut_state.position_pks[1]) // position_pks[1] is redeem position
                        .unwrap();
                mut_state.position_pks[1] = self.replace_position_if_required(
                    payer,
                    remaining_accounts,
                    mut_state, // immutable borrow
                    new_price_bin_id,
                    &poskey,
                    &position_state,
                )?;
            }
            mut_state.position_pks[1]
        }
        else if position_keys_with_states.len() > 1 {
            msg!("    position_pks not initialized, storing second poskey.");
            // pick position that is for redeeming
            let (poskey, position_state) =
                position_keys_with_states
                    .last()
                    .unwrap();

            mut_state.position_pks[1] = self.replace_position_if_required(
                payer,
                remaining_accounts,
                mut_state, // immutable borrow
                new_price_bin_id,
                &poskey,
                &position_state,
            )?;
            mut_state.position_pks[1]
        }
        else {
            msg!("    No mint position found, initialize new redeeming position.");
            mut_state.position_pks[1] = self.initialize_position(
                payer,
                remaining_accounts,
                mut_state,
                new_price_bin_id,
            )?;
            mut_state.position_pks[1]
        };
        self.deposit(
            payer, remaining_accounts, mut_state, 0, REDEMPTION_TARGET, new_price_bin_id, &new_position
        )?;

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

#[cfg(test)]
mod fill_tests {
    use super::*;
    use crate::pricing::StableState;
    use crate::MarketMakingMode;
    use commons::derive_bin_array_pda;

    const UNIT: u64 = 1_000_000; // 6 decimals for IRMA and the stablecoin

    // get_bytemuck_account_ref needs the struct after the 8-byte discriminator to be aligned;
    // u128 is 16-byte aligned on the host (8 on-chain), so back the data with u128s.
    fn pod_data<T: bytemuck::Pod + anchor_lang::Discriminator>(value: &T) -> &'static mut [u8] {
        let len = 8 + std::mem::size_of::<T>();
        let buf: &'static mut [u128] = Box::leak(vec![0u128; len / 16 + 2].into_boxed_slice());
        let data = &mut bytemuck::cast_slice_mut::<u128, u8>(buf)[8..8 + len];
        data[..8].copy_from_slice(T::DISCRIMINATOR);
        data[8..].copy_from_slice(bytemuck::bytes_of(value));
        data
    }

    fn account(key: Pubkey, data: &'static mut [u8], owner: Pubkey) -> AccountInfo<'static> {
        AccountInfo::new(
            Box::leak(Box::new(key)), false, true, Box::leak(Box::new(0u64)),
            data, Box::leak(Box::new(owner)), false, 0,
        )
    }

    struct Pool {
        lb_pair: Pubkey,
        position: Pubkey,
        accounts: &'static [AccountInfo<'static>], // [lb pair, bin array, position]
        core: Core,
        state: Account<'static, StateMap>,
    }

    /// One pool: Core's position covers bins 0..69 with the floor (stablecoin) at bin 0 and the
    /// ceiling (IRMA) at bin 2. `lp_share_bin2` is the share of bin 2 held by another LP.
    fn pool(lp_share_bin2: u128, circulation: u128) -> Pool {
        let lb_pair = Pubkey::new_unique();
        let position = Pubkey::new_unique();
        let bin_array_key = derive_bin_array_pda(lb_pair, 0).0;

        let mut pair: LbPair = bytemuck::Zeroable::zeroed();
        pair.bin_step = 5;
        pair.active_id = 1;

        let mut bins: BinArray = bytemuck::Zeroable::zeroed();
        bins.lb_pair = lb_pair;
        bins.bins[0].amount_y = 110_000 * UNIT;
        bins.bins[0].liquidity_supply = 1_000;
        bins.bins[2].amount_x = 1_100_000 * UNIT;
        bins.bins[2].liquidity_supply = 1_000 + lp_share_bin2;

        let mut pos: PositionV2 = bytemuck::Zeroable::zeroed();
        pos.lb_pair = lb_pair;
        pos.lower_bin_id = 0;
        pos.upper_bin_id = 69;
        pos.liquidity_shares[0] = 1_000;
        pos.liquidity_shares[2] = 1_000;

        let accounts: &'static [AccountInfo<'static>] = Box::leak(vec![
            account(lb_pair, pod_data(&pair), DLMM_ID),
            account(bin_array_key, pod_data(&bins), DLMM_ID),
            account(position, pod_data(&pos), DLMM_ID),
        ].into_boxed_slice());

        let mut single = SinglePosition::default();
        single.lb_pair = lb_pair;
        single.position_pks = vec![position, position]; // Core lists the one position twice
        single.bin_array_pks = vec![bin_array_key, bin_array_key];
        let core = Core {
            owner: Pubkey::new_unique(),
            config: vec![PairConfig {
                pair_address: lb_pair.to_string(), x_amount: 0, y_amount: 0, mode: MarketMakingMode::ModeBoth,
            }],
            position_data: AllPosition { all_positions: vec![single], tokens: vec![] },
        };

        let mut reserve = StableState::new("USDT", Pubkey::new_unique(), 6).unwrap();
        reserve.mint_price = 1.001;
        reserve.backing_reserves = circulation;
        reserve.irma_in_circulation = circulation;
        reserve.pool_id = lb_pair;
        let map = StateMap { reserves: vec![reserve], bump: 0, padding: [0; 7] };
        let mut data = Vec::new();
        map.try_serialize(&mut data).unwrap();
        let info: &'static AccountInfo<'static> = Box::leak(Box::new(account(
            Pubkey::new_unique(), Box::leak(data.into_boxed_slice()), crate::ID)));
        let state = Account::<StateMap>::try_from(info).unwrap();

        Pool { lb_pair, position, accounts, core, state }
    }

    impl Pool {
        /// Apply a swap to a bin: `dx`/`dy` are the changes in the bin's token amounts.
        fn swap_in_bin(&self, bin: usize, dx: i64, dy: i64) {
            let mut data = self.accounts[1].data.borrow_mut();
            let size = std::mem::size_of::<BinArray>();
            let bins: &mut BinArray = bytemuck::from_bytes_mut(&mut data[8..8 + size]);
            bins.bins[bin].amount_x = (bins.bins[bin].amount_x as i64 + dx) as u64;
            bins.bins[bin].amount_y = (bins.bins[bin].amount_y as i64 + dy) as u64;
        }
        fn sync(&mut self) -> bool {
            self.core.sync_protocol_fills(&mut self.state, self.accounts, self.lb_pair, "USDT").unwrap()
        }
        fn books(&self) -> (u128, u128) {
            (self.state.reserves[0].backing_reserves, self.state.reserves[0].irma_in_circulation)
        }
        fn snapshot(&self) -> (u64, u64) {
            (self.core.config[0].x_amount, self.core.config[0].y_amount)
        }
    }

    #[test]
    fn classify_fill_directions() {
        assert_eq!(classify_fill((10, 5), (10, 5)), Fill::None);
        assert_eq!(classify_fill((10, 5), (7, 8)), Fill::Sale(3));
        assert_eq!(classify_fill((10, 5), (14, 1)), Fill::Buy(4));
        assert_eq!(classify_fill((10, 5), (12, 9)), Fill::Other); // liquidity added
        assert_eq!(classify_fill((10, 5), (4, 2)), Fill::Other);  // liquidity removed
        assert_eq!(classify_fill((10, 5), (9, 5)), Fill::Other);  // one-sided dust
    }

    #[test]
    fn first_sync_sets_baseline_without_booking() {
        let mut p = pool(0, 1_000);
        assert!(p.sync());
        assert_eq!(p.books(), (1_000, 1_000));
        assert_eq!(p.snapshot(), (1_100_000 * UNIT, 110_000 * UNIT));
    }

    #[test]
    fn ceiling_fill_books_stablecoin_received_net_of_fee() {
        let mut p = pool(0, 1_000);
        p.sync();
        // 10 USDT buy: 9.96 enters the bin after the 0.04 fee, 9.95 IRMA leaves
        p.swap_in_bin(2, -9_950_000, 9_960_000);
        assert!(p.sync());
        // mint_irma(9.96): backing += ceil(9.96) = 10, circulation += ceil(9.96 / 1.001) = 10
        assert_eq!(p.books(), (1_010, 1_010));
        assert_eq!(p.snapshot(), (1_100_000 * UNIT - 9_950_000, 110_000 * UNIT + 9_960_000));
    }

    #[test]
    fn fill_by_another_lp_books_nothing() {
        let mut p = pool(0, 1_000);
        p.sync();
        // the trade fills an outside LP at bin 1, where Core has no shares
        p.swap_in_bin(1, 0, 9_960_000);
        assert!(p.sync());
        assert_eq!(p.books(), (1_000, 1_000));
    }

    #[test]
    fn shared_bin_books_only_cores_share() {
        let mut p = pool(1_000, 1_000); // an LP holds half of bin 2
        p.sync();
        p.swap_in_bin(2, -20_000_000, 20_000_000);
        assert!(p.sync());
        // Core's half: 10 stablecoin -> backing +10, circulation +ceil(10 / 1.001) = 10
        assert_eq!(p.books(), (1_010, 1_010));
    }

    #[test]
    fn floor_fill_books_irma_bought_back() {
        let mut p = pool(0, 1_000);
        p.sync();
        // 10 IRMA sold into the floor: 9.96 IRMA enters after the fee, 9.95 stablecoin leaves
        p.swap_in_bin(0, 9_960_000, -9_950_000);
        assert!(p.sync());
        // redeem_irma(9.96 IRMA) -> ceil = 10 IRMA at redemption price 1
        assert_eq!(p.books(), (990, 990));
    }

    #[test]
    fn large_buy_back_is_split_under_the_redeem_limit() {
        let mut p = pool(0, 1_000_000);
        p.sync();
        p.swap_in_bin(0, 150_000 * UNIT as i64, -100_000 * UNIT as i64);
        assert!(p.sync());
        assert_eq!(p.books().1, 1_000_000 - 150_000);
    }

    #[test]
    fn pricing_error_leaves_books_unchanged_and_moves_on() {
        let mut p = pool(0, 5); // only 5 IRMA in circulation
        p.sync();
        p.swap_in_bin(0, 9_960_000, -9_950_000); // buy-back of 10 > circulation
        assert!(p.sync());
        assert_eq!(p.books(), (5, 5));
        assert_eq!(p.snapshot(), (1_100_000 * UNIT + 9_960_000, 110_000 * UNIT - 9_950_000));
    }

    #[test]
    fn missing_position_account_postpones_booking() {
        let mut p = pool(0, 1_000);
        p.sync();
        let before = p.snapshot();
        p.swap_in_bin(2, -9_950_000, 9_960_000);
        let without_position = &p.accounts[..2];
        let synced = p.core.sync_protocol_fills(&mut p.state, without_position, p.lb_pair, "USDT").unwrap();
        assert!(!synced);
        assert_eq!(p.books(), (1_000, 1_000));
        assert_eq!(p.snapshot(), before);
        // with the position passed again, the fill is booked
        assert!(p.sync());
        assert_eq!(p.books(), (1_010, 1_010));
        let _ = p.position;
    }

    #[test]
    fn missing_bin_array_or_foreign_account_postpones_booking() {
        let mut p = pool(0, 1_000);
        p.sync();
        p.swap_in_bin(2, -9_950_000, 9_960_000);
        // without the bin array
        let accounts: &'static [AccountInfo<'static>] = Box::leak(vec![p.accounts[0].clone(), p.accounts[2].clone()].into_boxed_slice());
        assert!(!p.core.sync_protocol_fills(&mut p.state, accounts, p.lb_pair, "USDT").unwrap());
        // the same bin array data, but in an account not owned by the DLMM program
        let data: &'static mut [u8] = Box::leak(p.accounts[1].data.borrow().to_vec().into_boxed_slice());
        let forged = account(*p.accounts[1].key, data, Pubkey::new_unique());
        let accounts: &'static [AccountInfo<'static>] = Box::leak(vec![p.accounts[0].clone(), forged, p.accounts[2].clone()].into_boxed_slice());
        assert!(!p.core.sync_protocol_fills(&mut p.state, accounts, p.lb_pair, "USDT").unwrap());
        assert_eq!(p.books(), (1_000, 1_000));
    }

    #[test]
    fn program_liquidity_change_is_not_a_trade() {
        let mut p = pool(0, 1_000);
        p.sync();
        // a shift withdraws the ceiling and redeposits elsewhere; store_fill_snapshot re-reads
        p.swap_in_bin(2, -100 * UNIT as i64, 0);
        p.core.store_fill_snapshot(p.accounts, p.lb_pair).unwrap();
        assert!(p.sync());
        assert_eq!(p.books(), (1_000, 1_000));
    }
}
