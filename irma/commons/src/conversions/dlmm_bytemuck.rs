use anchor_lang::prelude::*;
use anchor_lang::prelude::Result;
use anchor_lang::error::Error;
use crate::dlmm::accounts::*;
use crate::constants::CustomError;
use std::collections::HashMap;

/// Fetch LbPair state dynamically when needed
pub fn fetch_lb_pair_state(acct_infos: &[AccountInfo], lb_pair: Pubkey) -> Result<LbPair> {
    get_bytemuck_account::<LbPair>(acct_infos, &lb_pair)
        .ok_or(error!(CustomError::MissingLbPairState))
}

/// Fetch bin arrays dynamically when needed
pub fn fetch_bin_arrays(acct_infos: &[AccountInfo], bin_array_keys: &[Pubkey]) -> Result<Vec<(Pubkey, BinArray)>> {
    let accounts: HashMap<Pubkey, Option<BinArray>> = 
        get_multiple_bytemuck_accounts(acct_infos, &bin_array_keys.to_vec())?;

    let mut bin_arrays = Vec::new();
    for key in bin_array_keys {
        if let Some(Some(bin_array)) = accounts.get(key) {
            bin_arrays.push((*key, *bin_array));
        }
    }
    Ok(bin_arrays)
}

// For executing DLMM instructions via CPI
// Derive bump if it does not exist:
// let (_pda, bump) = Pubkey::find_program_address(
//     &[b"irma", context.accounts.irma_admin.key().as_ref()],
//     &crate::ID, // Your program ID
// );
pub fn get_bytemuck_account<T: bytemuck::Pod>(
    acct_infos: &[AccountInfo],
    pubkey: &Pubkey
) -> Option<T> {
    let account_info = if let Some(acc) = acct_infos.iter().find(|acc| acc.key == pubkey) {
        acc
    } else {
        return None;
    };
    
    let data: T = bytemuck::pod_read_unaligned(&account_info.data.borrow()[8..]);
    Some(data)
}

pub fn get_multiple_bytemuck_accounts<T: bytemuck::Pod>(
    acct_infos: &[AccountInfo],
    pubkeys: &Vec<Pubkey>
) -> Result<HashMap<Pubkey, Option<T>>> {
    let mut data = HashMap::new();
    for pubkey in pubkeys.iter() {
        let account_info = acct_infos.iter()
            .find(|acc| acc.key == pubkey);
        if let Some(account_info) = account_info {
            let account_data: T = bytemuck::pod_read_unaligned(&account_info.data.borrow()[8..]);
            data.insert(*pubkey, Some(account_data));
        } else {
            msg!("Account not found for pubkey: {}", pubkey);
            data.insert(*pubkey, None);
        }
    }
    Ok(data)
}
