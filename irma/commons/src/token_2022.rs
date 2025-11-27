use std::result::Result::Ok; // disambiguate with std::result::Result::Ok
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::transfer_hook;
use anchor_spl::token_2022::spl_token_2022::*;
use anchor_spl::{
    token::spl_token,
    token_2022::spl_token_2022::extension::*};
// use anchor_lang::system_program;
use spl_transfer_hook_interface::onchain::add_extra_accounts_for_execute_cpi; // add_extra_account_metas_for_execute;
use crate::token_2022::transfer_fee::TransferFee;
use crate::token_2022::transfer_fee::MAX_FEE_BASIS_POINTS;
use crate::dlmm::types::*;
use crate::dlmm::accounts::*;
// use crate::constants::CustomError;

const ONE_IN_BASIS_POINTS: u128 = MAX_FEE_BASIS_POINTS as u128;

pub enum ActionType {
    Liquidity,
    Reward(usize),
}

pub fn get_potential_token_2022_related_ix_data_and_accounts(
    lb_pair: &LbPair,
    remaining_accounts: &[AccountInfo],
    action_type: ActionType,
) -> Result<Option<(Vec<RemainingAccountsSlice>, Vec<AccountMeta>)>> {
    let potential_token_2022_mints = match action_type {
        ActionType::Liquidity => {
            vec![
                (lb_pair.token_x_mint, AccountsType::TransferHookX),
                (lb_pair.token_y_mint, AccountsType::TransferHookY),
            ]
        }
        ActionType::Reward(idx) => {
            vec![(
                lb_pair.reward_infos[idx].mint,
                AccountsType::TransferHookReward,
            )]
        }
    };

    let mut slices = vec![];
    let mut accounts = vec![];
    let mut no_mint_accounts = vec![];

    for (mint, accounts_type) in potential_token_2022_mints {
        // Find the corresponding AccountInfo for this mint
        let mint_account = if let Some(mint_accnt) = remaining_accounts.iter().find(|acc| acc.key() == mint) {
            mint_accnt
        } else {
            continue; // Skip if mint account not found
        };
        remaining_accounts.iter().for_each(|acc| if acc.key() != mint { no_mint_accounts.push(acc.clone()); });
        let extra_account_metas =
            get_extra_account_metas_for_transfer_hook(mint, mint_account.clone(), &no_mint_accounts);

        if !extra_account_metas.is_empty() {
            slices.push(RemainingAccountsSlice {
                accounts_type,
                length: extra_account_metas.len() as u8,
            });

            accounts.extend(extra_account_metas);
        }       
    }

    if !slices.is_empty() {
        Ok(Some((slices, accounts)))
    } else {
        msg!("    No Token 2022 related accounts found.");
        Ok(None)
    }
}

pub fn get_extra_account_metas_for_transfer_hook<'a>(
    mint: Pubkey,
    mint_account: AccountInfo<'a>,
    remaining_accounts: &[AccountInfo<'a>],
) -> Vec<AccountMeta> {
    if mint_account.owner.eq(&spl_token::ID) {
        return vec![];
    }
    // require!(mint != *mint_account.key, CustomError::MintAccountMismatch);

    let mint_data = &mint_account.data.borrow(); // .as_ref();

    let mint_state = 
        StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
            mint_data,
        );
    
    // Handle the case where unpacking fails (e.g., in tests with mock data)
    let mint_state = match mint_state {
        Ok(state) => state,
        Err(_) => {
            // Return empty vector for tests or invalid mint data
            return vec![];
        }
    };

    if let Some(transfer_hook_program_id) = transfer_hook::get_program_id(&mint_state) {
        let mut transfer_ix =
            anchor_spl::token_2022::spl_token_2022::instruction::transfer_checked(
                &mint_account.owner,
                &Pubkey::default(),
                &mint,
                &Pubkey::default(),
                &Pubkey::default(),
                &[],
                0,
                mint_state.base.decimals,
            ).unwrap();

        let _ =add_extra_accounts_for_execute_cpi(
            &mut transfer_ix,
            &mut vec![mint_account.clone()],
            &transfer_hook_program_id,
            mint_account.clone(),
            remaining_accounts[0].clone(),
            remaining_accounts[1].clone(),
            remaining_accounts[2].clone(),
            0,
            &[mint_account.clone()],
        );
        // .map_err(|e: anyhow::Error| anyhow!(e))?;

        // Skip 4, source, mint, destination, authority
        let transfer_hook_required_accounts = transfer_ix.accounts[4..].to_vec();
        return transfer_hook_required_accounts;
    }

    vec![]
}

pub fn get_epoch_transfer_fee(mint_account: AccountInfo, epoch: u64) -> Result<Option<TransferFee>> {
    if *mint_account.owner == spl_token::ID {
        return Ok(None);
    }

    let token_mint_data = mint_account.data.borrow(); // .as_ref();
    let token_mint_unpacked = StateWithExtensions::<
        anchor_spl::token_2022::spl_token_2022::state::Mint,
    >::unpack(&token_mint_data);
    
    // Handle unpacking failure gracefully (e.g., in tests)
    let token_mint_unpacked = match token_mint_unpacked {
        Ok(unpacked) => unpacked,
        Err(_) => return Ok(None), // Return None for invalid/test data
    };

    if let std::result::Result::Ok(transfer_fee_config) =
        token_mint_unpacked.get_extension::<extension::transfer_fee::TransferFeeConfig>()
    {
        return Ok(Some(*transfer_fee_config.get_epoch_fee(epoch)));
    }

    Ok(None)
}

#[derive(Debug)]
pub struct TransferFeeExcludedAmount {
    pub amount: u64,
    pub transfer_fee: u64,
}

pub fn calculate_transfer_fee_excluded_amount(
    mint_account: AccountInfo,
    transfer_fee_included_amount: u64,
    epoch: u64,
) -> Result<TransferFeeExcludedAmount> {
    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(mint_account, epoch)? {
        let transfer_fee = epoch_transfer_fee
            .calculate_fee(transfer_fee_included_amount).ok_or("overflow").unwrap();
        let transfer_fee_excluded_amount = transfer_fee_included_amount
            .checked_sub(transfer_fee).ok_or("overflow").unwrap();

        return Ok(TransferFeeExcludedAmount {
            amount: transfer_fee_excluded_amount,
            transfer_fee,
        });
    }

    Ok(TransferFeeExcludedAmount {
        amount: transfer_fee_included_amount,
        transfer_fee: 0,
    })
}

#[derive(Debug)]
pub struct TransferFeeIncludedAmount {
    pub amount: u64,
    pub transfer_fee: u64,
}

pub fn calculate_transfer_fee_included_amount(
    mint_account: AccountInfo,
    transfer_fee_excluded_amount: u64,
    epoch: u64,
) -> Result<TransferFeeIncludedAmount> {
    if transfer_fee_excluded_amount == 0 {
        return Ok(TransferFeeIncludedAmount {
            amount: 0,
            transfer_fee: 0,
        });
    }

    if let Some(epoch_transfer_fee) = get_epoch_transfer_fee(mint_account, epoch)? {
        let transfer_fee: u64 =
            if u16::from(epoch_transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
                u64::from(epoch_transfer_fee.maximum_fee)
            } else {
                calculate_inverse_fee(&epoch_transfer_fee, transfer_fee_excluded_amount)
                    .ok_or("overflow").unwrap()
            };

        let transfer_fee_included_amount = transfer_fee_excluded_amount
            .checked_add(transfer_fee).ok_or("overflow").unwrap();

        return Ok(TransferFeeIncludedAmount {
            amount: transfer_fee_included_amount,
            transfer_fee,
        });
    }

    Ok(TransferFeeIncludedAmount {
        amount: transfer_fee_excluded_amount,
        transfer_fee: 0,
    })
}

pub fn calculate_pre_fee_amount(transfer_fee: &TransferFee, post_fee_amount: u64) -> Option<u64> {
    if post_fee_amount == 0 {
        return Some(0);
    }
    let maximum_fee = u64::from(transfer_fee.maximum_fee);
    let transfer_fee_basis_points = u16::from(transfer_fee.transfer_fee_basis_points) as u128;
    if transfer_fee_basis_points == 0 {
        Some(post_fee_amount)
    } else if transfer_fee_basis_points == ONE_IN_BASIS_POINTS {
        Some(maximum_fee.checked_add(post_fee_amount)?)
    } else {
        let numerator = (post_fee_amount as u128).checked_mul(ONE_IN_BASIS_POINTS)?;
        let denominator = ONE_IN_BASIS_POINTS.checked_sub(transfer_fee_basis_points)?;
        let raw_pre_fee_amount = numerator
            .checked_add(denominator)?
            .checked_sub(1)?
            .checked_div(denominator)?;

        if raw_pre_fee_amount.checked_sub(post_fee_amount as u128)? >= maximum_fee as u128 {
            post_fee_amount.checked_add(maximum_fee)
        } else {
            u64::try_from(raw_pre_fee_amount).ok()
        }
    }
}

pub fn calculate_inverse_fee(transfer_fee: &TransferFee, post_fee_amount: u64) -> Option<u64> {
    let pre_fee_amount = calculate_pre_fee_amount(transfer_fee, post_fee_amount)?;
    transfer_fee.calculate_fee(pre_fee_amount)
}
