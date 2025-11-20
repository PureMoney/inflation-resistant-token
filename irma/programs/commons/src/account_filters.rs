use anchor_lang::prelude::*;
use crate::dlmm::accounts::*;

/// Check if a position account matches the given wallet and pair
/// For on-chain usage where we have direct access to account data
pub fn position_matches_wallet_and_pair(
    position_account: &AccountInfo,
    wallet: &Pubkey,
    pair: &Pubkey,
) -> Result<bool> {
    // Ensure account has enough data
    if position_account.data.borrow().len() < std::mem::size_of::<PositionV2>() + 8 {
        return Ok(false);
    }

    // Skip the 8-byte discriminator and read the position data
    let position_data = &position_account.data.borrow()[8..];
    
    // Read the position struct using bytemuck
    let position: PositionV2 = *bytemuck::try_from_bytes(
        &position_data[..std::mem::size_of::<PositionV2>()]
    ).map_err(|_| ProgramError::InvalidAccountData)?;

    // Check if both lb_pair and owner match
    Ok(position.lb_pair == *pair && position.owner == *wallet)
}

/// Filter a slice of position accounts by wallet and pair
/// Returns indices of accounts that match the criteria
pub fn filter_positions_by_wallet_and_pair(
    position_accounts: &[AccountInfo],
    wallet: &Pubkey,
    pair: &Pubkey,
) -> Result<Vec<usize>> {
    let mut matching_indices = Vec::new();
    
    for (index, account) in position_accounts.iter().enumerate() {
        if position_matches_wallet_and_pair(account, wallet, pair)? {
            matching_indices.push(index);
        }
    }
    
    Ok(matching_indices)
}

/// Extract position data from accounts that match the given criteria
/// Returns tuples of (pubkey, position_data) for matching accounts
pub fn get_matching_positions(
    position_accounts: &[AccountInfo],
    wallet: &Pubkey,
    pair: &Pubkey,
) -> Result<Vec<(Pubkey, PositionV2)>> {
    let mut matching_positions = Vec::new();
    
    for account in position_accounts.iter() {
        if position_matches_wallet_and_pair(account, wallet, pair)? {
            // Skip the 8-byte discriminator and read the position data
            let position_data = &account.data.borrow()[8..];
            let position: PositionV2 = *bytemuck::try_from_bytes(
                &position_data[..std::mem::size_of::<PositionV2>()]
            ).map_err(|_| ProgramError::InvalidAccountData)?;
            
            matching_positions.push((account.key(), position));
        }
    }
    
    Ok(matching_positions)
}

/// Check if an account contains a valid position
pub fn is_position_account(account: &AccountInfo, program_id: &Pubkey) -> bool {
    // Check owner
    if account.owner != program_id {
        return false;
    }
    
    // Check minimum size (8 bytes discriminator + PositionV2 size)
    let min_size = 8 + std::mem::size_of::<PositionV2>();
    if account.data.borrow().len() < min_size {
        return false;
    }
    
    // Could add discriminator check here if needed
    // let discriminator = &account.data.borrow()[0..8];
    // Check if discriminator matches Position account type
    
    true
}

/// Validate that an account contains a position owned by the expected wallet
pub fn validate_position_owner(
    position_account: &AccountInfo,
    expected_owner: &Pubkey,
) -> Result<bool> {
    if !is_position_account(position_account, &crate::dlmm::ID) {
        return Ok(false);
    }
    
    let position_data = &position_account.data.borrow()[8..];
    let position: PositionV2 = *bytemuck::try_from_bytes(
        &position_data[..std::mem::size_of::<PositionV2>()]
    ).map_err(|_| ProgramError::InvalidAccountData)?;
    
    Ok(position.owner == *expected_owner)
}
