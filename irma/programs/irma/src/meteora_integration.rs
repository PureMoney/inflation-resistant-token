use crate::MarketMakingMode;
use crate::position_manager::*;
use commons::dlmm::accounts::*;
use commons::dlmm::types::*;
use commons::derive_event_authority_pda;
use commons::get_matching_positions;
use commons::*;
use crate::pair_config::*;
use crate::Maint;
use commons::{BASIS_POINT_MAX, DEFAULT_BIN_PER_POSITION, MAX_BIN_PER_ARRAY};
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use anchor_spl::token_interface::Mint;
use anchor_spl::token_interface::TokenAccount;
use anchor_lang::prelude::*;
use anchor_lang::prelude::program::*;
use anchor_lang::prelude::instruction::Instruction;
use anchor_lang::prelude::instruction::AccountMeta;
use anchor_lang::solana_program::clock::Clock;
use anchor_lang::solana_program::sysvar::Sysvar;
use anchor_lang::system_program;
use anchor_lang::*;
use std::collections::HashMap;
use std::str::FromStr;

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
pub struct Core<'a> {
    // pub context: &'a mut Context<'a, 'a, 'a, 'a, T>, // contains wallet and owner
    pub context: &'a mut Context<'a, 'a, 'a, 'a, Maint<'a>>,
    // pub wallet: Signer<'a>, // Option<Keypair>,
    pub owner: Pubkey,
    pub config: Vec<PairConfig>,
    pub state: AllPosition,
}

impl<'a> Core<'a> {
    // Helper function to get current epoch time in seconds (on-chain version)
    fn get_epoch_sec() -> Result<i64> {
        let clock = Clock::get()?;
        Ok(clock.unix_timestamp)
    }

    // For executing DLMM instructions via CPI
    // Derive bump if it does not exist:
    // let (_pda, bump) = Pubkey::find_program_address(
    //     &[b"irma", context.accounts.irma_admin.key().as_ref()],
    //     &crate::ID, // Your program ID
    // );
    fn get_bytemuck_account<T: bytemuck::Pod>(
        &self,
        pubkey: &Pubkey
    ) -> Result<T> {
        let account_info = self.context.remaining_accounts.iter()
            .find(|acc| acc.key == pubkey)
            .ok_or(CustomError::AccountNotFound)?;
        
        let data: T = bytemuck::pod_read_unaligned(&account_info.data.borrow()[8..]);
        Ok(data)
    }

    fn get_multiple_bytemuck_accounts<T: bytemuck::Pod>(
        &self,
        pubkeys: &Vec<Pubkey>
    ) -> Result<HashMap<Pubkey, Option<T>>> {
        let mut data = HashMap::new();
        for pubkey in pubkeys.iter() {
            let account_info = self.context.remaining_accounts.iter()
                .find(|acc| acc.key == pubkey);
            if let Some(account_info) = account_info {
                let account_data: T = bytemuck::pod_read_unaligned(&account_info.data.borrow()[8..]);
                data.insert(*pubkey, Some(account_data));
            } else {
                data.insert(*pubkey, None);
            }
        }
        Ok(data)
    }

    fn get_multiple_anchor_accounts<T: anchor_lang::AccountDeserialize>(
        &self,
        pubkeys: &Vec<Pubkey>
    ) -> Result<HashMap<Pubkey, Option<T>>> {
        let mut data = HashMap::new();
        for pubkey in pubkeys.iter() {
            let account_info = self.context.remaining_accounts.iter()
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
        &self,
        instructions: Vec<Instruction>,
        sign: bool
    ) -> Result<()> {

        let key = self.context.accounts.irma_admin.key();
        for instruction in instructions.iter() {
            if sign {
                // If PDA signing needed - manually derive bump
                let (_pda, bump) = Pubkey::find_program_address(
                    &[b"irma", key.as_ref()],
                    &crate::ID,
                );
                let seeds = &[
                    b"irma",
                    key.as_ref(),
                    &[bump],
                ];
                invoke_signed(&instruction, self.context.remaining_accounts, &[seeds])?;
            }
            else {
                invoke(&instruction, self.context.remaining_accounts)?;
            }
        }
        Ok(())
    }




    pub fn refresh_state(&mut self) -> Result<()> {

        for pair in self.config.iter() {
            let pair_address =
                Pubkey::from_str(&pair.pair_address).unwrap();

            // let lb_pair_state: LbPair = self.get_bytemuck_account(&pair_address)?;

            // get all position with an user
            let mut position_key_with_state = get_matching_positions(
                self.context.remaining_accounts,
                &self.owner, 
                &pair_address
            ).unwrap();

            let mut position_pks = vec![];
            let mut positions = vec![];
            let mut min_bin_id = 0;
            let mut max_bin_id = 0;
            let mut bin_arrays = HashMap::<Pubkey, BinArray>::new();

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

                for (key, state) in position_key_with_state.iter() {
                    position_pks.push(*key);
                    positions.push(state.to_owned());
                }

                let bin_array_keys = position_key_with_state
                    .iter()
                    .filter_map(|(_key, state)| state.get_bin_array_keys_coverage().ok())
                    .flatten()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                let bin_arrays_raw: HashMap::<Pubkey, Option<BinArray>> = self.get_multiple_bytemuck_accounts(&bin_array_keys)?;

                for (key, bin_array_option) in bin_arrays_raw.iter() {
                    if let Some(bin_array_state) = bin_array_option {
                        bin_arrays.insert(*key, *bin_array_state);
                    }
                }
            }

            let all_state = &mut self.state; // .lock().unwrap();
            let state = all_state.all_positions.get_mut(&pair_address).unwrap();

            state.lb_pair = pair_address; // Some(lb_pair);
            state.bin_arrays = bin_arrays;
            state.position_pks = position_pks;
            state.positions = positions;
            state.min_bin_id = min_bin_id;
            state.max_bin_id = max_bin_id;
            state.last_update_timestamp = Self::get_epoch_sec()?.max(0) as u64;
        }

        Ok(())
    }

    pub fn fetch_token_info(&mut self) -> Result<()> {
        let token_mints_with_program = self.get_all_token_mints_with_program_id()?;

        let token_mint_keys = token_mints_with_program
            .iter()
            .map(|(key, _program_id)| *key)
            .collect::<Vec<_>>();

        let accounts: HashMap<Pubkey, Option<Mint>> = self.get_multiple_anchor_accounts(&token_mint_keys)?;
        let mut tokens = HashMap::new();

        for ((_key, program_id), account) in token_mints_with_program.iter().zip(accounts) {
            if let (pubkey, Some(mint)) = account {
                tokens.insert(pubkey, (mint, *program_id));
            }
        }
        let state = &mut self.state; // .lock().unwrap();
        state.tokens = tokens;

        Ok(())
    }

    fn get_all_token_mints_with_program_id(&self) -> Result<Vec<(Pubkey, Pubkey)>> {
        let state = &self.state;
        let mut token_mints_with_program = vec![];

        for (_, position) in state.all_positions.iter() {
            let lb_pair_state = &position.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
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
        let state = &self.state;
        let position = state.all_positions.get(&lp_pair).unwrap();
        position.clone()
    }

    // Helper function to get or create ATA on-chain
    fn get_or_create_ata(
        &self,
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
        let ata_exists = self.context.remaining_accounts.iter()
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
            self.execute_meteora_instruction(vec![create_ata_ix], true)?;
        }

        Ok(ata_address)
    }

    pub fn init_user_ata(
        &self
    ) -> Result<()> {
        let wallet = &self.context.accounts.irma_admin;
        for (token_mint, program_id) in self.get_all_token_mints_with_program_id()?.iter() {
            self.get_or_create_ata(
                *token_mint,
                *program_id,
                &wallet.key(),
                wallet,
            )?;
        }

        Ok(())
    }

    // withdraw all positions
    pub fn withdraw(
        &self,
        state: &SinglePosition
    ) -> Result<()> {
        if state.position_pks.len() == 0 {
            return Ok(());
        }

        // let rpc_client = self.rpc_client();

        let payer = self.context.accounts.irma_admin.clone();

        let (event_authority, _bump) = derive_event_authority_pda();

        let lb_pair = state.lb_pair;
        let lb_pair_state = state.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;

        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;

        let mut remaining_account_info = RemainingAccountsInfo { slices: vec![] };
        let mut transfer_hook_remaining_accounts = vec![];

        if let Some((slices, remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                self.context.remaining_accounts,
                ActionType::Liquidity,
            )?
            // .await?
        {
            remaining_account_info.slices = slices;
            transfer_hook_remaining_accounts = remaining_accounts;
        }

        for (i, &position) in state.position_pks.iter().enumerate() {
            let position_state = &state.positions[i];

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
                position,
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
                position,
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
                position,
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

            let _result = self.execute_meteora_instruction(instructions, true)?;
            msg!("Close position {position} {result}");
        }

        Ok(())
    }



    // We may need this to overcome AMM behavior, in case off-chain swap is too slow.
    pub fn swap(
        &self,
        state: &SinglePosition,
        amount_in: u64,
        swap_for_y: bool
    ) -> Result<()> {

        let lb_pair_state = state.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;
        let [token_x_program, token_y_program] = lb_pair_state.get_token_programs()?;
        let lb_pair = state.lb_pair;

        let payer = self.context.accounts.irma_admin.clone();

        let (event_authority, _bump) = derive_event_authority_pda();
        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);

        let bitmap_extension: BinArrayBitmapExtension = 
            self.get_bytemuck_account(&bin_array_bitmap_extension)?;

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

        if let Some((slices, transfer_hook_remaining_accounts)) =
            get_potential_token_2022_related_ix_data_and_accounts(
                &lb_pair_state,
                self.context.remaining_accounts,
                ActionType::Liquidity,
            )?
            // .await?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts.extend(transfer_hook_remaining_accounts);
        }

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
            min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y)?,
            remaining_accounts_info,
        }
        .data();

        let accounts = [main_accounts.to_vec(), remaining_accounts].concat();

        let swap_ix = Instruction {
            program_id: DLMM_ID,
            accounts,
            data,
        };

        // let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        let instructions = [swap_ix];

        let _result = self.execute_meteora_instruction(instructions.to_vec(), true)?;
        msg!("Swap {amount_in} {swap_for_y} {result:?}");

        Ok(())
    }

    pub fn deposit(
        &self,
        state: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
        active_id: i32
    ) -> Result<()> {
        let payer = self.context.accounts.irma_admin.clone();

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

        let mut instructions = vec![];

        for idx in lower_bin_array_idx..=upper_bin_array_idx {
            // Initialize bin array if not exists
            let (bin_array, _bump) = derive_bin_array_pda(lb_pair, idx.into());

            if self.get_bytemuck_account::<BinArray>(&bin_array).is_err() {
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

        // fake it for now
        let position = *state.position_pks.first().ok_or(
                Error::from(CustomError::PositionNotFound)
            )?;

        let accounts = dlmm::client::accounts::InitializePosition {
            lb_pair,
            payer: payer.key(),
            position,
            owner: payer.key(),
            rent: sysvar::rent::ID,
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

        // TODO implement add liquidity by strategy imbalance
        let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
        // let bin_array_bitmap_extension = get_account(&bin_array_bitmap_extension)
        //     .map(|_| bin_array_bitmap_extension)
        //     .unwrap_or(DLMM_ID);

        let (bin_array_lower, _bump) = derive_bin_array_pda(lb_pair, lower_bin_array_idx.into());
        let (bin_array_upper, _bump) = derive_bin_array_pda(lb_pair, upper_bin_array_idx.into());

        let lb_pair_state = state.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;
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
                self.context.remaining_accounts,
                ActionType::Liquidity,
            )?
            // .await?
        {
            remaining_accounts_info.slices = slices;
            remaining_accounts.extend(transfer_hook_remaining_accounts);
        }

        remaining_accounts.extend(
            [bin_array_lower, bin_array_upper]
                .into_iter()
                .map(|k| AccountMeta::new(k, false)),
        );

        // fake it for now
        let position = *state.position_pks.first().ok_or(
                Error::from(CustomError::PositionNotFound)
            )?;

        let main_accounts = dlmm::client::accounts::AddLiquidityByStrategy2 {
            lb_pair,
            position,
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

        let _result = self.execute_meteora_instruction(instructions, true)?;
        msg!("deposit {amount_x} {amount_y} {_result}");

        Ok(())
    }

    pub fn get_deposit_amount(
        &self,
        position: &SinglePosition,
        amount_x: u64,
        amount_y: u64,
    ) -> Result<(u64, u64)> {
        let lb_pair_state = position.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;

        // let rpc_client = self.rpc_client();
        let payer = self.context.accounts.irma_admin.clone();

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

        let accounts: HashMap<Pubkey, Option<TokenAccount>> = self.get_multiple_anchor_accounts(&vec![user_token_x, user_token_y])?;

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
        let state = &self.state;
        let mut positions = vec![];
        for (_, position) in &state.all_positions {
            positions.push(position.clone());
        }
        positions
    }

    pub fn get_all_tokens(&self) -> HashMap<Pubkey, MintWithProgramId> {
        let state = &self.state;
        state.tokens.clone()
    }

    pub fn get_positions(&self) -> Result<Vec<PositionInfo>> {
        let all_positions = self.get_all_positions();
        let tokens = self.get_all_tokens();

        let mut position_infos = vec![];
        for position in all_positions.iter() {
            let lb_pair_state = &position.lb_pair_state.ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;
            let x_decimals = get_decimals(lb_pair_state.token_x_mint, &tokens);
            let y_decimals = get_decimals(lb_pair_state.token_y_mint, &tokens);
            let position_raw = position.get_positions()?;
            position_infos.push(position_raw.to_position_info(x_decimals, y_decimals)?);
        }
        return Ok(position_infos);
    }

    pub fn check_shift_price_range(&self) -> Result<()> {
        let all_positions = self.get_all_positions();
        for position in all_positions.iter() {
            let pair_config = get_pair_config(&self.config, position.lb_pair);
            // check whether out of price range
            let lb_pair = &position.lb_pair_state.as_ref().ok_or(
                Error::from(CustomError::MissingLbPairState)
            )?;
            if pair_config.mode == MarketMakingMode::ModeRight
                && lb_pair.active_id > position.max_bin_id
            {
                // self.shift_right(&position)?;
                // self.inc_rebalance_time(position.lb_pair);
            }

            if pair_config.mode == MarketMakingMode::ModeLeft
                && lb_pair.active_id < position.min_bin_id
            {
                // self.shift_left(&position)?;
                // self.inc_rebalance_time(position.lb_pair);
            }

            if pair_config.mode == MarketMakingMode::ModeBoth {
                if lb_pair.active_id < position.min_bin_id {
                    // self.shift_left(&position)?;
                    // self.inc_rebalance_time(position.lb_pair);
                } else if lb_pair.active_id > position.max_bin_id {
                    // self.shift_right(&position)?;
                    // self.inc_rebalance_time(position.lb_pair);
                }
            }
        }

        Ok(())
    }

    pub fn inc_rebalance_time(&mut self, lb_pair: Pubkey) {
        if let Some(state) = self.state.all_positions.get_mut(&lb_pair) {
            state.inc_rebalance_time();
        }
    }
}

#[cfg(test)]
mod core_test {
    use super::*;
    use std::env;
    use std::sync::Arc;

    #[test]
    fn test_withdraw() {
        let wallet = env::var("MM_WALLET").unwrap();
        let cluster = env::var("MM_CLUSTER").unwrap();
        let payer = read_keypair_file(wallet.clone()).unwrap();

        let lp_pair = Pubkey::from_str("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX").unwrap();

        let config = vec![PairConfig {
            pair_address: lp_pair.to_string(),
            x_amount: 17000000,
            y_amount: 2000000,
            mode: MarketMakingMode::ModeBoth,
        }];

        let core = &Core {
            context: /*Arc::new(*/MeteoraContext {
                accounts: MeteoraAccounts {
                    irma_admin: payer.clone(),
                },
                remaining_accounts: vec![],
            }/*)*/,
            owner: payer.key(),
            // wallet: Some(Arc::new(payer)),
            config: config.clone(),
            state: AllPosition::new(&config).unwrap(),
        };

        core.refresh_state().unwrap();

        let state = core.get_position_state(lp_pair);

        // withdraw
        core.withdraw(&state).unwrap();
    }

    #[test]
    fn test_swap() {
        let wallet = env::var("MM_WALLET").unwrap();
        let cluster = env::var("MM_CLUSTER").unwrap();
        let payer = read_keypair_file(wallet.clone()).unwrap();

        let lp_pair = Pubkey::from_str("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX").unwrap();

        let config = vec![PairConfig {
            pair_address: lp_pair.to_string(),
            x_amount: 17000000,
            y_amount: 2000000,
            mode: MarketMakingMode::ModeBoth,
        }];

        let core = &Core {
            context: /*Arc::new(*/MeteoraContext {
                accounts: MeteoraAccounts {
                    irma_admin: payer.clone(),
                },
                remaining_accounts: vec![],
            }/*)*/,
            owner: payer.key(),
            // wallet: Some(Arc::new(payer)),
            config: config.clone(),
            state: AllPosition::new(&config).unwrap(),
        };

        core.refresh_state().unwrap();

        let state = core.get_position_state(lp_pair);

        core.swap(&state, 1000000, true).unwrap();
    }
}

