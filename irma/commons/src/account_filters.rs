use anchor_lang::prelude::*;
use std::collections::HashMap;
use crate::anyhow;
use crate::dlmm::accounts::*;
use crate::conversions::*;

/// Check if a position account matches the given wallet and pair
/// For on-chain usage where we have direct access to account data
pub fn position_matches_wallet_and_pair(
    position_account: &AccountInfo,
    wallet: &Pubkey,
    pair: &Pubkey,
) -> Result<bool> {
    // Ensure account has enough data
    let required_size = std::mem::size_of::<PositionV2>() + 8;
    if position_account.data.borrow().len() < required_size {
        msg!("Account {} has insufficient data: {} bytes, need {}", 
             position_account.key(), position_account.data.borrow().len(), required_size);
        return Ok(false);
    }

    // Skip the 8-byte discriminator and read the position data
    let data_borrow = position_account.data.borrow();
    let position_data = &data_borrow[8..];
    
    // Ensure we have exactly the right amount of data for PositionV2
    let position_size = std::mem::size_of::<PositionV2>();
    if position_data.len() < position_size {
        msg!("Position data too small: {} bytes, need {}", position_data.len(), position_size);
        return Ok(false);
    }
    
    // Safely read the position struct using bytemuck with bounds checking
    let position_bytes = &position_data[..position_size];
    let position = match bytemuck::try_from_bytes::<PositionV2>(position_bytes) {
        Ok(pos) => *pos,
        Err(e) => {
            msg!("Failed to parse position data: {:?}", e);
            return Ok(false);
        }
    };

    msg!("    Checking position: lb_pair {:?}, owner {:?}", position.lb_pair, position.owner);
    msg!("    Against pair {:?}, wallet {:?}", pair, wallet);

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

/// Fetch positions dynamically when needed
pub fn fetch_positions(acct_infos: &[AccountInfo], position_pks: &[Pubkey]) -> Result<Vec<PositionV2>> {
    let accounts: HashMap<Pubkey, Option<PositionV2>> = 
        get_multiple_bytemuck_accounts(acct_infos, &position_pks.to_vec())?;
        
    let mut positions = Vec::new();
    for pk in position_pks {
        if let Some(Some(position)) = accounts.get(pk) {
            positions.push(*position);
        }
    }
    Ok(positions)
}

/// Extract position data from accounts that match the given criteria
/// Returns tuples of (pubkey, position_data) for matching accounts
pub fn get_matching_positions(
    position_accounts: &[AccountInfo],
    wallet: &Pubkey,
    pair: &Pubkey,
) -> anyhow::Result<Vec<(Pubkey, PositionV2)>> {
    let mut matching_positions = Vec::new();
    
    for account in position_accounts.iter() {
        if account.data.borrow().is_empty() {
            return Err(anyhow!("Invalid position {} found in input list", account.key()))?;
        }
        let discriminator = &account.data.borrow()[0..8];
        msg!("Discriminator: {:?}", discriminator);
        if position_matches_wallet_and_pair(account, wallet, pair)? {
            // The position_matches_wallet_and_pair already validated the data,
            // so we can safely read it here
            let data_borrow = account.data.borrow();
            let position_data = &data_borrow[8..];
            let position_size = std::mem::size_of::<PositionV2>();
            let position_bytes = &position_data[..position_size];
            
            match bytemuck::try_from_bytes::<PositionV2>(position_bytes) {
                Ok(position) => {
                    matching_positions.push((account.key(), *position));
                }
                Err(e) => {
                    msg!("Failed to parse position data for {}: {:?}", account.key(), e);
                    // Skip this position instead of failing completely
                    continue;
                }
            }
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
