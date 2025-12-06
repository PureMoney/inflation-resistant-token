
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod core_test {
    use super::*;
    // use super::*;
    use anchor_lang::prelude::*;
    use std::env;
    // use std::mem::size_of;
    // use std::sync::Arc;
    use irma::IRMA_ID;
    use irma::pair_config::PairConfig;
    use irma::position_manager::{AllPosition};
    use irma::pricing::init_pricing;
    use irma::pricing::MAX_BACKING_COUNT;
    use irma::pricing::StateMap;
    use irma::meteora_integration::Core;
    use irma::{MarketMakingMode, Init, Maint, InitBumps, MaintBumps};
    use commons::dlmm::accounts::{LbPair, PositionV2};
    use commons::dlmm::types::{UserRewardInfo, FeeInfo, StaticParameters, VariableParameters, ProtocolFee, RewardInfo};
    use anchor_lang::prelude::borsh::BorshSerialize;

    // Use SystemTime
    fn get_current_time_test() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    // Helper function to create mock AccountInfo
    fn create_mock_account_info<'a>(
        key: &'a Pubkey,
        lamports: &'a mut u64,
        data: &'a mut [u8],
        owner: &'a Pubkey,
    ) -> AccountInfo<'a> {
        AccountInfo::new(
            key,
            false, // is_signer
            false, // is_writable  
            lamports,
            data,
            owner,
            false, // executable
            0,     // rent_epoch
        )
    }

    // Usage example:
    // let mut position_data = vec![0u8; std::mem::size_of::<PositionV2>()];
    // let mut lamports = 0u64;
    // let position_pubkey = Pubkey::new_unique();
    // let owner = Pubkey::new_unique();

    // let position_account_info = create_mock_account_info(
    //     &position_pubkey,
    //     &mut lamports,
    //     &mut position_data,
    //     &owner,
    // );

    // Then use in remaining_accounts
    // remaining_accounts: &[position_account_info],

    fn allocate_state() -> StateMap {
        StateMap::new()
    }

    fn create_position(lb_pair: &Pubkey, owner: &Pubkey) -> PositionV2 {
        // Debug: Print the expected size
        println!("----Expected PositionV2 size: {}", std::mem::size_of::<PositionV2>());
        println!("    UserRewardInfo size: {}", std::mem::size_of::<UserRewardInfo>());
        println!("    FeeInfo size: {}", std::mem::size_of::<FeeInfo>());

        PositionV2 {
            lb_pair: *lb_pair,
            owner: *owner,
            liquidity_shares: [0u128; 70],
            reward_infos: [UserRewardInfo::default(); 70],
            fee_infos: [FeeInfo::default(); 70],
            lower_bin_id: -50i32,
            upper_bin_id: 50i32,
            last_updated_at: get_current_time_test() as i64,
            total_claimed_fee_x_amount: 0u64,
            total_claimed_fee_y_amount: 0u64,
            total_claimed_rewards: [0u64; 2],
            operator: Pubkey::new_unique(),
            lock_release_point: 0u64,
            fee_owner: Pubkey::new_unique(),
            _reserved: [0u8; 87],
            _padding_0: 0u8,
        }
    }

    fn prep_accounts<'info>(
            owner: &'info Pubkey, // program owner, not user owner
            state_account: Pubkey,
            lb_pair: &'info Pubkey
        ) -> (
            AccountInfo<'info>,
            AccountInfo<'info>,
            AccountInfo<'info>,
            AccountInfo<'info>,
            AccountInfo<'info>,
            AccountInfo<'info>,
        ) {
        // Create a buffer for StateMap and wrap it in AccountInfo
        let lamports: &mut u64 = Box::leak(Box::new(100000u64));
        let mut state: StateMap = allocate_state();
        // let _ = state.init_reserves(); // Add initial stablecoins to the state

        // Prepare the account data with the correct discriminator
        let mut state_data_vec: Vec<u8> = Vec::with_capacity(120*MAX_BACKING_COUNT);
        state.try_serialize(&mut state_data_vec).unwrap();

        let state_data: &'info mut Vec<u8> = Box::leak(Box::new(state_data_vec));
        let state_key: &'info mut Pubkey = Box::leak(Box::new(state_account));
        // msg!("StateMap pre-test account data: {:?}", state_data);
        let state_account_info: AccountInfo<'info> = AccountInfo::new(
            state_key,
            false, // is_signer
            true,  // is_writable
            lamports,
            state_data,
            owner,
            false,
            0,
        );

        let config = vec![PairConfig {
            pair_address: lb_pair.to_string(),
            x_amount: 17000000,
            y_amount: 2000000,
            mode: MarketMakingMode::ModeBoth,
        }];

        let signer_pubkey: &'info mut Pubkey 
            = Box::leak(Box::new(Pubkey::from_str_const("68bjdGBTr4yRxLW56s7LvpQehMn9jBvaJvV134NQjpmP")));

        let lamportsc: &mut u64 = Box::leak(Box::new(1000000u64));
        let all_positions = AllPosition::new(&config).unwrap().all_positions;
        let all_positions = all_positions.into_iter().map(|pe| pe.lb_pair).collect::<Vec<_>>();
        let core_state = &mut Core::create_core(
            *signer_pubkey, // owner
            all_positions,
        ).unwrap();
        let mut core_data_vec: Vec<u8> = Vec::with_capacity(std::mem::size_of::<Core>());
        core_state.try_serialize(&mut core_data_vec).unwrap();
        let core_data: &'info mut Vec<u8> = Box::leak(Box::new(core_data_vec));
        let core_key: &'info mut Pubkey = Box::leak(Box::new(Pubkey::new_unique()));
        let core_account_info: AccountInfo<'info> = AccountInfo::new(
            core_key,
            false, // is_signer
            true,  // is_writable
            lamportsc,
            core_data,
            owner,
            false,
            0,
        );
        
        // msg!("StateMap account created: {:?}", state_account_info.key);
        // msg!("StateMap owner: {:?}", owner);
        // Use a mock Signer for testing purposes
        let lamportsx: &'info mut u64 = Box::leak(Box::new(0u64));
        let data: &'info mut Vec<u8> = Box::leak(Box::new(vec![]));
        let owner: &'info mut Pubkey = Box::leak(Box::new(Pubkey::default()));
        let signer_account_info: AccountInfo<'info> = AccountInfo::new(
            signer_pubkey,
            true, // is_signer
            false, // is_writable
            lamportsx,
            data,
            owner,
            false,
            0,
        );

        // Create LbPair account info
        let lb_pair_state: LbPair = LbPair {
            parameters: StaticParameters::default(),
            v_parameters: VariableParameters::default(),
            bump_seed: [0u8; 1],
            bin_step_seed: [0u8; 2],
            pair_type: 0u8,
            active_id: 0i32,
            bin_step: 100u16,
            status: 0u8,
            require_base_factor_seed: 0u8,
            base_factor_seed: [0u8; 2],
            activation_type: 0u8,
            creator_pool_on_off_control: 0u8,
            token_x_mint: Pubkey::new_unique(),
            token_y_mint: Pubkey::new_unique(),
            reserve_x: Pubkey::new_unique(),
            reserve_y: Pubkey::new_unique(),
            protocol_fee: ProtocolFee {
                amount_x: 0u64,
                amount_y: 0u64,
            },
            _padding_1: [0u8; 32],
            reward_infos: [RewardInfo {
                mint: Pubkey::new_unique(),
                vault: Pubkey::new_unique(),
                funder: Pubkey::new_unique(),
                reward_duration: 0u64,
                reward_duration_end: 0u64,
                reward_rate: 0u128,
                last_update_time: 0u64,
                cumulative_seconds_with_empty_liquidity_reward: 0u64,
            }; 2],
            oracle: Pubkey::new_unique(),
            bin_array_bitmap: [0u64; 16],
            last_updated_at: get_current_time_test() as i64,
            _padding_2: [0u8; 32],
            pre_activation_swap_address: Pubkey::default(),
            base_key: *lb_pair,
            activation_point: 0u64,
            pre_activation_duration: 0u64,
            _padding_3: [0u8; 8],
            _padding_4: 0u64,
            creator: Pubkey::default(),
            token_mint_x_program_flag: 0u8,
            token_mint_y_program_flag: 0u8,
            _reserved: [0u8; 22],
        };
        let lb_pair_data_vec = bytemuck::bytes_of(&lb_pair_state).to_vec();
        let mut lb_pair_data = vec![33, 11, 49, 98, 181, 101, 177, 13]; // discriminator
        lb_pair_data.extend_from_slice(&lb_pair_data_vec);
        let lb_pair_data: &'info mut Vec<u8> = Box::leak(Box::new(lb_pair_data));
        let lb_pair_lamports: &mut u64 = Box::leak(Box::new(100000u64));
        let lb_pair_owner: &'info mut Pubkey = Box::leak(Box::new(Pubkey::default()));
        let lb_pair_account_info: AccountInfo<'info> = AccountInfo::new(
            lb_pair,
            false, // is_signer
            false,  // is_writable
            lb_pair_lamports,
            lb_pair_data,
            lb_pair_owner,
            false,
            0,
        );

        // second parameter is the user owner of the position
        let position: PositionV2 = create_position(lb_pair, signer_pubkey);
        // msg!("    Created PositionV2 for testing: {:?}", position);
        // msg!("    Size of PositionV2: {}", std::mem::size_of::<PositionV2>());
        let lamports: &mut u64 = Box::leak(Box::new(std::mem::size_of::<PositionV2>() as u64));
        
        // Serialize using bytemuck (for Pod types)
        let position_data_vec = bytemuck::bytes_of(&position).to_vec();
        
        // Add discriminator (8 bytes) at the beginning if needed for account format
        let mut full_data = vec![117, 176, 212, 199, 245, 180, 133, 182]; // discriminator
        full_data.extend_from_slice(&position_data_vec);
        msg!("    Full PositionV2 account data length: {}", full_data.len());
        msg!("    Expected PositionV2 account data length: {}", 8 + std::mem::size_of::<PositionV2>());
        
        // Use pod_read_unaligned to handle alignment issues in tests
        if full_data.len() >= 8 + std::mem::size_of::<PositionV2>() {
            let pos = bytemuck::pod_read_unaligned::<PositionV2>(&full_data[8..8 + std::mem::size_of::<PositionV2>()]);
            msg!("    PositionV2 account owner: {:?}", pos.owner.to_string());
        }
        
        let position_data: &'info mut Vec<u8> = Box::leak(Box::new(full_data));
        let position_key: &'info mut Pubkey = Box::leak(Box::new(Pubkey::new_unique()));
        let position_account_info: AccountInfo<'info> = AccountInfo::new(
            position_key,
            false, // is_signer
            true,  // is_writable
            lamports,
            position_data,
            owner,
            false,
            0,
        );

        // Create AccountInfo for system_program
        let sys_lamports: &'info mut u64 = Box::leak(Box::new(0u64));
        let sys_data: &'info mut Vec<u8> = Box::leak(Box::new(vec![]));
        let sys_owner: &'info mut Pubkey = Box::leak(Box::new(Pubkey::default()));
        let sys_account_info: AccountInfo<'info> = AccountInfo::new(
            &system_program::ID,
            false, // is_signer
            false, // is_writable
            sys_lamports,
            sys_data,
            sys_owner,
            true,
            0,
        );
        (state_account_info,
            signer_account_info,
            sys_account_info,
            position_account_info,
            lb_pair_account_info,
            core_account_info)
    }

    fn initialize_anchor<'info>(
        program_id: &'info Pubkey, 
        lb_pair: &'info Pubkey
    ) -> (Account<'info, StateMap>,
            Signer<'info>, 
            Program<'info, anchor_lang::system_program::System>,
            AccountInfo<'info>,
            AccountInfo<'info>,
            Account<'info, Core>) {
        //                 state_account_info: &'info AccountInfo<'info>) {
        //                 sys_account_info: &AccountInfo<'info>) {
        // let program_id: &'info Pubkey = Box::leak(Box::new(Pubkey::new_from_array(irma::ID.to_bytes())));
        let state_account: Pubkey = Pubkey::find_program_address(&[b"state".as_ref()], program_id).0;
        let (state_account_info, 
            irma_admin_account_info,
            sys_account_info,
            position_account_info,
            lb_pair_account_info,
            core_account_info)
                 = prep_accounts(program_id, state_account, lb_pair);
        // Bind to variables to extend their lifetime
        let state_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(state_account_info));
        let irma_admin_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(irma_admin_account_info));
        let sys_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(sys_account_info));
        let core_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(core_account_info));
        let mut accounts: Init<'_> = Init {
            state: Account::try_from(state_account_static).unwrap(),
            irma_admin: Signer::try_from(irma_admin_account_static).unwrap(),
            system_program: Program::try_from(sys_account_static).unwrap(),
            core: Account::try_from(core_account_static).unwrap(),
        };
        let mut ctx: Context<Init> = Context::new(
            program_id,
            &mut accounts,
            &[],
            InitBumps::default(), // Use default bumps if not needed
        );
        let result: std::result::Result<(), Error> = init_pricing(&mut ctx);
        assert!(result.is_ok());
        // msg!("StateMap account: {:?}", accounts.state);
        return (accounts.state,
            accounts.irma_admin,
            accounts.system_program,
            position_account_info,
            lb_pair_account_info,
            accounts.core);
    }

    #[test]
    fn test_withdraw() {
        let program_id: &Pubkey = &IRMA_ID;

        let lb_pair = Pubkey::from_str_const("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX");
        let (state_account,
            irma_admin_account,
            sys_account,
            position_account_info,
            lb_pair_account_info,
            core_account)
                = initialize_anchor(program_id, &lb_pair);

        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };

        // Clone core before creating the mutable context
        let mut core = accounts.core.clone();
        
        let remaining_accounts: &[AccountInfo] = &[position_account_info];
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            remaining_accounts,
            MaintBumps::default(), // Use default bumps if not needed
        );

        core.refresh_position_data(
            &state_account.reserves,
            remaining_accounts,
            "BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k".to_string() // devUSDC
        ).unwrap();

        let state = {
            let mut_state = core.get_mut_position_state(lb_pair);
            let lb_pair_data = &lb_pair_account_info.data.borrow()[8..];
            // let lb_pair_state = bytemuck::pod_read_unaligned::<LbPair>(
            //     &lb_pair_data
            // );
            // mut_state.lb_pair_state = Some(lb_pair_state);
            mut_state.clone() // Clone the state to end the mutable borrow
        };

        // withdraw - now we can borrow core immutably
        core.withdraw(&ctx, &state).unwrap();
    }

    #[test]
    fn test_swap() {
        let program_id: &Pubkey = &IRMA_ID;

        let lb_pair = Pubkey::from_str_const("FoSDw2L5DmTuQTFe55gWPDXf88euaxAEKFre74CnvQbX");
        let (state_account,
            irma_admin_account,
            sys_account,
            position_account_info,
            lb_pair_account_info,
            core_account)
                = initialize_anchor(program_id, &lb_pair);

        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };

        // Clone core before creating the mutable context
        let mut core = accounts.core.clone();
        
        let remaining_accounts: &[AccountInfo] = &[position_account_info];
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            remaining_accounts,
            MaintBumps::default(), // Use default bumps if not needed
        );

        core.refresh_position_data(
            &state_account.reserves,
            remaining_accounts,
            "BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k".to_string() // devUSDC
        ).unwrap();

        let state = {
            let mut_state = core.get_mut_position_state(lb_pair);
            let lb_pair_data = &lb_pair_account_info.data.borrow()[8..];
            // let lb_pair_state = bytemuck::pod_read_unaligned::<LbPair>(
            //     &lb_pair_data
            // );
            // mut_state.lb_pair_state = Some(lb_pair_state);
            mut_state.clone() // Clone the state to end the mutable borrow
        };

        core.swap(&ctx, &state, 1000000, true).unwrap();
    }
}
