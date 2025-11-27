use crate::calculate_transfer_fee_excluded_amount;
use crate::calculate_transfer_fee_included_amount;
use crate::derive_bin_array_pda;
// use crate::ensure;
use crate::extensions::bin::BinExtension;
use crate::extensions::bin_array::BinArrayExtension;
use crate::extensions::bin_array_bitmap::BinArrayBitmapExtExtension;
use crate::dlmm::accounts::*;
use crate::dlmm::types::*;
use crate::extensions::lb_pair::LbPairExtension;
use crate::SwapResult;
use crate::CustomError;

use anchor_lang::prelude::*;
use anchor_lang::require;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SwapExactInQuote {
    pub amount_out: u64,
    pub fee: u64,
}

#[derive(Debug)]
pub struct SwapExactOutQuote {
    pub amount_in: u64,
    pub fee: u64,
}

fn validate_swap_activation(
    lb_pair: &LbPair,
    current_timestamp: u64,
    current_slot: u64,
) -> Result<()> {
    require!(
        lb_pair.status()?.eq(&PairStatus::Enabled),
        CustomError::PairDisabled
    );

    let pair_type = lb_pair.pair_type()?;
    if pair_type.eq(&PairType::Permission) {
        let activation_type = lb_pair.activation_type()?;
        let current_point = match activation_type {
            ActivationType::Slot => current_slot,
            ActivationType::Timestamp => current_timestamp,
        };

        require!(
            current_point >= lb_pair.activation_point,
            CustomError::PairDisabled
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn quote_exact_out<'a>(
    lb_pair_pubkey: Pubkey,
    lb_pair: &LbPair,
    mut amount_out: u64,
    swap_for_y: bool,
    bin_arrays: HashMap<Pubkey, BinArray>,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    clock: &Clock,
    mint_x_account: AccountInfo<'a>,
    mint_y_account: AccountInfo<'a>,
) -> Result<SwapExactOutQuote> {
    let current_timestamp = clock.unix_timestamp as u64;
    let current_slot = clock.slot;
    let epoch = clock.epoch;

    validate_swap_activation(lb_pair, current_timestamp, current_slot)?;

    let mut lb_pair = *lb_pair;
    lb_pair.update_references(current_timestamp as i64)?;

    let mut total_amount_in: u64 = 0;
    let mut total_fee: u64 = 0;

    let (in_mint_account, out_mint_account) = if swap_for_y {
        (mint_x_account, mint_y_account)
    } else {
        (mint_y_account, mint_x_account)
    };

    amount_out =
        calculate_transfer_fee_included_amount(out_mint_account, amount_out, epoch)?.amount;
    
    // safeguard against infinite loop
    let mut iterations = 0;
    const MAX_ITERATIONS: u64 = 70 * 512;

    while amount_out > 0 {
        let active_bin_array_pubkey = get_bin_array_pubkeys_for_swap(
            lb_pair_pubkey,
            &lb_pair,
            bitmap_extension,
            swap_for_y,
            1,
        )?
        .pop()
        .ok_or("Pool out of liquidity").unwrap();

        msg!("Quoting exact out, active bin array pubkey: {}", active_bin_array_pubkey);

        let mut active_bin_array = bin_arrays
            .get(&active_bin_array_pubkey)
            .cloned()
            .ok_or("Active bin array not found").unwrap();

        loop {

            if !active_bin_array.is_bin_id_within_range(lb_pair.active_id)? {
                msg!("Active bin id {} not within bin array index {}", lb_pair.active_id, active_bin_array.index);
                lb_pair.advance_active_bin(swap_for_y)?;
                break;
            } else if amount_out == 0 {
                break;
            }
            msg!("Active bin id {} is in bin array index {}", lb_pair.active_id, active_bin_array.index);

            lb_pair.update_volatility_accumulator()?;

            let active_bin = active_bin_array.get_bin_mut(lb_pair.active_id)?;
            let price = active_bin.get_or_store_bin_price(lb_pair.active_id, lb_pair.bin_step)?;

            msg!("--> result: active_id {}, price {}", lb_pair.active_id, price);

            if !active_bin.is_empty(!swap_for_y) {
                let bin_max_amount_out = active_bin.get_max_amount_out(swap_for_y);
                if amount_out >= bin_max_amount_out {
                    let max_amount_in = active_bin.get_max_amount_in(price, swap_for_y)?;
                    let max_fee = lb_pair.compute_fee(max_amount_in)?;

                    total_amount_in = total_amount_in
                        .checked_add(max_amount_in)
                        .ok_or("MathOverflow").unwrap();

                    total_fee = total_fee.checked_add(max_fee).ok_or("MathOverflow").unwrap();

                    amount_out = amount_out
                        .checked_sub(bin_max_amount_out)
                        .ok_or("MathOverflow").unwrap();
                } else {
                    let amount_in = Bin::get_amount_in(amount_out, price, swap_for_y)?;
                    let fee = lb_pair.compute_fee(amount_in)?;

                    total_amount_in = total_amount_in
                        .checked_add(amount_in)
                        .ok_or("MathOverflow").unwrap();

                    total_fee = total_fee.checked_add(fee).ok_or("MathOverflow").unwrap();

                    amount_out = 0;
                }
            }

            iterations += 1;
            require!(iterations < MAX_ITERATIONS, CustomError::ExceededMaxIterationsQuoteExactOut);

            if amount_out > 0 {
                lb_pair.advance_active_bin(swap_for_y)?;
            }
            else {
                break;
            }
        }
    }

    total_amount_in = total_amount_in
        .checked_add(total_fee)
        .ok_or("MathOverflow").unwrap();

    total_amount_in =
        calculate_transfer_fee_included_amount(in_mint_account, total_amount_in, epoch)?.amount;

    Ok(SwapExactOutQuote {
        amount_in: total_amount_in,
        fee: total_fee,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in<'a>(
    lb_pair_pubkey: Pubkey,
    lb_pair: &LbPair,
    amount_in: u64,
    swap_for_y: bool,
    bin_arrays: HashMap<Pubkey, BinArray>,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    clock: &Clock,
    mint_x_account: AccountInfo<'a>,
    mint_y_account: AccountInfo<'a>,
) -> Result<SwapExactInQuote> {
    let current_timestamp = clock.unix_timestamp as u64;
    let current_slot = clock.slot;
    let epoch = clock.epoch;

    validate_swap_activation(lb_pair, current_timestamp, current_slot)?;

    let mut lb_pair = *lb_pair;
    lb_pair.update_references(current_timestamp as i64)?;

    let mut total_amount_out: u64 = 0;
    let mut total_fee: u64 = 0;

    let (in_mint_account, out_mint_account) = if swap_for_y {
        (mint_x_account, mint_y_account)
    } else {
        (mint_y_account, mint_x_account)
    };

    let transfer_fee_excluded_amount_in =
        calculate_transfer_fee_excluded_amount(in_mint_account, amount_in, epoch)?.amount;

    let mut amount_left = transfer_fee_excluded_amount_in;
    
    // safeguard against infinite loop
    let mut iterations = 0;
    const MAX_ITERATIONS: u64 = 70 * 512;

    while amount_left > 0 {
        let active_bin_array_pubkey = get_bin_array_pubkeys_for_swap(
            lb_pair_pubkey,
            &lb_pair,
            bitmap_extension,
            swap_for_y,
            1,
        )?
        .pop()
        .ok_or("Pool out of liquidity").unwrap();

        let mut active_bin_array = bin_arrays
            .get(&active_bin_array_pubkey)
            .cloned()
            .ok_or("Active bin array not found").unwrap();

        loop {
            if !active_bin_array.is_bin_id_within_range(lb_pair.active_id)? || amount_left == 0 {
                break;
            }

            lb_pair.update_volatility_accumulator()?;

            let active_bin = active_bin_array.get_bin_mut(lb_pair.active_id)?;
            let price = active_bin.get_or_store_bin_price(lb_pair.active_id, lb_pair.bin_step)?;

            if !active_bin.is_empty(!swap_for_y) {
                let SwapResult {
                    amount_in_with_fees,
                    amount_out,
                    fee,
                    ..
                } = active_bin.swap(amount_left, price, swap_for_y, &lb_pair, None)?;

                amount_left = amount_left
                    .checked_sub(amount_in_with_fees)
                    .ok_or("MathOverflow").unwrap();

                total_amount_out = total_amount_out
                    .checked_add(amount_out)
                    .ok_or("MathOverflow").unwrap();
                total_fee = total_fee.checked_add(fee).ok_or("MathOverflow").unwrap();
            }

            iterations += 1;
            require!(iterations < MAX_ITERATIONS, CustomError::ExceededMaxIterationsQuoteExactIn);

            if amount_left > 0 {
                lb_pair.advance_active_bin(swap_for_y)?;
            }
            else {
                break;
            }
        }
    }

    let transfer_fee_excluded_amount_out =
        calculate_transfer_fee_excluded_amount(out_mint_account, total_amount_out, epoch)?.amount;

    Ok(SwapExactInQuote {
        amount_out: transfer_fee_excluded_amount_out,
        fee: total_fee,
    })
}

pub fn get_bin_array_pubkeys_for_swap(
    lb_pair_pubkey: Pubkey,
    lb_pair: &LbPair,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    swap_for_y: bool,
    take_count: u8,
) -> Result<Vec<Pubkey>> {
    let mut start_bin_array_idx = BinArray::bin_id_to_bin_array_index(lb_pair.active_id)?;
    let mut bin_array_idx = vec![];
    let increment = if swap_for_y { -1 } else { 1 };

    loop {
        if bin_array_idx.len() == take_count as usize {
            break;
        }

        msg!(
            "Getting bin array pubkeys for swap, start_bin_array_idx: {}, active_id: {}, increment: {}",
            start_bin_array_idx,
            lb_pair.active_id,
            increment
        );

        if lb_pair.is_overflow_default_bin_array_bitmap(start_bin_array_idx) {
            let Some(bitmap_extension) = bitmap_extension else {
                msg!("Out of search range. No extension.");
                break;
            };
            let Ok((next_bin_array_idx, has_liquidity)) = bitmap_extension
                .next_bin_array_index_with_liquidity(swap_for_y, start_bin_array_idx)
            else {
                msg!("Out of search range. No liquidity.");
                break;
            };
            if has_liquidity {
                bin_array_idx.push(next_bin_array_idx);
                start_bin_array_idx = next_bin_array_idx + increment;
            } else {
                // Switch to internal bitmap
                start_bin_array_idx = next_bin_array_idx;
            }
        } else {
            let Ok((next_bin_array_idx, has_liquidity)) = lb_pair
                .next_bin_array_index_with_liquidity_internal(swap_for_y, start_bin_array_idx)
            else {
                msg!("next bin array idx and has liquidity not set, exiting loop");
                break;
            };
            if has_liquidity {
                bin_array_idx.push(next_bin_array_idx);
                start_bin_array_idx = next_bin_array_idx + increment;
            } else {
                // Switch to external bitmap
                start_bin_array_idx = next_bin_array_idx;
            }
            msg!("-----> next_bin_array_idx = {}, has_liquidity {}", next_bin_array_idx, has_liquidity);
        }
    }

    let bin_array_pubkeys = bin_array_idx
        .into_iter()
        .map(|idx| derive_bin_array_pda(lb_pair_pubkey, idx.into()).0)
        .collect();

    Ok(bin_array_pubkeys)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use anchor_lang::prelude::*;
//     use anchor_lang::prelude::sysvar::clock;
//     // use anchor_lang::clock::Clock;
//     // use anchor_client::{
//     //     solana_client::nonblocking::rpc_client::RpcClient, solana_sdk::pubkey::Pubkey, Cluster,
//     // };

//     /// Get on chain clock
//     fn get_clock() -> Result<clock::Clock> {
//         // Use the sysvar directly in on-chain programs
//         let clock = clock::Clock::get()?;
//         Ok(clock)
//     }

//     fn get_bytemuck_account<T: bytemuck::Pod>(
//         context: &Context<Maint>,
//         pubkey: &Pubkey
//     ) -> Option<T> {
//         let account_info = if let Some(acc) = context.remaining_accounts.iter().find(|acc| acc.key == pubkey) {
//             acc
//         } else {
//             return None;
//         };
        
//         let data: T = bytemuck::pod_read_unaligned(&account_info.data.borrow()[8..]);
//         Some(data)
//     }

//     #[test]
//     fn test_swap_quote_exact_out() {
//         // RPC client. No gPA is required.
//         // let rpc_client = RpcClient::new(Cluster::Mainnet.url().to_string());

//         let sol_usdc = Pubkey::from_str_const("HTvjzsfX3yU6BUodCjZ5vZkUrAxMDTrBs3CJaq43ashR");

//         let lb_pair = LbPair {
//             parameters: StaticParameters::default(),
//             v_parameters: VariableParameters::default(),
//             bump_seed: [0u8; 1],
//             bin_step_seed: [0u8; 2],
//             pair_type: 0u8,
//             active_id: 0i32,
//             bin_step: 100u16,
//             status: 0u8,
//             require_base_factor_seed: 0u8,
//             base_factor_seed: [0u8; 2],
//             activation_type: 0u8,
//             creator_pool_on_off_control: 0u8,
//             token_x_mint: Pubkey::new_unique(),
//             token_y_mint: Pubkey::new_unique(),
//             reserve_x: Pubkey::new_unique(),
//             reserve_y: Pubkey::new_unique(),
//             protocol_fee: ProtocolFee {
//                 amount_x: 0u64,
//                 amount_y: 0u64,
//             },
//             _padding_1: [0u8; 32],
//             reward_infos: [RewardInfo {
//                 mint: Pubkey::new_unique(),
//                 vault: Pubkey::new_unique(),
//                 funder: Pubkey::new_unique(),
//                 reward_duration: 0u64,
//                 reward_duration_end: 0u64,
//                 reward_rate: 0u128,
//                 last_update_time: 0u64,
//                 cumulative_seconds_with_empty_liquidity_reward: 0u64,
//             }; 2],
//             oracle: Pubkey::new_unique(),
//             bin_array_bitmap: [0u64; 16],
//             last_updated_at: get_current_time_test() as i64,
//             _padding_2: [0u8; 32],
//             pre_activation_swap_address: Pubkey::default(),
//             base_key: *lb_pair,
//             activation_point: 0u64,
//             pre_activation_duration: 0u64,
//             _padding_3: [0u8; 8],
//             _padding_4: 0u64,
//             creator: Pubkey::default(),
//             token_mint_x_program_flag: 0u8,
//             token_mint_y_program_flag: 0u8,
//             _reserved: [0u8; 22],
//         };

//         let mut mint_accounts = get_multiple_accounts(&[lb_pair.token_x_mint, lb_pair.token_y_mint]).unwrap();

//         let mint_x_account = mint_accounts[0].take().unwrap();
//         let mint_y_account = mint_accounts[1].take().unwrap();

//         // 3 bin arrays to left, and right is enough to cover most of the swap, and stay under 1.4m CU constraint.
//         // Get 3 bin arrays to the left from the active bin
//         let left_bin_array_pubkeys =
//             get_bin_array_pubkeys_for_swap(sol_usdc, &lb_pair, None, true, 3).unwrap();

//         // Get 3 bin arrays to the right the from active bin
//         let right_bin_array_pubkeys =
//             get_bin_array_pubkeys_for_swap(sol_usdc, &lb_pair, None, false, 3).unwrap();

//         // Fetch bin arrays
//         let bin_array_pubkeys = left_bin_array_pubkeys
//             .into_iter()
//             .chain(right_bin_array_pubkeys.into_iter())
//             .collect::<Vec<Pubkey>>();

//         let accounts = get_multiple_accounts(&bin_array_pubkeys).unwrap();

//         let bin_arrays = accounts
//             .into_iter()
//             .zip(bin_array_pubkeys.into_iter())
//             .map(|(account, key)| {
//                 (
//                     key,
//                     bytemuck::pod_read_unaligned(&account.unwrap().data[8..]),
//                 )
//             })
//             .collect::<HashMap<_, _>>();

//         let usdc_token_multiplier = 1_000_000.0;
//         let sol_token_multiplier = 1_000_000_000.0;

//         let out_sol_amount = 1_000_000_000;
//         let clock = get_clock().unwrap();

//         let quote_result = quote_exact_out(
//             sol_usdc,
//             &lb_pair,
//             out_sol_amount,
//             false,
//             bin_arrays.clone(),
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();

//         let in_amount = quote_result.amount_in + quote_result.fee;

//         let quote_result = quote_exact_in(
//             sol_usdc,
//             &lb_pair,
//             in_amount,
//             false,
//             bin_arrays.clone(),
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();


//         let out_usdc_amount = 200_000_000;

//         let quote_result = quote_exact_out(
//             sol_usdc,
//             &lb_pair,
//             out_usdc_amount,
//             true,
//             bin_arrays.clone(),
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();

//         let in_amount = quote_result.amount_in + quote_result.fee;

//         let quote_result = quote_exact_in(
//             sol_usdc,
//             &lb_pair,
//             in_amount,
//             true,
//             bin_arrays,
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();
//     }

//     #[test]
//     fn test_swap_quote_exact_in() {
//         // RPC client. No gPA is required.
//         // let rpc_client = RpcClient::new(Cluster::Mainnet.url().to_string());

//         let sol_usdc = Pubkey::from_str_const("HTvjzsfX3yU6BUodCjZ5vZkUrAxMDTrBs3CJaq43ashR");

//         let lb_pair_account = get_account(&sol_usdc).unwrap();

//         let lb_pair: LbPair = bytemuck::pod_read_unaligned(&lb_pair_account.data[8..]);

//         let mut mint_accounts = get_multiple_accounts(&[lb_pair.token_x_mint, lb_pair.token_y_mint]).unwrap();

//         let mint_x_account = mint_accounts[0].take().unwrap();
//         let mint_y_account = mint_accounts[1].take().unwrap();

//         // 3 bin arrays to left, and right is enough to cover most of the swap, and stay under 1.4m CU constraint.
//         // Get 3 bin arrays to the left from the active bin
//         let left_bin_array_pubkeys =
//             get_bin_array_pubkeys_for_swap(sol_usdc, &lb_pair, None, true, 3).unwrap();

//         // Get 3 bin arrays to the right the from active bin
//         let right_bin_array_pubkeys =
//             get_bin_array_pubkeys_for_swap(sol_usdc, &lb_pair, None, false, 3).unwrap();

//         // Fetch bin arrays
//         let bin_array_pubkeys = left_bin_array_pubkeys
//             .into_iter()
//             .chain(right_bin_array_pubkeys.into_iter())
//             .collect::<Vec<Pubkey>>();

//         let accounts = get_multiple_accounts(&bin_array_pubkeys).unwrap();

//         let bin_arrays = accounts
//             .into_iter()
//             .zip(bin_array_pubkeys.into_iter())
//             .map(|(account, key)| {
//                 (
//                     key,
//                     bytemuck::pod_read_unaligned(&account.unwrap().data[8..]),
//                 )
//             })
//             .collect::<HashMap<_, _>>();

//         // 1 SOL -> USDC
//         let in_sol_amount = 1_000_000_000;

//         let clock = get_clock().unwrap();

//         let quote_result = quote_exact_in(
//             sol_usdc,
//             &lb_pair,
//             in_sol_amount,
//             true,
//             bin_arrays.clone(),
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();

//         // 100 USDC -> SOL
//         let in_usdc_amount = 100_000_000;

//         let quote_result = quote_exact_in(
//             sol_usdc,
//             &lb_pair,
//             in_usdc_amount,
//             false,
//             bin_arrays.clone(),
//             None,
//             &clock,
//             &mint_x_account,
//             &mint_y_account,
//         )
//         .unwrap();
//     }
// }
