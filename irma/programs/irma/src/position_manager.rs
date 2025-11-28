// programs/irma/src/position_manager.rs
//
// This module provides the account contexts for interacting with Meteora DLMMpools.
// The actual price-to-tick conversion and liquidity management is handled by 
// Meteora's DLMMpool program and TypeScript SDK.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use rust_decimal::{prelude::FromPrimitive, prelude::ToPrimitive, Decimal, MathematicalOps};
use std::str::FromStr;

use crate::pair_config::PairConfig;
use crate::bin_array_manager::BinArrayManager;

use commons::dlmm::accounts::*;
use commons::dlmm::types::Bin;
use commons::u64x64_math::pow;
use commons::bin::*;
use commons::position::*;
use commons::{ONE, BASIS_POINT_MAX, SCALE_OFFSET, CustomError};

// Serializable version of Mint info
#[account]
#[derive(Debug)]
pub struct MintInfo {
    pub mint_authority: Option<Pubkey>,
    pub supply: u64,
    pub decimals: u8,
    pub is_initialized: bool,
    pub freeze_authority: Option<Pubkey>,
}

impl From<&Mint> for MintInfo {
    fn from(mint: &Mint) -> Self {
        MintInfo {
            mint_authority: mint.mint_authority.into(),
            supply: mint.supply,
            decimals: mint.decimals,
            is_initialized: mint.is_initialized,
            freeze_authority: mint.freeze_authority.into(),
        }
    }
}

#[account]
#[derive(Debug)]
pub struct MintWithProgramId {
    pub mint_info: MintInfo,
    pub program_id: Pubkey,
}

#[account]
#[derive(Debug)]
pub struct PositionEntry {
    pub pubkey: Pubkey,
    pub position: SinglePosition,
}

#[account]
#[derive(Debug)]
pub struct TokenEntry {
    pub pubkey: Pubkey,
    pub mint_with_program: MintWithProgramId,
}

// Code below is from state.rs in Meteora SDK, adapted for our use case.

#[account]
#[derive(Debug)]
pub struct AllPosition {
    pub all_positions: Vec<PositionEntry>,  // Use struct instead of tuple
    pub tokens: Vec<TokenEntry>,            // Use struct instead of tuple
}

impl AllPosition {
    pub fn new(config: &Vec<PairConfig>) -> Result<Self> {
        let mut all_positions = Vec::new();
        for pair in config.iter() {
            let pool_pk = Pubkey::from_str(&pair.pair_address).unwrap();
            let position_entry = PositionEntry {
                pubkey: pool_pk,
                position: SinglePosition::new(pool_pk),
            };
            all_positions.push(position_entry);
        }
        Ok(AllPosition {
            all_positions,
            tokens: Vec::new(),
        })
    }
    
    // Helper methods to work with Vec like HashMap
    pub fn get_position(&self, pubkey: &Pubkey) -> Option<&SinglePosition> {
        self.all_positions.iter()
            .find(|entry| &entry.pubkey == pubkey)
            .map(|entry| &entry.position)
    }
    
    pub fn get_position_mut(&mut self, pubkey: &Pubkey) -> Option<&mut SinglePosition> {
        self.all_positions.iter_mut()
            .find(|entry| &entry.pubkey == pubkey)
            .map(|entry| &mut entry.position)
    }
    
    // Helper methods for tokens
    pub fn get_token(&self, pubkey: &Pubkey) -> Option<&MintWithProgramId> {
        self.tokens.iter()
            .find(|entry| &entry.pubkey == pubkey)
            .map(|entry| &entry.mint_with_program)
    }
}

#[account]
#[derive(Default, Debug)]
pub struct SinglePosition {
    pub lb_pair: Pubkey,
    // Remove non-serializable types - these will be fetched dynamically
    // pub lb_pair_state: Option<LbPair>,
    // pub bin_arrays: Vec<(Pubkey, BinArray)>,
    // pub positions: Vec<PositionV2>,
    
    pub position_pks: Vec<Pubkey>,  // Keep pubkeys to fetch positions dynamically
    pub rebalance_time: u64,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub last_update_timestamp: u64,
}

const SLIPPAGE_RATE: u64 = 300; // 3%
const BASIC_POINT_MAX: u64 = 10_000;

impl SinglePosition {
    pub fn inc_rebalance_time(&mut self) {
        self.rebalance_time += 1;
    }

    pub fn get_min_out_amount_with_slippage_rate(
        &self,
        amount_in: u64,
        swap_for_y: bool,
        lb_pair_state: &LbPair,  // Pass as parameter instead of storing
    ) -> Result<u64> {
        let price = PositionRaw::get_price_from_id(lb_pair_state.active_id, lb_pair_state.bin_step)?;
        let out_amount = Bin::get_amount_out(amount_in, price, swap_for_y)?;

        let min_out_amount =
            match out_amount.checked_mul(BASIC_POINT_MAX - SLIPPAGE_RATE) {
                Some(val) => val.checked_div(BASIC_POINT_MAX).unwrap(),
                None => out_amount.checked_div(BASIC_POINT_MAX).unwrap().checked_mul(BASIC_POINT_MAX - SLIPPAGE_RATE).unwrap(),
            };

        msg!("    min_out_amount {}", min_out_amount);

        Ok(min_out_amount)
    }

    /// Calculate total position amounts and fees across all positions
    pub fn get_positions(
        &self, 
        positions: &[PositionV2],           // Pass as parameter
        bin_arrays: &[(Pubkey, BinArray)],  // Pass as parameter
        lb_pair_state: &LbPair,             // Pass as parameter
    ) -> Result<PositionRaw> {
        if positions.len() == 0 {
            return Ok(PositionRaw::default());
        }

        let mut amount_x = 0u64;
        let mut amount_y = 0u64;

        let mut fee_x = 0u64;
        let mut fee_y = 0u64;

        for position in positions.iter() {
            let bin_array_keys = position.get_bin_array_keys_coverage()?;
            let mut bin_arrays_for_position = vec![];

            for key in bin_array_keys {
                let bin_array_state = bin_arrays.iter()
                    .find(|(array_key, _)| array_key == &key)
                    .map(|(_, array)| array)
                    .ok_or(error!(CustomError::CannotGetBinArray))?;
                bin_arrays_for_position.push(*bin_array_state);
            }

            let bin_array_manager = BinArrayManager {
                bin_arrays: &bin_arrays_for_position,
            };

            for (i, liquidity_share) in position.liquidity_shares.iter().enumerate() {
                let bin_id = position
                    .lower_bin_id
                    .checked_add(i as i32).unwrap();

                let bin = bin_array_manager.get_bin(bin_id)?;
                let (bin_amount_x, bin_amount_y) = bin.calculate_out_amount(*liquidity_share)?;

                amount_x = amount_x
                    .checked_add(bin_amount_x).unwrap();

                amount_y = amount_y
                    .checked_add(bin_amount_y).unwrap();
            }

            let (fee_x_pending, fee_y_pending) =
                bin_array_manager.get_total_fee_pending(position)?;

            fee_x = fee_x
                .checked_add(fee_x_pending).unwrap();
            fee_y = fee_y
                .checked_add(fee_y_pending).unwrap();
        }

        Ok(PositionRaw {
            position_len: positions.len(),
            bin_step: lb_pair_state.bin_step,
            rebalance_time: self.rebalance_time,
            min_bin_id: self.min_bin_id,
            active_id: lb_pair_state.active_id,
            max_bin_id: self.max_bin_id,
            amount_x,
            amount_y,
            fee_x,
            fee_y,
            last_update_timestamp: self.last_update_timestamp,
        })
    }
}

#[derive(Default, PartialEq, Debug, Clone)]
pub struct PositionRaw {
    pub position_len: usize,
    pub rebalance_time: u64,
    pub max_bin_id: i32,
    pub active_id: i32,
    pub min_bin_id: i32,
    pub bin_step: u16,
    pub amount_x: u64,
    pub amount_y: u64,
    pub fee_x: u64,
    pub fee_y: u64,
    pub last_update_timestamp: u64,
}

impl PositionRaw {
    pub fn to_position_info(
        &self,
        token_x_decimals: u8,
        token_y_decimals: u8,
    ) -> Result<PositionInfo> {
        let bin_step = self.bin_step;

        let ui_price_adjustment_factor =
            Decimal::TEN.powi(token_x_decimals as i64 - token_y_decimals as i64);

        let token_x_ui_adjustment_factor = 10f64.powi(token_x_decimals.into());
        let token_y_ui_adjustment_factor = 10f64.powi(token_y_decimals.into());

        let min_price_fp = PositionRaw::get_price_from_id(self.min_bin_id, bin_step)?;
        let min_price =
            Decimal::from_u128(min_price_fp).unwrap() / Decimal::from(ONE);
        let adjusted_min_price = min_price
            .checked_mul(ui_price_adjustment_factor.into())
            .unwrap()
            .to_f64()
            .unwrap();

        let max_price_fp = PositionRaw::get_price_from_id(self.max_bin_id, bin_step)?;
        let max_price =
            Decimal::from_u128(max_price_fp).unwrap() / Decimal::from(ONE);
        let adjusted_max_price = max_price
            .checked_mul(ui_price_adjustment_factor.into())
            .unwrap()
            .to_f64()
            .unwrap();

        let current_price_fp = PositionRaw::get_price_from_id(self.active_id, bin_step)?;
        let current_price =
            Decimal::from_u128(current_price_fp).unwrap() / Decimal::from(ONE);
        let adjusted_current_price = current_price
            .checked_mul(ui_price_adjustment_factor.into())
            .unwrap()
            .to_f64()
            .unwrap();

        let amount_x = self.amount_x as f64 / token_x_ui_adjustment_factor;
        let amount_y = self.amount_y as f64 / token_y_ui_adjustment_factor;

        let fee_x = self.fee_x as f64 / token_x_ui_adjustment_factor;
        let fee_y = self.fee_y as f64 / token_y_ui_adjustment_factor;

        return Ok(PositionInfo {
            position_len: self.position_len,
            rebalance_time: self.rebalance_time,
            max_price: adjusted_max_price,
            current_price: adjusted_current_price,
            min_price: adjusted_min_price,
            amount_x,
            amount_y,
            fee_x,
            fee_y,
            last_update_timestamp: self.last_update_timestamp,
        });
    }

    pub fn get_price_from_id(active_id: i32, bin_step: u16) -> Result<u128> {
        let bps = u128::from(bin_step)
            .checked_shl(SCALE_OFFSET.into())
            .unwrap()
            .checked_div(BASIS_POINT_MAX as u128)
            .unwrap();

        let base = ONE.checked_add(bps).unwrap();

        Ok(pow(base, active_id).unwrap())
    }
}

#[derive(Default, PartialEq, Debug, Clone)]
pub struct PositionInfo {
    pub position_len: usize,
    pub rebalance_time: u64,
    pub max_price: f64,
    pub current_price: f64,
    pub min_price: f64,
    pub amount_x: f64,
    pub amount_y: f64,
    pub fee_x: f64,
    pub fee_y: f64,
    pub last_update_timestamp: u64,
}

impl SinglePosition {
    pub fn new(lb_pair: Pubkey) -> Self {
        SinglePosition {
            lb_pair,
            rebalance_time: 0,
            position_pks: vec![],
            min_bin_id: 0,
            max_bin_id: 0,
            last_update_timestamp: 0,
        }
    }
}

pub fn get_decimals(token_mint_pk: Pubkey, all_tokens: &[TokenEntry]) -> u8 {
    let token = all_tokens.iter()
        .find(|entry| entry.pubkey == token_mint_pk)
        .unwrap();
    return token.mint_with_program.mint_info.decimals;
}
