use anchor_lang::prelude::*;
use anyhow::anyhow;

// pub struct Account<'info, T> {
//     pub key: Pubkey,
//     pub is_signer: bool,
//     pub is_writable: bool,
//     pub lamports: &'info mut u64,
//     pub data: &'info mut T,
//     pub owner: Pubkey,
//     info: AccountInfo<'info>,
//     account: T,
// }

/// Create Clock on-chain
pub fn create_clock(slot: u64, unix_timestamp: i64) -> Clock {
    Clock {
        slot,
        epoch_start_timestamp: unix_timestamp - 1000000,
        epoch: slot / 432000, // Approximate epoch calculation
        leader_schedule_epoch: slot / 432000,
        unix_timestamp,
    }
}


/// Utility to verify on-chain program logic without RPC calls
pub fn verify_program_state<T: anchor_lang::AccountDeserialize>(
    account_data: &[u8],
) -> anyhow::Result<T, anyhow::Error> {
    T::try_deserialize(&mut &account_data[8..]) // Skip discriminator
        .map_err(|e| anyhow!(format!("Failed to deserialize account: {}", e)))
}

/// Slot advancement for testing time-dependent logic
pub fn advance_slot(current_clock: &mut Clock, slots: u64) {
    current_clock.slot += slots;
    // Approximate unix timestamp advancement (400ms per slot)
    current_clock.unix_timestamp += (slots * 400) as i64 / 1000;
}

/// Create mint account data
pub fn create_mint_data(
    mint_authority: Option<Pubkey>,
    supply: u64,
    decimals: u8,
    is_token_2022: bool,
) -> Vec<u8> {
    let mut data = vec![0u8; if is_token_2022 { 165 } else { 82 }];
    
    // Basic mint data structure (simplified)
    // In a real implementation, you'd use the actual SPL token mint layout
    data[0] = 1; // Account type: Mint
    data[4..12].copy_from_slice(&supply.to_le_bytes());
    data[44] = decimals;
    
    if let Some(authority) = mint_authority {
        data[12..44].copy_from_slice(authority.as_ref());
    }
    
    data
}

/// Create token account data for testing
pub fn create_token_account_data(
    mint: Pubkey,
    owner: Pubkey,
    amount: u64,
) -> Vec<u8> {
    let mut data = vec![0u8; 165];
    
    // Basic token account data structure (simplified)
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    
    data
}

/// Create Token 2022 mint data with extensions  
pub fn create_token_2022_mint_data(
    mint_authority: Option<Pubkey>,
    supply: u64,
    decimals: u8,
    with_transfer_fee: bool,
    transfer_fee_basis_points: u16,
    max_fee: u64,
) -> Vec<u8> {
    // For testing purposes, create a basic structure
    // The actual Token 2022 parsing will be handled gracefully in the calling code
    let mut data = vec![0u8; if with_transfer_fee { 300 } else { 200 }];
    
    // Set some basic fields to make it look like mint data
    // This is simplified for testing - real Token 2022 structure is more complex
    data[0] = 1; // Indicates this is a mint account
    if let Some(authority) = mint_authority {
        data[4..36].copy_from_slice(authority.as_ref());
    }
    data[36..44].copy_from_slice(&supply.to_le_bytes());
    data[44] = decimals;
    data[45] = 1; // is_initialized
    
    // Add some transfer fee data if requested
    if with_transfer_fee {
        data[200..202].copy_from_slice(&transfer_fee_basis_points.to_le_bytes());
        data[202..210].copy_from_slice(&max_fee.to_le_bytes());
    }
    
    data
}

/// Create Token 2022 account data with extensions
pub fn create_token_2022_account_data(
    mint: Pubkey,
    owner: Pubkey,
    amount: u64,
    with_transfer_fee: bool,
) -> Vec<u8> {
    // Token 2022 accounts can have extensions
    let mut data = vec![0u8; if with_transfer_fee { 250 } else { 200 }];
    
    // Basic token account data structure
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    
    // Add extension data if needed
    if with_transfer_fee {
        // Simplified extension data for transfer fee accounts
        data[200] = 1; // Has transfer fee extension
    }
    
    data
}

/// Test on-chain instruction execution without blockchain
pub fn simulate_instruction_execution<T, F>(
    instruction_handler: F,
    accounts: &mut [AccountInfo],
    instruction_data: &[u8],
) -> Result<T>
where
    F: FnOnce(&mut [AccountInfo], &[u8]) -> Result<T>,
{
    instruction_handler(accounts, instruction_data)
}

/// Extract token balance from token account data
pub fn get_token_balance_from_data(token_account_data: &[u8]) -> anyhow::Result<u64, anyhow::Error> {
    if token_account_data.len() < 72 {
        return Err(anyhow!("Invalid token account data length"));
    }
    
    let amount_bytes = token_account_data[64..72]
        .try_into()
        .map_err(|_| anyhow!("Failed to extract amount bytes"))?;

    Ok(u64::from_le_bytes(amount_bytes))
}

/// Update token balance in token account data
pub fn set_token_balance_in_data(token_account_data: &mut [u8], new_balance: u64) -> anyhow::Result<()> {
    if token_account_data.len() < 72 {
        msg!("Invalid token account data length");
        return Ok(());
    }
    
    token_account_data[64..72].copy_from_slice(&new_balance.to_le_bytes());
    Ok(())
}

/// Create program-derived address for testing
pub fn create_program_derived_address(seeds: &[&[u8]], program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, program_id)
}

/// Simulate time passage for time-dependent on-chain logic
pub fn simulate_time_passage(clock: &mut Clock, seconds: i64) {
    clock.unix_timestamp += seconds;
    // Approximate slot advancement (400ms per slot)
    clock.slot += (seconds * 1000 / 400) as u64;
}

/// Rent calculation for determining rent-exempt requirements
pub fn calculate_rent_exemption(data_len: usize) -> u64 {
    // Simplified rent calculation for testing
    // In reality, this would use the actual rent calculation logic
    let base_rent = 1_000_000; // 0.001 SOL base
    let per_byte_rent = 6960; // Approximate lamports per byte
    base_rent + (data_len as u64 * per_byte_rent)
}

#[cfg(test)]
mod utils_tests {
    use super::*;
    
    #[test]
    fn test_on_chain_utilities() -> anyhow::Result<()> {
        // Test clock creation
        let clock = create_clock(100, 1700000000);
        assert_eq!(clock.slot, 100);
        assert_eq!(clock.unix_timestamp, 1700000000);
        
        // Test slot advancement
        let mut clock = create_clock(100, 1700000000);
        advance_slot(&mut clock, 10);
        assert_eq!(clock.slot, 110);
        
        // Test time passage simulation
        let mut clock = create_clock(100, 1700000000);
        simulate_time_passage(&mut clock, 60); // 1 minute
        assert_eq!(clock.unix_timestamp, 1700000060);
        
        // Test mint data creation
        let mint_data = create_mint_data(
            Some(Pubkey::new_unique()),
            1000000,
            6,
            false,
        );
        assert_eq!(mint_data.len(), 82);
        assert_eq!(mint_data[44], 6); // decimals
        
        // Test token account data creation and balance operations
        let mut token_data = create_token_account_data(
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            500000,
        );
        assert_eq!(token_data.len(), 165);
        
        // Test balance extraction
        let balance = get_token_balance_from_data(&token_data).unwrap();
        assert_eq!(balance, 500000);
        
        // Test balance update
        let _nothing = set_token_balance_in_data(&mut token_data, 750000);
        let new_balance = get_token_balance_from_data(&token_data).unwrap();
        assert_eq!(new_balance, 750000);
        
        // Test PDA creation
        let program_id = Pubkey::new_unique();
        let (_pda, _bump) = create_program_derived_address(&[b"test", b"seed"], &program_id);
        // assert!(bump <= 255u8);
        
        // Test rent calculation
        let rent = calculate_rent_exemption(165);
        assert!(rent > 0);
        
        println!("On-chain utilities test completed successfully");
        Ok(())
    }
    
    #[test]
    fn test_account_info_creation() {
        let key = Pubkey::new_unique();
        let mut lamports = 1000000u64;
        let mut data = vec![0u8; 100];
        let owner = Pubkey::new_unique();

        let account = Account {
            key,
            is_signer: true,
            is_writable: true,
            lamports: &mut lamports,
            data: &mut data,
            owner,
        };
        
        let account_info = account.to_account_info();
        
        assert_eq!(*account_info.key, key);
        assert_eq!(account_info.is_signer, true);
        assert_eq!(account_info.is_writable, true);
        assert_eq!(*account_info.owner, owner);
        assert_eq!(account_info.data_len(), 100);
    }
}