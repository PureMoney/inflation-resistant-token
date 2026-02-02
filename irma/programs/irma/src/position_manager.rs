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
use crate::errors::CustomError;

use commons::dlmm::accounts::*;
use commons::dlmm::types::Bin;
use commons::bin::*;
use commons::position::*;
use commons::ONE;
use commons::conversions::*;
use commons::math::*;

pub const MAX_POSITIONS: usize = 2; // allow only 2 positions per pair

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
pub struct TokenEntry {
    pub pubkey: Pubkey,
    pub mint_with_program: MintWithProgramId,
}

// Code below is from state.rs in Meteora SDK, adapted for our use case.

#[account]
#[derive(Debug)]
pub struct AllPosition {
    pub all_positions: Vec<SinglePosition>,  // Each LbPair has its own SinglePosition
    pub tokens: Vec<TokenEntry>,            // All tokens used across pairs
}

impl AllPosition {
    pub fn new(config: &Vec<PairConfig>) -> Result<Self> {
        let mut all_positions = Vec::new();
        for pair in config.iter() {
            let pool_pk = Pubkey::from_str(&pair.pair_address).unwrap();
            let position_entry = SinglePosition::new(pool_pk);
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
            .find(|entry| &entry.lb_pair == pubkey)
    }
    
    pub fn get_position_mut(&mut self, pubkey: &Pubkey) -> Option<&mut SinglePosition> {
        self.all_positions.iter_mut()
            .find(|entry| &entry.lb_pair == pubkey)
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
    pub bin_array_pks: Vec<Pubkey>, // Keep bin array keys to fetch dynamically
    pub position_pks: Vec<Pubkey>,  // Keep pubkeys to fetch positions dynamically
    pub rebalance_time: u64,
    pub min_bin_id: i32, // use this to track current redemption price bin
    pub max_bin_id: i32, // use this to track current minting price bin
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

    /// Get total liquidity in a position
    /// This assumes that position_pks has both position keys: [mint_position_pk, redeem_position_pk]
    pub fn get_liquidity_in_position<'a>(
        &self,
        acct_infos: &'a [AccountInfo<'a>],
    ) -> Result<(u64, u64)> {
        let raw: PositionRaw = self.get_positions_total(acct_infos)?;

        Ok((raw.amount_x, raw.amount_y))
    }

    /// Calculate total position amounts and fees across all positions
    /// Note: this does not distinguish between mint and redeem positions.
    pub fn get_positions_total<'a>(
        &self,
        acct_infos: &'a [AccountInfo<'a>],
    ) -> Result<PositionRaw> {
        // msg!("Fetching total position for LB Pair {}", self.lb_pair);

        // Fetch positions
        let mut positions = fetch_positions(acct_infos, &self.position_pks)?;
         // msg!("    --> fetched {} positions", positions.len());

        if positions.len() == 0 || positions.len() > MAX_POSITIONS {
            return Ok(PositionRaw::default());
        }
        if positions[0].lower_bin_id == positions[1].lower_bin_id {
            positions.remove(1);
        }
        
        // Fetch bin arrays
        let mut bin_arrays = fetch_bin_arrays(acct_infos, &self.bin_array_pks)?;
        // msg!("    --> fetched {} bin arrays", bin_arrays.len());

        // bin arrays must be present because positions exist
        if bin_arrays.len() == 0 || bin_arrays.len() > MAX_POSITIONS {
            Err(Error::from(CustomError::FailedToFetchBinArrays))?;
        }
        if bin_arrays[0].0 == bin_arrays[1].0 {
            bin_arrays.remove(1);
        }

        let mut amount_x = 0u64;
        let mut amount_y = 0u64;

        let mut fee_x = 0u64;
        let mut fee_y = 0u64;

        for position in positions.iter() {
            let mut bin_array_keys: Vec<Pubkey> = Vec::new();
            position.get_bin_array_keys_coverage(&mut bin_array_keys)?;
            let mut bin_arrays_for_position = vec![];

            msg!("    --> position lower_bin_id: {}, liquidity_shares len: {}",
                position.lower_bin_id, position.liquidity_shares.len());

            msg!("    --> position upper_bin_id: {}, bin_array_keys len: {}",
                position.upper_bin_id, bin_array_keys.len());

            for key in bin_array_keys {
                let bin_array_state = bin_arrays.iter()
                    .find(|(array_key, _)| array_key == &key)
                    .map(|(_, array)| array);
                    // .ok_or(None); // error!(CustomError::CannotGetBinArray))?;
                if !bin_array_state.is_some() {
                    continue;
                }
                bin_arrays_for_position.push(*bin_array_state.unwrap());
            }
            if bin_arrays_for_position.len() == 0 {
                msg!("    --> no bin arrays found for position, skipping...");
                continue;
            }

            let bin_array_manager = BinArrayManager {
                bin_arrays: &bin_arrays_for_position,
            };

            for (i, liquidity_share) in position.liquidity_shares.iter().enumerate() {
                if *liquidity_share == 0 {
                    continue;
                }

                let bin_id = position
                    .lower_bin_id
                    .checked_add(i as i32).unwrap();

                msg!("    --> getting bin_id: {}", bin_id);
                let bin = bin_array_manager.get_bin(bin_id)?;
                msg!("    --> bin found: price {}", bin.price);
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
        msg!("    --> total fees - x: {}, y: {}", fee_x, fee_y);

        // Fetch lb pair state
        let lb_pair_state = fetch_lb_pair_state(acct_infos, &self.lb_pair)?;

        msg!("    --> lb_pair_state fetched");

        Ok(PositionRaw {
            position_len: self.position_pks.len(),
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

    /// Find bin ID for a given price using mathematical inverse instead of binary search
    /// This is the inverse of get_price_from_id()
    pub fn search_bin_given_price(
        lb_pair_state: &LbPair,
        target_price: u128,
    ) -> Result<i32> {
        // msg!("    search bin, target price: {}", target_price);
        let bin_step = lb_pair_state.bin_step;
        
        // DLMM price formula: price = base_price * (1 + bin_step / 10000)^bin_id
        // Where base_price is the price at bin_id = 0
        // Inverse: bin_id = log(price / base_price) / log(1 + bin_step / 10000)
        
        const SCALE_OFFSET: u128 = 1 << 64; // 2^64 for precision
        const BASE_FACTOR: u128 = 10000; // DLMM base factor
        
        // Get base price (price at bin_id = 0)
        let base_price = PositionRaw::get_price_from_id(0, bin_step)?;
        
        // Handle edge case: if target_price equals base_price, bin_id = 0
        if target_price == base_price {
            return Ok(0);
        }
        
        // Calculate bin_step_factor = 1 + bin_step / 10000
        // We'll work with scaled integers to avoid floating point
        // let bin_step_factor_scaled = BASE_FACTOR + (bin_step as u128);
        
        // Calculate price_ratio = target_price / base_price (scaled)
        let price_ratio_scaled = target_price * (SCALE_OFFSET / base_price);
        
        // Use integer logarithm approximation
        // For small bin_step values, we can use: bin_id ≈ (price_ratio - 1) / (bin_step / 10000)
        if target_price > base_price {
            // Positive bin_id case
            let ratio_minus_one = price_ratio_scaled - SCALE_OFFSET;
            let bin_step_scaled = (bin_step as u128) * SCALE_OFFSET / BASE_FACTOR;
            let bin_id_approx = (ratio_minus_one / bin_step_scaled) as i32;
            
            // Refine the approximation by checking nearby bins
            let start_bin = bin_id_approx.saturating_sub(2);
            let end_bin = bin_id_approx.saturating_add(2);
            
            Self::find_closest_bin(lb_pair_state, target_price, start_bin, end_bin)
        } else {
            // Negative bin_id case (target_price < base_price)
            let one_minus_ratio = SCALE_OFFSET - price_ratio_scaled;
            let bin_step_scaled = (bin_step as u128) * SCALE_OFFSET / BASE_FACTOR;
            let bin_id_approx = -((one_minus_ratio / bin_step_scaled) as i32);
            
            // Refine the approximation by checking nearby bins
            let start_bin = bin_id_approx.saturating_sub(2);
            let end_bin = bin_id_approx.saturating_add(2);
            
            Self::find_closest_bin(lb_pair_state, target_price, start_bin, end_bin)
        }
    }
    
    /// Helper function to find the closest bin within a small range
    fn find_closest_bin(
        lb_pair_state: &LbPair,
        target_price: u128,
        start_bin: i32,
        end_bin: i32,
    ) -> Result<i32> {
        let bin_step = lb_pair_state.bin_step;
        let min_bin = lb_pair_state.parameters.min_bin_id;
        let max_bin = lb_pair_state.parameters.max_bin_id;
        
        let mut best_bin = start_bin.max(min_bin).min(max_bin);
        let mut best_diff = u128::MAX;
        
        for bin_id in start_bin.max(min_bin)..=end_bin.min(max_bin) {
            let bin_price = PositionRaw::get_price_from_id(bin_id, bin_step)?;
            let diff = if bin_price > target_price {
                bin_price - target_price
            } else {
                target_price - bin_price
            };
            
            if diff < best_diff {
                best_diff = diff;
                best_bin = bin_id;
            }
        }
        
        Ok(best_bin)
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
        Ok(price_math::get_price_from_id(active_id, bin_step).unwrap())
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
            bin_array_pks: vec![],
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
