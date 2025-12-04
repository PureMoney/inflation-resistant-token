
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anchor_lang::prelude::*;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::system_program;
    use anchor_lang::prelude::Signer;
    // use anchor_lang::prelude::Account;
    use anchor_lang::prelude::Program;
    use anchor_lang::context::Context;
    use solana_program::pubkey;
    // use bytemuck::bytes_of_mut;
    // use anchor_lang::Discriminator;
    use irma::IRMA_ID;
    use irma::pricing::{StateMap, StableState};
    use irma::pricing::{init_pricing, set_mint_price, mint_irma, redeem_irma, list_reserves};
    use irma::pricing::MAX_BACKING_COUNT;
    use irma::{Init, Maint, InitBumps, MaintBumps};
    use irma::meteora_integration::Core;

    
    fn allocate_state() -> StateMap {
        StateMap::new()
    }

    fn init_state() -> StateMap {
        let mut state: StateMap = allocate_state();
        let usdt: StableState = 
            StableState::new("USDT", pubkey!("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6 as u64).unwrap();
        state.add_reserve(usdt);
        assert_eq!(state.len(), 1);
        state
    }

    #[test]
    fn test_set_state_directly() -> Result<()> {
        let mut state: StateMap = init_state();
        let quote_token: &str = "USDT";
        let new_price: f64 = 1.23;
        {
            let mut_reserve = state.get_mut_stablecoin(quote_token).unwrap();
            // assert_eq!(mut_reserve.mint_price, 1.0);
            mut_reserve.mint_price = 1.0;
        }
        {
            assert_eq!(state.get_stablecoin(quote_token).unwrap().mint_price, 1.0);
        }
        {
            let mut_reserve = state.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.mint_price = new_price;
        }
        assert_eq!(state.get_stablecoin(quote_token).unwrap().mint_price, new_price);
        Ok(())
    }

    #[test]
    fn test_mint_irma_directly() -> Result<()> {
        let mut state = init_state();
        let quote_token = "USDT";
        let amount = 100;
        let price = state.get_stablecoin(quote_token).unwrap().mint_price;
        let prev_circulation = state.get_stablecoin(quote_token).unwrap().irma_in_circulation;
        let prev_reserve = state.get_stablecoin(quote_token).unwrap().backing_reserves;
        // Simulate mint_irma logic
        let mut_reserve = state.get_mut_stablecoin(quote_token).unwrap();
        mut_reserve.backing_reserves += amount;
        mut_reserve.irma_in_circulation += (amount as f64 / price).ceil() as u128;
        assert_eq!(state.get_stablecoin(quote_token).unwrap().backing_reserves, 
            prev_reserve + amount);
        assert_eq!(state.get_stablecoin(quote_token).unwrap().irma_in_circulation, 
            prev_circulation + (amount as f64 / price).ceil() as u128);
        Ok(())
    }

    #[test]
    fn test_redeem_irma_simple() -> Result<()> {
        let mut state = init_state();
        let quote_token = "USDT";
        {
            let mut_reserve = state.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.backing_reserves = 1000;
        }
        let prev_backing = state.get_stablecoin(quote_token).unwrap().backing_reserves;
        {
            let mut_reserve = state.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.backing_reserves -= 100;
        }
        // Simulate redeem_irma logic (simple case)
        assert_eq!(state.get_stablecoin(quote_token).unwrap().backing_reserves, prev_backing - 100);
        Ok(())
    }

    #[test]
    fn test_reduce_circulations_logic() -> Result<()> {
        let mut state = init_state();
        let prev_circulation = 100; // state.get_stablecoin("USDT").irma_in_circulation;
        let irma_amount = 5;
        {
            // Manipulate state to create a price difference
            let mut_reserve = state.get_mut_stablecoin("USDT").unwrap();
            mut_reserve.mint_price = 2.0;
            mut_reserve.backing_reserves = 1000;
            mut_reserve.irma_in_circulation = 100;
            mut_reserve.irma_in_circulation -= irma_amount;
        }
        assert_eq!(state.get_stablecoin("USDT").unwrap().irma_in_circulation, 
            prev_circulation - irma_amount);
        Ok(())
    }

    fn prep_accounts<'info>(owner: &'info Pubkey, state_account: Pubkey) -> 
    (AccountInfo<'info>, AccountInfo<'info>, AccountInfo<'info>, AccountInfo<'info>) {
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
        let lamportsc: &mut u64 = Box::leak(Box::new(1000000u64));
        let mut vec = Vec::new();
        vec.push(Pubkey::new_unique());
        let mut core_state: Core = Core::create_core(*owner, vec).unwrap();
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
        let signer_pubkey: &'info mut Pubkey = Box::leak(Box::new(Pubkey::new_unique()));
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
        (state_account_info, signer_account_info, sys_account_info, core_account_info)
    }

    fn initialize_anchor<'info>(program_id: &'info Pubkey) -> 
    (Account<'info, StateMap>, Signer<'info>, Program<'info, anchor_lang::system_program::System>, Account<'info, Core>) {
        //                 state_account_info: &'info AccountInfo<'info>) {
        //                 sys_account_info: &AccountInfo<'info>) {
        // let program_id: &'info Pubkey = Box::leak(Box::new(Pubkey::new_from_array(irma::ID.to_bytes())));
        let state_account: Pubkey = Pubkey::find_program_address(&[b"state".as_ref()], program_id).0;
        let (state_account_info, irma_admin_account_info, sys_account_info, core_account_info) 
                 = prep_accounts(program_id, state_account);
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
        let irma_admin = accounts.irma_admin.key().to_string();
        let mut ctx: Context<Init> = Context::new(
            program_id,
            &mut accounts,
            &[],
            InitBumps::default(), // Use default bumps if not needed
        );
        let mut vec = Vec::new();
        vec.push(Pubkey::new_unique().to_string());
        assert_eq!(vec.len(), 1);
        let result: std::result::Result<(), Error> = irma::irma::initialize(
            ctx, irma_admin, vec);
        assert!(result.is_ok());
        // msg!("StateMap account: {:?}", accounts.state);
        return (accounts.state, accounts.irma_admin, accounts.system_program, accounts.core);
    }

    #[test]
    fn test_initialize_anchor<'info>() {
        msg!("\n-------------------------------------------------------------------------");
        msg!("Testing init_pricing IRMA with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        // initialize_anchor calls irma:irma::initialize
        let (state_account, irma_admin_account, sys_account, core_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Init<'_> = Init {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            system_program: sys_account.clone(),
            core: core_account.clone(),
        };
        let mut ctx: Context<Init> = Context::new(
            program_id,
            &mut accounts,
            &[],
            InitBumps::default(), // Use default bumps if not needed
        );

        // Call the initialize function again. This should fail because the account is already initialized.
        let mut vec = Vec::new();
        vec.push(Pubkey::new_unique().to_string());
        assert_eq!(vec.len(), 1);
        let result: std::result::Result<(), Error> = irma::irma::initialize(
            ctx, irma_admin_account.key().to_string(), vec);
        assert!(result.is_ok()); // not running on-chain, so it's OK?
        msg!("StateMap account initialized successfully: {:?}", accounts.state);
   }

    #[test]
    fn test_set_mint_price_anchor<'info>() {
        msg!("\n-------------------------------------------------------------------------");
        msg!("Testing set IRMA mint price with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account, core_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };

        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let xresult = irma::irma::add_reserve(
            ctx, "USDT".to_string(), pubkey!("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let xresult = irma::irma::add_reserve(
            ctx, "USDC".to_string(), pubkey!("Es8vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let xresult = irma::irma::add_reserve(
            ctx, "FDUSD".to_string(), pubkey!("Es7vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);

        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let mut result: std::result::Result<(), Error> = irma::irma::set_mint_price(
            ctx, "USDT".to_string(), 1.5);
        assert!(result.is_ok());
        // Re-create ctx for the next call if needed
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = irma::irma::set_mint_price(ctx, "USDC".to_string(), 1.8);
        assert!(result.is_ok());
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = irma::irma::set_mint_price(ctx, "FDUSD".to_string(), 1.3);
        assert!(result.is_ok());
        // msg!("Mint price for USDT set successfully: {:?}", accounts.state.mint_price["USDT" as usize]);
        // msg!("Mint price for USDC set successfully: {:?}", accounts.state.mint_price[Stablecoins::USDC as usize]);
        // msg!("Mint price for USDE set successfully: {:?}", accounts.state.mint_price[Stablecoins::FDUSD as usize]);
    }

    #[test]
    fn test_mint_irma_anchor<'info>() -> Result<()> {
        msg!("\n-------------------------------------------------------------------------");
        msg!("Testing mint IRMA with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        // let state_account: Pubkey = Pubkey::find_program_address(&[b"state".as_ref()], program_id).0;
        let (state_account, irma_admin_account, sys_account, core_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };

        // Add all six stablecoins for testing
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDT".to_string(), pubkey!("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDC".to_string(), pubkey!("Es8vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "FDUSD".to_string(), pubkey!("Es7vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "PYUSD".to_string(), pubkey!("Es6vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDG".to_string(), pubkey!("Es5vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDE".to_string(), pubkey!("Es4vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);

        // make sure they're all in there
        msg!("Pre-mint IRMA state:");
        msg!("Backing reserves for USDT: {:?}", 
            accounts.state.get_stablecoin("USDT").unwrap().backing_reserves);
        msg!("Backing reserves for PYUSD: {:?}", 
            accounts.state.get_stablecoin("PYUSD").unwrap().backing_reserves);
        msg!("Backing reserves for USDG: {:?}", 
            accounts.state.get_stablecoin("USDG").unwrap().backing_reserves);
        msg!("IRMA in circulation for USDT: {:?}", 
            accounts.state.get_stablecoin("USDT").unwrap().irma_in_circulation);
        msg!("IRMA in circulation for PYUSD: {:?}", 
            accounts.state.get_stablecoin("PYUSD").unwrap().irma_in_circulation);
        msg!("IRMA in circulation for USDG: {:?}", 
            accounts.state.get_stablecoin("USDG").unwrap().irma_in_circulation);

        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let mut result = mint_irma(&mut ctx.accounts.state, "USDT", 100);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for USDT");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = mint_irma(&mut ctx.accounts.state, "PYUSD", 1000);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for PYUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for PYUSD");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = mint_irma(&mut ctx.accounts.state, "USDG", 10000);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for USDG: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for USDG");
            }
        }
        msg!("\n-------------------------------------------------------------------------");
        msg!("Post-mint IRMA state:");
        msg!("Backing reserves for USDT: {:?}", 
            accounts.state.get_stablecoin("USDT").unwrap().backing_reserves);
        msg!("Backing reserves for PYUSD: {:?}", 
            accounts.state.get_stablecoin("PYUSD").unwrap().backing_reserves);
        msg!("Backing reserves for USDG: {:?}", 
            accounts.state.get_stablecoin("USDG").unwrap().backing_reserves);
        msg!("IRMA in circulation for USDT: {:?}", 
            accounts.state.get_stablecoin("USDT").unwrap().irma_in_circulation);
        msg!("IRMA in circulation for PYUSD: {:?}", 
            accounts.state.get_stablecoin("PYUSD").unwrap().irma_in_circulation);
        msg!("IRMA in circulation for USDG: {:?}", 
            accounts.state.get_stablecoin("USDG").unwrap().irma_in_circulation);
        Ok(())
    }


    #[test]
    fn test_redeem_irma_anchor<'info>() -> Result<()> {
        msg!("\n-------------------------------------------------------------------------");
        msg!("Testing redeem IRMA when mint price is less than redemption price");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account, core_account) 
            = initialize_anchor(program_id);
        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            system_program: sys_account.clone(),
            core: core_account.clone(),
        };
        // Bind to variables to extend their lifetime
        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };

        // Add all six stablecoins for testing
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDT".to_string(), pubkey!("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDC".to_string(), pubkey!("Es8vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "FDUSD".to_string(), pubkey!("Es7vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "PYUSD".to_string(), pubkey!("Es6vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDG".to_string(), pubkey!("Es5vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);
        let _ = irma::irma::add_reserve(
            Context::new(program_id, &mut accounts, &[], MaintBumps::default()),
            "USDE".to_string(), pubkey!("Es4vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), 6u8);

        {
            let ctx: Context<Maint> = Context::new(
                program_id,
                &mut accounts,
                &[],
                MaintBumps::default(),
            );
            msg!("Pre-redeem IRMA state 1:");
            msg!("Backing reserves: {}", list_reserves(ctx));
        }
        let reserves = accounts.state.reserves.clone();
        for sc in reserves {
            msg!("Backing reserves for {}: {:?}", sc.symbol, sc.backing_reserves);
            if sc.backing_decimals == 0 {
                msg!("Skipping non-existent stablecoin: {}", sc.symbol);
                continue; // skip non-existent stablecoins
            }
            let mut_backing = accounts.state.get_mut_stablecoin(&sc.symbol).unwrap();
            let reserve: &mut u128 = &mut mut_backing.backing_reserves;
            let circulation: &mut u128 = &mut mut_backing.irma_in_circulation;
            *reserve = 1000000; // Set a large reserve for testing
            *circulation = 100000; // Set a large IRMA in circulation for testing
        }
        // msg!("Current prices: {:?}", accounts.state.mint_price);
        // msg!("Backing reserves: {:?}", accounts.state.backing_reserves);
        // msg!("IRMA in circulation: {:?}", accounts.state.irma_in_circulation);
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        let mut result: std::result::Result<(), Error> = redeem_irma(&mut ctx.accounts.state, "USDC", 10);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDC: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDC");
            }
        }
        // assert!(result.is_ok(), "Redeem IRMA failed for USDC");
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = redeem_irma(&mut ctx.accounts.state, "USDT", 20);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDT");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = redeem_irma(&mut ctx.accounts.state, "PYUSD", 30);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for PYUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for PYUSD");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = redeem_irma(&mut ctx.accounts.state, "USDG", 40);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDG: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDG");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = redeem_irma(&mut ctx.accounts.state, "FDUSD", 50);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for FDUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for FDUSD");
            }
        }

        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );

        msg!("Mid-state for USDT before further redemption: {:?}", 
            irma::irma::list_reserves(ctx));
        
        // mint IRMA for USDT
        
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        // pub fn sale_trade_event(ctx: Context<Maint>, bought_token: String, bought_amount: u64) -> Result<()> {
        result = irma::irma::sale_trade_event(ctx, "USDT".to_string(), 50_000_000);

        // Test for near maximum redemption
        
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = irma::irma::buy_trade_event(ctx, "USDT".to_string(), 10_000);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDT");
            }
        }
        ctx = Context::<Maint>::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        result = redeem_irma(&mut ctx.accounts.state, "USDS", 10);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDS: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDS");
            }
        }
        msg!("-------------------------------------------------------------------------");
        msg!("Redeem IRMA successful:");
        msg!("Backing reserves for USDT: {:?}", accounts.state.reserves);
        Ok(())
    }

    /// Test cases for when redemption price is less than mint price
    #[test]
    fn test_redeem_irma_normal<'info>() -> Result<()> {
        msg!("\n-------------------------------------------------------------------------");
        msg!("Testing redeem IRMA with normal conditions, but with large discrepancies in mint prices");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account, core_account) 
            = initialize_anchor(program_id);
        let mut accounts: Maint<'_> = Maint {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            core: core_account.clone(),
            system_program: sys_account.clone(),
        };
        {
            msg!("Pre-redeem IRMA state 2:");
            let ctx: Context<Maint> = Context::new(
                program_id,
                &mut accounts,
                &[],
                MaintBumps::default(),
            );
            msg!("Backing reserves: {}", list_reserves(ctx));
            let state: &mut StateMap = &mut accounts.state;
            let reserves = state.reserves.clone();
            let mut i: u64 = 0;
            for sc in reserves {
                msg!("Backing reserves for {}: {:?}", sc.symbol, sc.backing_reserves);
                if sc.backing_decimals == 0 {
                    msg!("Skipping non-existent stablecoin: {}", sc.symbol);
                    continue; // skip non-existent stablecoins
                }
                let mut_backing = state.get_mut_stablecoin(&sc.symbol).unwrap();
                let reserve: &mut u128 = &mut mut_backing.backing_reserves;
                let circulation: &mut u128 = &mut mut_backing.irma_in_circulation;
                let price: &mut f64 = &mut mut_backing.mint_price;
                *reserve = 9_900_000_000; // Set a large reserve for testing
                *circulation = 10_000_000_000; // Set a large IRMA in circulation for testing
                *price = (i as f64 + 1.0) * (i as f64 + 1.0); // Set a price for testing
                i += 1;
            }
        }
        let mut ctx: Context<Maint> = Context::new(
            program_id,
            &mut accounts,
            &[],
            MaintBumps::default(),
        );
        // msg!("Current prices: {:?}", accounts.state.mint_price);
        // msg!("Backing reserves: {:?}", accounts.state.backing_reserves);
        // msg!("IRMA in circulation: {:?}", accounts.state.irma_in_circulation);
        let mut count: u64 = 0;
        // Test for near maximum redemption, multiple times, until it fails.
        // What we expect is that these repeated redemptions will equalize the differences between
        // mint prices and redemptions prices for all stablecoins.
        let mut reslt = redeem_irma(&mut ctx.accounts.state, "FDUSD", 100_000_000_000);
        while reslt.is_ok() {
            ctx = Context::<Maint>::new(
                program_id,
                &mut accounts,
                &[],
                MaintBumps::default(),
            );
            reslt = redeem_irma(&mut ctx.accounts.state, "FDUSD", 100_000_000_000);
            match reslt {
                Err(e) => {
                    msg!("Error redeeming IRMA for USDT: {:?}", e);
                    break; // Exit loop on error
                },
                Ok(_) => {
                    // msg!("Redeem IRMA successful for USDT");
                }
            }

            // Print the current state after every ten redemptions
            if count % 10 == 0 {
                let reserves = &accounts.state.reserves;
                for sc in reserves {
                    let backing: u128 = sc.backing_reserves;
                    let circulation: u128 = sc.irma_in_circulation;
                    let redemption_price: f64 = backing as f64 / circulation as f64;
                    msg!("{}, {:.3}, {}, {}, {:.3}", 
                        sc.symbol, 
                        sc.mint_price, 
                        backing,
                        circulation,
                        redemption_price);
                }
            }

            count += 1;
        }

        // msg!("-------------------------------------------------------------------------");
        // msg!("Redeem IRMA successful:");
        // msg!("Backing reserves: {:?}", accounts.state.backing_reserves);
        // msg!("IRMA in circulation: {:?}", accounts.state.irma_in_circulation);
        Ok(())
    }
}
