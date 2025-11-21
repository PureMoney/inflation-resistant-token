
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
    use irma::pricing::CustomError;
    use irma::pricing::{StateMap, StableState};
    use irma::pricing::{init_pricing, set_mint_price, mint_irma, redeem_irma, list_reserves};
    use irma::pricing::MAX_BACKING_COUNT;
    use irma::{Init, Common, Maint, InitBumps, CommonBumps, MaintBumps};

    
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
        mut_reserve.irma_in_circulation += (amount as f64 / price).ceil() as u64;
        assert_eq!(state.get_stablecoin(quote_token).unwrap().backing_reserves, 
            prev_reserve + amount);
        assert_eq!(state.get_stablecoin(quote_token).unwrap().irma_in_circulation, 
            prev_circulation + (amount as f64 / price).ceil() as u64);
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

    fn prep_accounts<'info>(owner: &'info Pubkey, state_account: Pubkey) -> (AccountInfo<'info>, AccountInfo<'info>, AccountInfo<'info>) {
        // Create a buffer for StateMap and wrap it in AccountInfo
        let lamports: &mut u64 = Box::leak(Box::new(100000u64));
        let mut state: StateMap = allocate_state();
        let _ = state.init_reserves(); // Add initial stablecoins to the state

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
        (state_account_info, signer_account_info, sys_account_info)
    }

    fn initialize_anchor<'info>(program_id: &'info Pubkey) -> (Account<'info, StateMap>, Signer<'info>, Program<'info, anchor_lang::system_program::System>) {
        //                 state_account_info: &'info AccountInfo<'info>) {
        //                 sys_account_info: &AccountInfo<'info>) {
        // let program_id: &'info Pubkey = Box::leak(Box::new(Pubkey::new_from_array(irma::ID.to_bytes())));
        let state_account: Pubkey = Pubkey::find_program_address(&[b"state".as_ref()], program_id).0;
        let (state_account_info, irma_admin_account_info, sys_account_info) 
                 = prep_accounts(program_id, state_account);
        // Bind to variables to extend their lifetime
        let state_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(state_account_info));
        let irma_admin_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(irma_admin_account_info));
        let sys_account_static: &'info AccountInfo<'info> = Box::leak(Box::new(sys_account_info));
        let mut accounts: Init<'_> = Init {
            state: Account::try_from(state_account_static).unwrap(),
            irma_admin: Signer::try_from(irma_admin_account_static).unwrap(),
            system_program: Program::try_from(sys_account_static).unwrap(),
        };
        let ctx: Context<Init> = Context::new(
            program_id,
            &mut accounts,
            &[],
            InitBumps::default(), // Use default bumps if not needed
        );
        let result: std::result::Result<(), Error> = init_pricing(ctx);
        assert!(result.is_ok());
        // msg!("StateMap account: {:?}", accounts.state);
        return (accounts.state, accounts.irma_admin, accounts.system_program);
    }

    #[test]
    fn test_initialize_anchor<'info>() {
        msg!("-------------------------------------------------------------------------");
        msg!("Testing init_pricing IRMA with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Init<'_> = Init {
            state: state_account.clone(),
            irma_admin: irma_admin_account.clone(),
            system_program: sys_account.clone(),
        };
        let ctx: Context<Init> = Context::new(
            program_id,
            &mut accounts,
            &[],
            InitBumps::default(), // Use default bumps if not needed
        );
        let result: std::result::Result<(), Error> = init_pricing(ctx);
        assert!(result.is_ok());
        msg!("StateMap account initialized successfully: {:?}", accounts.state);
   }

    #[test]
    fn test_set_mint_price_anchor<'info>() {
        msg!("-------------------------------------------------------------------------");
        msg!("Testing set IRMA mint price with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Common<'_> = Common {
            state: state_account.clone(),
            trader: irma_admin_account.clone(),
            system_program: sys_account.clone(),
        };
        let mut ctx: Context<Common> = Context::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        let mut result: std::result::Result<(), Error> = set_mint_price(ctx, "USDT", 1.5);
        assert!(result.is_ok());
        // Re-create ctx for the next call if needed
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = set_mint_price(ctx, "USDC", 1.8);
        assert!(result.is_ok());
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = set_mint_price(ctx, "FDUSD", 1.3);
        assert!(result.is_ok());
        // msg!("Mint price for USDT set successfully: {:?}", accounts.state.mint_price["USDT" as usize]);
        // msg!("Mint price for USDC set successfully: {:?}", accounts.state.mint_price[Stablecoins::USDC as usize]);
        // msg!("Mint price for USDE set successfully: {:?}", accounts.state.mint_price[Stablecoins::FDUSD as usize]);
    }

    #[test]
    fn test_mint_irma_anchor<'info>() -> Result<()> {
        msg!("-------------------------------------------------------------------------");
        msg!("Testing mint IRMA with normal conditions");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        // let state_account: Pubkey = Pubkey::find_program_address(&[b"state".as_ref()], program_id).0;
        let (state_account, irma_admin_account, sys_account) 
                = initialize_anchor(program_id);
        // Bind to variables to extend their lifetime
        let mut accounts: Common<'_> = Common {
            state: state_account.clone(),
            trader: irma_admin_account.clone(),
            system_program: sys_account.clone(),
        };
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
        let mut ctx: Context<Common> = Context::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        let mut result = mint_irma(ctx, "USDT", 100);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for USDT");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = mint_irma(ctx, "PYUSD", 1000);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for PYUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for PYUSD");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = mint_irma(ctx, "USDG", 10000);
        match result {
            Err(e) => {
                msg!("Error minting IRMA for USDG: {:?}", e);
            },
            Ok(_) => {
                msg!("Mint IRMA successful for USDG");
            }
        }
        msg!("-------------------------------------------------------------------------");
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
        msg!("-------------------------------------------------------------------------");
        msg!("Testing redeem IRMA when mint price is less than redemption price");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account) 
            = initialize_anchor(program_id);
        let mut accounts: Common<'_> = Common {
            state: state_account.clone(),
            trader: irma_admin_account.clone(),
            system_program: sys_account.clone(),
        };
        {
            let ctx: Context<Common> = Context::new(
                program_id,
                &mut accounts,
                &[],
                CommonBumps::default(),
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
            let reserve: &mut u64 = &mut mut_backing.backing_reserves;
            let circulation: &mut u64 = &mut mut_backing.irma_in_circulation;
            *reserve = 1000000; // Set a large reserve for testing
            *circulation = 100000; // Set a large IRMA in circulation for testing
        }
        // msg!("Current prices: {:?}", accounts.state.mint_price);
        // msg!("Backing reserves: {:?}", accounts.state.backing_reserves);
        // msg!("IRMA in circulation: {:?}", accounts.state.irma_in_circulation);
        let mut ctx: Context<Common> = Context::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        let mut result: std::result::Result<(), Error> = redeem_irma(ctx, "USDC", 10);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDC: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDC");
            }
        }
        // assert!(result.is_ok(), "Redeem IRMA failed for USDC");
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = redeem_irma(ctx, "USDT", 20);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDT");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = redeem_irma(ctx, "PYUSD", 30);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for PYUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for PYUSD");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = redeem_irma(ctx, "USDG", 40);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDG: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDG");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = redeem_irma(ctx, "FDUSD", 50);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for FDUSD: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for FDUSD");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );

        msg!("Mid-state for USDT before further redemption: {:?}", 
            state_account.get_stablecoin("USDT").unwrap().backing_reserves);
        // Test for near maximum redemption
        result = redeem_irma(ctx, "USDT", 10_000);
        match result {
            Err(e) => {
                msg!("Error redeeming IRMA for USDT: {:?}", e);
            },
            Ok(_) => {
                msg!("Redeem IRMA successful for USDT");
            }
        }
        ctx = Context::<Common>::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        result = redeem_irma(ctx, "USDS", 10);
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
        msg!("-------------------------------------------------------------------------");
        msg!("Testing redeem IRMA with normal conditions, but with large discrepancies in mint prices");  
        msg!("-------------------------------------------------------------------------");
        let program_id: &'info Pubkey = &IRMA_ID;
        let (state_account, irma_admin_account, sys_account) 
            = initialize_anchor(program_id);
        let mut accounts: Common<'_> = Common {
            state: state_account.clone(),
            trader: irma_admin_account.clone(),
            system_program: sys_account.clone(),
        };
        {
            msg!("Pre-redeem IRMA state 2:");
            let ctx: Context<Common> = Context::new(
                program_id,
                &mut accounts,
                &[],
                CommonBumps::default(),
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
                let reserve: &mut u64 = &mut mut_backing.backing_reserves;
                let circulation: &mut u64 = &mut mut_backing.irma_in_circulation;
                let price: &mut f64 = &mut mut_backing.mint_price;
                *reserve = 9_900_000_000; // Set a large reserve for testing
                *circulation = 10_000_000_000; // Set a large IRMA in circulation for testing
                *price = (i as f64 + 1.0) * (i as f64 + 1.0); // Set a price for testing
                i += 1;
            }
        }
        let mut ctx: Context<Common> = Context::new(
            program_id,
            &mut accounts,
            &[],
            CommonBumps::default(),
        );
        // msg!("Current prices: {:?}", accounts.state.mint_price);
        // msg!("Backing reserves: {:?}", accounts.state.backing_reserves);
        // msg!("IRMA in circulation: {:?}", accounts.state.irma_in_circulation);
        let mut count: u64 = 0;
        // Test for near maximum redemption, multiple times, until it fails.
        // What we expect is that these repeated redemptions will equalize the differences between
        // mint prices and redemptions prices for all stablecoins.
        let mut reslt = redeem_irma(ctx, "FDUSD", 100_000_000_000);
        while reslt.is_ok() {
            ctx = Context::<Common>::new(
                program_id,
                &mut accounts,
                &[],
                CommonBumps::default(),
            );
            reslt = redeem_irma(ctx, "FDUSD", 100_000_000_000);
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
                    let backing: u64 = sc.backing_reserves;
                    let circulation: u64 = sc.irma_in_circulation;
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
