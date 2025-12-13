use anchor_lang::prelude::*;
use anchor_lang::prelude::Result;
use anchor_lang::error::Error;
use crate::dlmm::accounts::*;
use crate::constants::CustomError;
use std::collections::HashMap;

/// Fetch LbPair state dynamically when needed
pub fn fetch_lb_pair_state<'a>(
    acct_infos: &'a [AccountInfo<'a>], lb_pair: &Pubkey
) -> Result<&'a LbPair> {
    let account_info = acct_infos.iter()
        .find(|acc| acc.key == lb_pair)
        .ok_or(error!(CustomError::MissingLbPairState))?;
    get_bytemuck_account_ref::<LbPair>(account_info)
        .ok_or(error!(CustomError::MissingLbPairState))
}

/// Fetch bin arrays dynamically when needed
pub fn fetch_bin_arrays<'a>(
    acct_infos: &'a [AccountInfo<'a>], bin_array_keys: &[Pubkey]
) -> Result<Vec<(Pubkey, &'a BinArray)>> {
    let accounts: HashMap<Pubkey, Option<&BinArray>> = 
        get_multiple_bytemuck_account_refs(acct_infos, bin_array_keys)?;

    let mut bin_arrays = Vec::new();
    for key in bin_array_keys {
        if let Some(Some(bin_array)) = accounts.get(key) {
            bin_arrays.push((*key, *bin_array));
        }
    }
    Ok(bin_arrays)
}

/// Fetch positions dynamically when needed (zero-copy)
pub fn fetch_positions<'a>(
    acct_infos: &'a [AccountInfo<'a>], position_pks: &[Pubkey]
) -> Result<Vec<&'a PositionV2>> {
    let accounts: HashMap<Pubkey, Option<&PositionV2>> = 
        get_multiple_bytemuck_account_refs(acct_infos, position_pks)?;
        
    let mut positions = Vec::new();
    for pk in position_pks {
        if let Some(Some(position)) = accounts.get(pk) {
            positions.push(*position);
        }
    }
    Ok(positions)
}

// Backup copying version for when unsafe zero-copy isn't suitable
pub fn get_bytemuck_account<T: bytemuck::Pod>(
    acct_infos: &[AccountInfo],
    pubkey: &Pubkey
) -> Option<T> {
    let account_info = if let Some(acc) = acct_infos.iter().find(|acc| acc.key == pubkey) {
        acc
    } else {
        return None;
    };
    
    let borrowed_data = account_info.data.borrow();
    if borrowed_data.len() < 8 + std::mem::size_of::<T>() {
        return None;
    }
    
    let data_slice = &borrowed_data[8..];
    match bytemuck::try_from_bytes::<T>(&data_slice[..std::mem::size_of::<T>()]) {
        Ok(data_ref) => Some(*data_ref), // Copy the data
        Err(_) => None,
    }
}

pub fn get_multiple_bytemuck_accounts<T: bytemuck::Pod>(
    acct_infos: &[AccountInfo],
    pubkeys: &Vec<Pubkey>
) -> Result<HashMap<Pubkey, Option<T>>> {
    let mut data = HashMap::new();
    for pubkey in pubkeys.iter() {
        if let Some(account_data) = get_bytemuck_account::<T>(acct_infos, pubkey) {
            data.insert(*pubkey, Some(account_data));
        } else {
            data.insert(*pubkey, None);
        }
    }
    Ok(data)
}


/// Zero-copy version that returns a reference to the account data
/// SAFETY: This uses unsafe code to avoid RefCell borrowing issues
/// and enable true zero-copy access to account data
pub fn get_bytemuck_account_ref<'a, T: bytemuck::Pod>(
    account_info: &'a AccountInfo,
) -> Option<&'a T> {
    // Check if account has enough data
    let data_len = account_info.data.borrow().len();
    if data_len < 8 + std::mem::size_of::<T>() {
        return None;
    }
    
    // SAFETY: We need to use unsafe to get a stable reference to the account data
    // This bypasses RefCell's runtime borrowing and gives us direct access
    unsafe {
        let data_ptr = account_info.data.as_ptr();
        let data_slice = std::slice::from_raw_parts(data_ptr, data_len);
        let account_data_slice = &data_slice[8..8 + std::mem::size_of::<T>()];
        
        // Ensure alignment and create reference
        if account_data_slice.as_ptr() as usize % std::mem::align_of::<T>() == 0 {
            Some(&*(account_data_slice.as_ptr() as *const T))
        } else {
            // If not properly aligned, we can't safely create a reference
            None
        }
    }
}

pub fn get_multiple_bytemuck_account_refs<'a, T: bytemuck::Pod>(
    acct_infos: &'a [AccountInfo<'a>],
    pubkeys: &[Pubkey]  // Remove lifetime annotation here
) -> Result<HashMap<Pubkey, Option<&'a T>>> {
    let mut data = HashMap::new();
    for pubkey in pubkeys.iter() {
        let account_info = acct_infos.iter()
            .find(|acc| acc.key == pubkey);
        if let Some(account_info) = account_info {
            if let Some(data_ref) = get_bytemuck_account_ref::<T>(account_info) {
                data.insert(*pubkey, Some(data_ref));
            } else {
                data.insert(*pubkey, None);
            }
        } else {
            msg!("Account not found for pubkey: {}", pubkey);
            data.insert(*pubkey, None);
        }
    }
    Ok(data)
}
