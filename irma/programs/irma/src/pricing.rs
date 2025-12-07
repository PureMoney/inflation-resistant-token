#![allow(unexpected_cfgs)]

use std::string::String;
use std::option::Option;
use std::collections::BTreeMap;
use std::mem::size_of;


use anchor_lang::prelude::*;
use static_assertions::const_assert;

use crate::{Init, Maint};
use crate::errors::CustomError;


// Maximum number of stablecoins supported
// This is limited by the maximum size of the account data (10,240 bytes).
// Each stablecoin entry in the reserves requires approximately 120 + 32 bytes of storage.
// Therefore, the maximum number of stablecoins is calculated as 10,240 / 152 = 67 (rounded down).
pub const MAX_BACKING_COUNT: usize = 40; // 67;

// Minimum minatable amount. Any request to mint below this amount will error out.
// There is no maximum mint amount. Large mint requests are good for the system.
// Users are advised to start small to test the system, then increase their mint amounts gradually.
pub const MIN_MINT_AMOUNT: u128 = 100u128;

// Maximum redeemable amount per transaction.
// Any small amount below this amount is OK.
// Any amount above this will error out, but multiple transactions totalling more than this are allowed.
// Users are required to breack up large redemptions into multiple transactions.
// This is to prevent sudden large price movements that can destabilize the system.
pub const MAX_REDEEM_AMOUNT: u128 = 100_000u128;

// Maximum mint price.
// If the mint price exceeds this value, minting will be disabled for that stablecoin.
// The reason is that it means this reserve stablecoin would have lost significant value,
// and therefore it is no longer suitable as a backing stablecoin for IRMA.
// Users should choose another stablecoin to mint IRMA.
// If the USD itself loses significant value, then it's "goodbye" USD.
// If IRMA has gained significant network effects by then, then it should be able to survive.
pub const MAX_MINT_PRICE: f64 = 10_000.0;

/// IRMA module

pub fn init_pricing(ctx: &mut Context<Init>) -> Result<()> {
    msg!("Greetings from: {:?}", ctx.program_id);
    let state = &ctx.accounts.state;
    if state.reserves.len() > 0 {
        msg!("State already initialized, skipping init...");
        return Ok(());
    }
    *ctx.accounts.state = StateMap::new();
    let state = &mut ctx.accounts.state;
    state.bump = 13u8; // InitializeBumps::bump(ctx.bumps).unwrap_or(0);
    msg!("State initialized with bump: {}", state.bump);

    // state.init_reserves()?;
    // msg!("Initial stablecoins added to the state.");

    Ok(())
}

/// The whole purpose for using a BTreeMap (now a Vec) is to allow for easy addition of new stablecoins.
pub fn add_reserve(
        ctx: Context<Maint>, 
        symbol: &str, 
        mint_address: Pubkey,
        backing_decimals: u8) -> Result<()> 
{
    let state = &mut ctx.accounts.state;
    if state.reserves.len() >= MAX_BACKING_COUNT {
        msg!("Maximum number of stablecoins reached.");
        return Err(error!(CustomError::InvalidBacking));
    }
    let stablecoin = StableState::new(symbol, mint_address, backing_decimals as u64).unwrap();
    state.add_reserve(stablecoin.clone());
    msg!("Added stablecoin: {:?}", stablecoin);
    Ok(())
}

/// Remove a stablecoin from the reserves by its symbol.
pub fn remove_reserve(ctx: Context<Maint>, symbol: &str) -> Result<()> {
    let state = &mut ctx.accounts.state;
    if !state.contains_reserve(symbol) {
        msg!("Stablecoin {} not found in reserves.", symbol);
        return Err(error!(CustomError::InvalidBacking));
    }
    state.remove_reserve(symbol);
    msg!("Removed stablecoin: {}", symbol);
    Ok(())
}

/// Deactivate a reserve stablecoin.
pub fn disable_reserve(ctx: Context<Maint>, symbol: &str) -> Result<()> {
    let state = &mut ctx.accounts.state;
    if !state.contains_reserve(symbol) {
        msg!("Stablecoin {} not found in reserves.", symbol);
        return Err(error!(CustomError::InvalidBacking));
    }
    state.disable_reserve(symbol);
    msg!("Deactivated stablecoin: {}", symbol);
    Ok(())
}

fn validate_params(state_map: &StateMap, quote_token: &str) -> Result<()> {
    require!(state_map.reserves.len() > 0, CustomError::InvalidReserveList);
    require!(state_map.contains_reserve(quote_token), CustomError::InvalidQuoteToken);
    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    require!(stablecoin.active, CustomError::InvalidQuoteToken);
    require!(stablecoin.backing_decimals > 0, CustomError::InvalidQuoteToken);
    require!(stablecoin.mint_price > 0.0, CustomError::InvalidAmount);
    require!(stablecoin.irma_in_circulation > 0u128, CustomError::InsufficientCirculation);
    Ok(())
}

/// Set mint price for a given quote token based on inflation data.
/// This should be called for every backing stablecoin supported, only once per day
/// because Truflation updates the inflation data only once per day.
/// The mint price is the ACTUAL price of IRMA in terms of the quote token (no decimals).
pub fn set_mint_price(ctx: Context<Maint>, quote_token: &str, mint_price: f64) -> Result<()> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;
    require!(mint_price > 0.0, CustomError::InvalidAmount);
    require!(
        mint_price < MAX_MINT_PRICE,
        CustomError::RemoveReserve
    ); // sanity check, mint price should not be too high
    let stablecoin = state_map.get_mut_stablecoin(quote_token).unwrap();
    stablecoin.mint_price = mint_price;
    Ok(())
}

/// Mint IRMA tokens for a given amount of quote token.
/// The mint price is the price of IRMA in terms of the quote token, which is set by the Truflation oracle.
/// Input amount is  in quote token's smallest unit (e.g. 1 USDT = 10^6, 1 USDC = 10^6, etc.)
/// Input amount therefore is an unsigned integer suitable for on-chain processing, not for 
/// human consumption.
pub fn mint_irma(state_map: &mut Account<StateMap>, quote_token: &str, amount: u64) -> Result<()> {
    require!(amount >= 100_000_000u64, CustomError::InvalidAmount);
    validate_params(&(*state_map), quote_token)?;

    if amount == 0u64 { return Ok(()); };

    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    let curr_price: f64 = stablecoin.mint_price;
    // 10f64.powi(token_x_decimals.into())
    let amount = (amount as f64 / (10.0_f64).powi(stablecoin.backing_decimals as i32)) as f64;

    let stablecoin = state_map.get_mut_stablecoin(quote_token).unwrap();
    stablecoin.backing_reserves += amount.ceil() as u128; // backing should not have a fractional part
    stablecoin.irma_in_circulation += (amount / curr_price).ceil() as u128;

    Ok(())
}

/// RedeemIRMA - user surrenders IRMA in irma_amount, expecting to get back quote_token according to redemption price.
/// FIXME: If resulting redemption price increases by more than 0.0000001, then actual redemption price 
/// should be updated immediately.
pub fn redeem_irma(state_map: &mut Account<StateMap>, quote_token: &str, irma_amount: u64) -> Result<()> {
    validate_params(&(*state_map), quote_token)?;

    if irma_amount == 0 { return Ok(()) };

    let state = state_map.get_stablecoin(quote_token).unwrap();
    // There is a redemption rule: every redemption is limited to 100k IRMA or 10% of the IRMA in circulation (for
    // the quote token) whichever is smaller.
    let circulation: u128 = state.irma_in_circulation;
    let irma_amount = (irma_amount as f64 / (10.0_f64).powi(IRMA.backing_decimals as i32)).ceil() as u64;
    require!((irma_amount <= MAX_REDEEM_AMOUNT as u64), CustomError::InvalidIrmaAmount);
    require!(circulation >= irma_amount as u128, CustomError::InsufficientCirculation);

    state_map.distribute(quote_token, irma_amount)?;

    Ok(())
}

pub fn list_reserves(ctx: Context<Maint>) -> String {
    let state_map = &mut ctx.accounts.state;
    let sorted_list = state_map.list_reserves();
    sorted_list.join(", ")
}

pub fn get_reserve_info(ctx: Context<Maint>, quote_token: &str) -> Result<StableState> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;
    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    Ok(stablecoin.clone())
}

/// Get the current redemption price for a given quote token.
/// Redemption price = total backing reserves / total IRMA in circulation
pub fn get_redemption_price(ctx: Context<Maint>, quote_token: &str) -> Result<f64> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;
    
    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    let backing_reserves = stablecoin.backing_reserves;
    let irma_in_circulation = stablecoin.irma_in_circulation;

    if irma_in_circulation == 0u128 {
        return Ok(1.0); // Default to 1.0 if no IRMA in circulation
    }

    let ten_pow_decimals =  10.0_f64.powi(IRMA.backing_decimals as i32 - stablecoin.backing_decimals as i32);
    let redemption_price = (backing_reserves.checked_div(irma_in_circulation).unwrap_or(0) as f64) * ten_pow_decimals;
    Ok(redemption_price)
}

/// Get both mint and redemption prices for a given quote token.
pub fn get_prices(ctx: Context<Maint>, quote_token: &str) -> Result<(f64, f64)> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;
    
    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    let mint_price = stablecoin.mint_price;
    let redemption_price = get_redemption_price(ctx, quote_token)?;
    
    Ok((mint_price, redemption_price))
}

/// This is the stablecoin struct with the specs for each reserve stablecoin.
/// Pricing.rs maintains a Vec of these structs in the StateMap account.
/// Each stablecoin struct uses 128 bytes.
// #[account]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Debug)]
pub struct StableState {
    pub symbol: String, // symbol of the stablecoin, e.g. "USDT"
    pub mint_address: Pubkey, // mint address of the stablecoin
    pub backing_decimals: u64, // need only u8, but for alignment reasons we use u64
    pub mint_price: f64, // mint price of IRMA in terms of the backing stablecoin
    pub backing_reserves: u128, // backing reserves is in whole numbers (no decimals)
    pub irma_in_circulation: u128, // in whole numbers (no decimals)
    pub pool_id: Pubkey, // market ID in some Solana DEX
    pub active: bool, // whether the stablecoin is active or not
    pub extra: [u8; 15], // padding to make the size of the struct 25 * EnumCount + 8
}

const_assert!(
    size_of::<StableState>() > 144 // 16 + 32 + 8 + 8 + 16 + 16 + 32 + 1 + 15 = 144 bytes
);

// Additional useful assertions
const_assert!(size_of::<StableState>() > 0);
const_assert!(size_of::<StableState>() <= 160); // At least 152 bytes
const_assert!(MAX_BACKING_COUNT <= 67); // Ensure we don't exceed account size limits
const_assert!(MAX_BACKING_COUNT > 0); // Must support at least one stablecoin
// const_assert_eq!(align_of::<StableState>(), 8); // Ensure proper alignment

#[account]
#[derive(PartialEq, Debug)]
pub struct StateMap {
    pub reserves: Vec<StableState>,
    pub bump: u8, // Bump seed for PDA
    pub padding: [u8; 7], // padding to make the size of the struct 25 * EnumCount + 8
}

/// Immutable data for IRMA itself.
/// (Had to remove 'const' to allow Pubkey type and mutable string)
/// NOTE: This is hardly used. The only field used is backing_decimals.
pub const IRMA: StableState = StableState {
    symbol: String::new(), // should be "IRMA".to_string(), but doesn't work in const context
    mint_address: pubkey!("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1"), // IRMA mint address on Solana
    backing_decimals: 6,
    mint_price: 1.0,
    backing_reserves: 1u128,
    irma_in_circulation: 1u128,
    pool_id: pubkey!("11111111111111111111111111111111"), // unused for IRMA because it is the other side of every pair
    active: false, // IRMA cannot be a reserve backing of itself
    extra: [0; 15], // padding
};

impl StableState {

    pub fn new(symbol: &str, mint_address: Pubkey, backing_decimals: u64) -> Result<Self> {
        // msg!("StableState size: {}", size_of::<StableState>());
        // const len: usize = symbol.to_bytes().len();
        require!(symbol.len() <= 8 && symbol.len() > 0, CustomError::InvalidBackingSymbol);
        require!(mint_address != Pubkey::default(), CustomError::InvalidBackingAddress);
        require!(backing_decimals > 0, CustomError::InvalidBacking);
        Ok(StableState {
            symbol: symbol.to_string(), // symbol of the stablecoin, e.g. "USDT"
            mint_address,
            backing_decimals,
            mint_price: 1.0f64, // default mint price is 1.0
            backing_reserves: 1u128,
            irma_in_circulation: 1u128,
            pool_id: Pubkey::default(), // to be set later, outside of pricing.rs
            active: true,
            extra: [0; 15], // for future use
        })
    }
}

impl StateMap {
    pub fn new() -> Self {
        StateMap {
            reserves: Vec::with_capacity(MAX_BACKING_COUNT), // Initialize with capacity for MAX_BACKING_COUNT stablecoins
            bump: 0,
            padding: [0; 7], // padding to make the size of the struct 25 * EnumCount + 8
        }
    }

    /// Add a stablecoin to the reserves, maintaining the order by symbol.
    pub fn add_reserve(&mut self, stablecoin: StableState) {
        if self.contains_reserve(&stablecoin.symbol) {
            msg!("Stablecoin {} already exists in reserves, skipping addition.", stablecoin.symbol);
            return;
        }
        let clone = stablecoin.clone();
        let symbol = clone.symbol; // Get the symbol from the stablecoin
        let i = self.reserves.partition_point(|e| e.symbol.as_str() < symbol.as_str());
        self.reserves.insert(i, stablecoin);
    }

    /// Get a stablecoin by its symbol.
    pub fn get_stablecoin(&self, symbol: &str) -> Result<StableState> {
        require!(symbol.len() <= 8 && symbol.len() > 0, CustomError::InvalidBackingSymbol);
        if !self.contains_reserve(symbol) {
            msg!("Symbol {} not found in reserves.", symbol);
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        if self.reserves.len() == 1 {
            if self.reserves[0].symbol == symbol {
                return Ok(self.reserves[0].clone());
            }
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        let i = self.reserves.partition_point(|e| e.symbol.as_str() < symbol);
        if i >= self.reserves.len() {
            msg!("Symbol {} not found in reserves.", symbol);
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        // if cfg!(debug_assertions) {
        //     msg!("get_stablecoin: in {}, out {}", symbol, self.reserves[i].symbol);
        // }
        Ok(self.reserves.get(i).unwrap().clone())
    }

    pub fn get_mut_stablecoin(&mut self, symbol: &str) -> Result<&mut StableState> {
        require!(symbol.len() <= 8 && symbol.len() > 0, CustomError::InvalidBackingSymbol);
        if !self.contains_reserve(symbol) {
            msg!("Input {} not found in reserves.", symbol);
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        if self.reserves.len() == 1 {
            if self.reserves[0].symbol == symbol {
                return Ok(&mut self.reserves[0]);
            }
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        let i = self.reserves.partition_point(|e| e.symbol.as_str() < symbol);
        if i >= self.reserves.len() {
            if cfg!(debug_assertions) {
                msg!("Symbol {} not found in reserves, index: {}", symbol, i);
            }
            return Err(Error::from(CustomError::SymbolNotFound));
        }
        // if cfg!(debug_assertions) {
        //     msg!("get_mut_stablecoin: in {}, out {}", symbol, self.reserves[i].symbol);
        // }
        Ok(self.reserves.get_mut(i).unwrap())
    }

    pub fn get_stablecoin_symbol(&self, mint_address: Pubkey) -> Option<String> {
        for stablecoin in &self.reserves {
            if stablecoin.mint_address == mint_address {
                return Some(stablecoin.symbol.clone());
            }
        }
        None
    }

    pub fn remove_reserve(&mut self, symbol: &str) -> Option<StableState> {
        if !self.contains_reserve(symbol) {
            msg!("Stablecoin {} not found in reserves.", symbol);
            return None;
        }
        let i = self.reserves.partition_point(|e| e.symbol > symbol.to_string());
        Some(self.reserves.remove(i - 1))
    }

    pub fn disable_reserve(&mut self, symbol: &str) {
        let result = self.get_mut_stablecoin(symbol);
        if result.is_err() {    
            msg!("Stablecoin {} not found in reserves.", symbol);
            return;
        }
        let stablecoin = result.unwrap();
        if stablecoin.backing_decimals > 0 {
            stablecoin.active = false;
            msg!("Deactivated stablecoin: {}", symbol);
        } else {
            msg!("Stablecoin found in reserves: {}, but it's not valid", symbol);
        }
    }

    pub fn contains_reserve(&self, symbol: &str) -> bool {
        self.reserves.iter().any(|e| &e.symbol == symbol)
    }
    
    pub fn len(&self) -> usize {
        self.reserves.len()
    }

    pub fn list_reserves(&self) -> Vec<String> {
        let sorted_reserves = self.reserves.iter()
            .map(|e| e.symbol.clone())
            .collect::<Vec<_>>();
        sorted_reserves
    }

    /// Distrubute (ReduceCirculations) implementation
    /// This now deals with mint_price being less than redemption_price (a period of deflation).
    /// If the price of the underlying reserve goes up with respect to USD, its exchange rate with IRMA
    /// would improve (i.e. IRMA would be worth less in terms of the reserve). In this case, the system
    /// would be expected to have a higher redemption price for IRMA than mint price; however, because
    /// the objective is always to preserve the backing, the system will not allow the mint price 
    /// to be less than the redemption price. Instead, it will simply set the redemption price to the mint price.
    /// NOTE: irma_amount is now scaled down by the backing_decimals of IRMA.
    pub fn distribute(&mut self, quote_token: &str, irma_amount: u64) -> Result<()> {

        require!(quote_token.len() > 2, CustomError::InvalidQuoteToken);
        let reserves = &mut self.reserves;
        let clone_reserves = reserves.clone();

        // determine what this redemption does:
        // does it keep the relative spreads even, or does it skew the spreads?
        let mut count: u8 = 0;
        let mut average_diff: f64 = 0.0;
        let price_differences: BTreeMap<String, f64> = clone_reserves.iter()
            .enumerate()
            .filter_map(|(i, reserve)| {
                let key = reserve.symbol.to_string();
                let reserve = reserve.clone(); // clone to get a copy of the StableState
                // msg!("{}: {}", i, reserve.symbol.to_string());
                let circulation = reserve.irma_in_circulation;
                let backing_reserves = reserve.backing_reserves;
                let stablecoin = reserve.clone();
                let ten_pow_decimals = 10.0_f64.powi(
                    IRMA.backing_decimals as i32 - stablecoin.backing_decimals as i32
                );
                let redemption_price = (backing_reserves.checked_div(circulation).unwrap_or(0) as f64)
                     * (ten_pow_decimals as f64);
                let mint_price = reserve.mint_price;
                if mint_price == 0.0 || reserve.backing_decimals == 0 || reserve.active == false {
                    // msg!("Skipping {}: mint_price is 0.0 or backing_decimals is 0", Stablecoins::from_index(i).unwrap().to_string());
                    return Some((key, 0.0));
                }
                count += 1;
                if count != i as u8 + 1 {
                    msg!("Warning: count is not equal to index + 1, count: {}, index: {}", count, i);
                }
                let x: f64 = mint_price - redemption_price;
                average_diff += x;
                Some((key, x))
            })
            .collect();
        require!(count > 0, CustomError::InvalidBacking);
        // if count == 0 {
        //     // msg!("No price differences found, returning early.");
        //     return Ok(());
        // }
        average_diff /= count as f64;
        // msg!("Average price difference: {}", average_diff);

        let min_diff: f64 = 0.001; // price differences below this are ignored

        let mut max_price_diff: f64 = average_diff;
        let mut other_target: &String = &quote_token.to_string();
        for (_i, (key, price_diff)) in price_differences.iter().enumerate() {
            // msg!("{}: {}, max {}", i, *price_diff, max_price_diff);
            if (*price_diff - max_price_diff).abs() > min_diff && *price_diff > max_price_diff {
                max_price_diff = *price_diff;
                other_target = key;
            }
        }
        // msg!("Max token: {}", other_target.to_string());
        // msg!("Max price diff: {}", max_price_diff);

        let stablecoin = &self.get_stablecoin(quote_token).unwrap();
        let ro_circulation: u128 = stablecoin.irma_in_circulation;
        let reserve: u128 = stablecoin.backing_reserves;
        let ten_pow_decimals =  10.0_f64.powi(
            IRMA.backing_decimals as i32 - stablecoin.backing_decimals as i32
        );
        let redemption_price = ((reserve.checked_div(ro_circulation).unwrap_or(0) as f64)
             * ten_pow_decimals) as f64;
        let subject_adjustment: u64 = (irma_amount as f64 * redemption_price).ceil() as u64; // irma_amount is in whole numbers, so we can use it directly

        // no matter what, we need to reduce the subject reserve (quote_token)
        require!(reserve >= subject_adjustment as u128, CustomError::InsufficientReserve);
        let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
        mut_reserve.backing_reserves -= subject_adjustment as u128;

        // Now determine which other stableoin this redeemed circulation can be subtracted from.
        // The objective is to reduce the price spread between mint price and redemption price,
        // and we choose that stablecoin which has the greatest spread.
        
        // if max price diff does not deviate much from average diff or all inflation-adjusted prices 
        // are less than the redemption prices, then reductions pertain to quote_token only.
        if (average_diff.abs() < min_diff) || (average_diff < 0.0) {
            // msg!("No significant price differences found");
            if price_differences[&quote_token.to_string()] >= 0.0 || *other_target == *quote_token {
                // msg!("If quote_token m price is larger than r price, then situation is normal.");
                // If the price difference is positive, it means that the mint price is higher than the redemption price;
                // in this case, we need to reduce IRMA in circulation by the irma_amount.
                // Note that this keeps price differences the same (it's minting that adjusts redemption price).
                let circulation: u128 = self.get_stablecoin(quote_token).unwrap().irma_in_circulation;
                require!(circulation >= irma_amount as u128, CustomError::InsufficientCirculation);
                let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
                mut_reserve.irma_in_circulation -= irma_amount as u128;
            } else {
                msg!("m price <= r price for quote token, adjust backing reserve only for {:?}.", quote_token);
                // If the price difference is negative, it means that the mint price is lower than the redemption price;
                // in this case, we need to set the redemption price eq to the mint price in order to preserve the backing.
                // We also do not reduce IRMA in circulation, which effectively means that we are still draining the reserve,
                // but not by much, while the reduction in the ratio of reserve to IRMA in circulation (normally the
                // redemption price) goes down faster than if we also reduced IRMA in circulation. 
                // And we're done!
            }
            // msg!("New reserve for {}: {}", quote_token.to_string(), *reserve);
            // let ro_circulation: u64 = reserves[quote_token].irma_in_circulation;
            // msg!("New circulation for {}: {}", quote_token.to_string(), ro_circulation);
            return Ok(());
        }
        // All the following code is for the semi-normal case, in which the mint price 
        // is higher than or equal to the redemption price; but the price differences
        // can be large.
        // msg!("Other target for normal adjustments: {}", other_target.to_string());

        let other_stablecoin = &self.get_stablecoin(other_target).unwrap();
        let other_circulation: u128 = other_stablecoin.irma_in_circulation;
        let other_price: f64 = other_stablecoin.mint_price;
        let other_reserve: u128 = other_stablecoin.backing_reserves;
        let ten_pow_decimals =  10.0_f64.powi(
            IRMA.backing_decimals as i32 - stablecoin.backing_decimals as i32
        );
        let other_red_price: f64 = ((other_reserve.checked_div(other_circulation).unwrap_or(0) as f64) 
            * ten_pow_decimals) as f64;

        let price: f64 = stablecoin.mint_price;
        let reserve: u128 = stablecoin.backing_reserves;
        let ro_circulation: u128 = stablecoin.irma_in_circulation;

        let other_price_diff: f64 = other_price - other_red_price as f64;
        let post_price_diff: f64 = price
            - ((reserve as i128 - (irma_amount as f64 / price) as i128) / ro_circulation as i128) as f64;
        let post_other_price_diff: f64 = other_price 
            - (other_reserve as i128 / (other_circulation as i128 - irma_amount as i128)) as f64;

        if other_price_diff < post_other_price_diff {
            // msg!("--> Other price diff is less than or equal to post other price diff, adjusting second circulation only.");
            // if irma_amount is such that it could not improve the redemption price when applied to other stabecoin reserve,
            // we can just subtract from the circulation (same as normal case).
            // Note that the normal case does not change redemtion prices.
            // let circulation: u128 = stablecoin.irma_in_circulation;
            // require!(irma_amount <= circulation, CustomError::InsufficientCirculation);
            let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.irma_in_circulation -= irma_amount as u128;
        } else
        if post_other_price_diff < post_price_diff {
            // msg!("--> Post other price diff is less than or equal to second price diff, 
            //         adjusting other circulation only.");
            // if irma_amount is such that it would reduce discrepancy for other stablecoin more post 
            // adjustment, we can choose to subtract irma_amount from the other_circulation only
            require!(irma_amount as u128 <= other_circulation, CustomError::InsufficientCirculation);
            let mut_reserve = self.get_mut_stablecoin(other_target).unwrap();
            mut_reserve.irma_in_circulation -= irma_amount as u128;
        } else {
            // if irma amount is such that it doesn't improve the redemption price for either stablecoin,
            // we can do a linear adjustment of both other and second circulations.
            // msg!("--> First and second prices are close enough, adjusting both circulations linearly.");
            // Do simple linear adjustment of both other and second circulations
            let adjustment_amount: f64 = irma_amount as f64 * (other_price_diff - post_price_diff) / (other_price_diff + post_price_diff);
            // msg!("Adjustment amount: {}", adjustment_amount);
            require!(adjustment_amount > 0.0, CustomError::InvalidAmount);
            require!(adjustment_amount <= irma_amount as f64, CustomError::InvalidAmount);
            // msg!("Adjusting other circulation by {} and second circulation by {}", adjustment_amount.ceil(), irma_amount as f64 - adjustment_amount.ceil());
            let mut_reserve = self.get_mut_stablecoin(other_target).unwrap();
            mut_reserve.irma_in_circulation -= adjustment_amount.ceil() as u128;
            let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.irma_in_circulation = (ro_circulation as i128 - (irma_amount as i128 - adjustment_amount.ceil() as i128)) as u128;
        }

        return Ok(());
    }
}

