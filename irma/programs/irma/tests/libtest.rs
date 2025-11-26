#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anchor_lang::prelude::*;
    use anchor_lang::prelude::Pubkey;
    // use anchor_lang::prelude::Clock;
    // use anchor_lang::prelude::Sysvar;
    use anchor_lang::prelude::Signer;
    // use anchor_lang::prelude::Account;
    use anchor_lang::prelude::Program;
    use anchor_lang::context::Context;
    // use anchor_lang::solana_program::sysvar::clock::ID as CLOCK_ID;
    use anchor_lang::system_program;
    // use anchor_lang::Accounts;

    use irma::irma as money;
    use irma::pricing::{StateMap, StableState};
    use irma::IRMA_ID;
    use irma::pricing::MAX_BACKING_COUNT;
    use irma::pricing::{init_pricing, set_mint_price, mint_irma, redeem_irma, list_reserves};
    use irma::{Init, Maint, InitBumps, MaintBumps};
    // use irma::State;



    #[test]
    fn test_crank<'info>() -> Result<()> {
        let program_id: &Pubkey = &IRMA_ID;
        msg!("Starting crank test...");

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

        fn allocate_state() -> StateMap {
            let mut state: StateMap = StateMap::new();
            state.init_reserves().unwrap(); // Initialize reserves
            state
        }
        msg!("Starting crank test...");
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
        let crank_result: std::result::Result<(), Error> = money::initialize(ctx);
        assert!(crank_result.is_ok());
        msg!("Crank executed successfully");

        msg!("Crank market completed successfully.");
        Ok(())
    }
}
