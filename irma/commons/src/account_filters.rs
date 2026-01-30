use anchor_lang::prelude::*;
use crate::dlmm::accounts::*;
use crate::conversions::dlmm_bytemuck::get_bytemuck_account_ref;

/// Get Account Info
pub fn get_account_info<'a>(
    accounts: &'a [AccountInfo<'a>],
    key: &Pubkey,
) -> Result<AccountInfo<'a>> {
    accounts.iter().find(|acc| acc.key == key).ok_or_else(|| {
        ProgramError::NotEnoughAccountKeys.into()
    }).cloned()
}

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
        // Removed memory-heavy logging
        return Ok(false);
    }

    // Skip the 8-byte discriminator and read the position data
    let data_borrow = position_account.data.borrow();
    let position_data = &data_borrow[8..];
    
    // Ensure we have exactly the right amount of data for PositionV2
    let position_size = std::mem::size_of::<PositionV2>();
    if position_data.len() < position_size {
        // Removed memory-heavy logging
        return Ok(false);
    }
    
    // Safely read the position struct using bytemuck with zerocopy
    let position = match get_bytemuck_account_ref::<PositionV2>(position_account) {
        Some(pos) => pos,
        None => {
            // Removed memory-heavy logging
            return Ok(false);
        }
    };

    // Removed memory-heavy debug logging
    
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
pub fn get_matching_positions<'a>(
    position_accounts: &'a [AccountInfo<'a>],
    wallet: &Pubkey,
    pair: &Pubkey,
) -> anyhow::Result<Vec<(&'a Pubkey, &'a PositionV2)>> {
    // Pre-allocate with estimated capacity to avoid reallocations
    let mut matching_positions = Vec::with_capacity(3); // Most cases will have 0-2 positions
    
    for account in position_accounts.iter() {
        if account.data.borrow().is_empty() {
            // Remove memory-heavy logging
            continue;
        }
        let discriminator = &account.data.borrow()[0..8];
        // msg!("Discriminator {:?}", discriminator);
        // Check if discriminator matches Position account type
        // Note: can't figure out how to reference the discriminator constant directly from the IDL,
        // so using the raw bytes for now (very bad kludge)
        // if discriminator != PositionV2::discriminator {
        if discriminator == [117, 176, 212, 199, 245, 180, 133, 182] {
            msg!("Account {} is a Position account", account.key);
            let data_slice = &account.data.borrow()[8..];
            let position = unsafe {
                // Ensure alignment and create reference
                if data_slice.as_ptr() as usize % std::mem::align_of::<PositionV2>() == 0 {
                    Some(&*(data_slice.as_ptr() as *const PositionV2))
                } else {
                    // If not properly aligned, we can't safely create a reference
                    msg!("-");
                    None
                }
            };
            let position = match position {
                Some(pos) => pos,
                None => {
                    msg!("-");
                    continue;
                }
            };
            if position.owner == *wallet && position.lb_pair == *pair {
                // The position_matches_wallet_and_pair already validated the data,
                // so we can safely read it here
                matching_positions.push((account.key, position));
                
                // Early exit if we've found the maximum expected positions
                if matching_positions.len() >= 3 {
                    break;
                }
            } else if position.owner != *wallet {
                msg!(
                    "Position account {} owner mismatch: expected {}, found {}",
                    account.key,
                    wallet,
                    position.owner
                );
            } else if position.lb_pair != *pair {
                msg!(
                    "Position account {} pair mismatch: expected {}, found {}",
                    account.key,
                    pair,
                    position.lb_pair
                );
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
    
    // Discriminator check
    let discriminator = &account.data.borrow()[0..8];
    // Remove memory-heavy discriminator logging
    // Check if discriminator matches Position account type
    // Note: can't figure out how to reference the discriminator constant directly from the IDL,
    // so using the raw bytes for now (very bad kludge)
    // if discriminator != PositionV2::discriminator {
    if discriminator != [117, 176, 212, 199, 245, 180, 133, 182] {
        return false;
    }

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
