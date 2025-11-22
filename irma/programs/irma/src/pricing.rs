#![allow(unexpected_cfgs)]
// #![feature(trivial_bounds)]
// #[cfg(feature = "idl-build")]
use std::string::String;
// se std::vec::Vec;
use std::option::Option;
// use anchor_lang_idl_spec::IdlType::Option as IdlOption;
// use anchor_lang_idl_spec::IdlType::Pubkey as IdlPubkey;
use crate::{Init, Maint};
use anchor_lang::*;
// use anchor_lang::system_program::ID;
use anchor_lang::prelude::*;
use Vec;
use std::collections::BTreeMap;
use static_assertions::const_assert_eq;
use core::mem::size_of;
// use anchor_lang::solana_program::pubkey;


// The number of stablecoins that are initially supported by the IRMA program.
pub const BACKING_COUNT: usize = 6 as usize;
// Maximum number of stablecoins supported
// This is limited by the maximum size of the account data (10,240 bytes).
// Each stablecoin entry in the reserves requires approximately 120 bytes of storage.
// Therefore, the maximum number of stablecoins is calculated as 10,240 / 120 = 85 (rounded down).
pub const MAX_BACKING_COUNT: usize = 10; // 85;

// declare_id!("8zs1JbqxqLcCXzBrkMCXyY2wgSW8uk8nxYuMFEfUMQa6");

/// IRMA module
/// FIXME: the decimals are all assumed to be zero, which is not true for all stablecoins.

pub fn init_pricing(ctx: Context<Init>) -> Result<()> {
    msg!("Greetings from: {:?}", ctx.program_id);
    let state = &ctx.accounts.state;
    if state.reserves.len() > 0 {
        msg!("State already initialized, skipping init...");
        return Ok(());
    }
    *ctx.accounts.state = StateMap::new();
    let state = &mut ctx.accounts.state;
    state.bump = 13u8; // InitializeBumps::bump(&ctx.bumps).unwrap_or(0);
    msg!("State initialized with bump: {}", state.bump);

    state.init_reserves()?;
    msg!("Initial stablecoins added to the state.");

    Ok(())
}

/// The whole purpose for using a BTreeMap (now a Vec) is to allow for easy addition of new stablecoins.
pub fn add_reserve(
        ctx: Context<Maint>, 
        symbol: &str, 
        mint_address: prelude::Pubkey,
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
    require!(stablecoin.irma_in_circulation > 0u64, CustomError::InsufficientCirculation);
    Ok(())
}

/// Set mint price for a given quote token based on inflation data.
/// This should be called for every backing stablecoin supported, only once per day
/// because Truflation updates the inflation data only once per day.
pub fn set_mint_price(ctx: Context<Maint>, quote_token: &str, mint_price: f64) -> Result<()> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;
    require!(mint_price > 0.0, CustomError::InvalidAmount);
    require!(mint_price < 100.0, CustomError::InvalidAmount); // sanity check, mint price should not be too high
    let stablecoin = state_map.get_mut_stablecoin(quote_token).unwrap();
    stablecoin.mint_price = mint_price;
    Ok(())
}

/// Mint IRMA tokens for a given amount of quote token.
/// Input amount is  in quote token's smallest unit (e.g. 1 USDT = 10^6, 1 USDC = 10^6, etc.)
/// The mint price is the price of IRMA in terms of the quote token, which is set by the Truflation oracle.
pub fn mint_irma(ctx: Context<Maint>, quote_token: &str, amount: u64) -> Result<()> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;

    if amount == 0 { return Ok(()); };

    let stablecoin = state_map.get_stablecoin(quote_token).unwrap();
    let curr_price: f64 = stablecoin.mint_price;
    let amount = (amount as f64 / (10.0_f64).powf(stablecoin.backing_decimals as f64)) as f64;

    let stablecoin = state_map.get_mut_stablecoin(quote_token).unwrap();
    stablecoin.backing_reserves += amount.ceil() as u64; // backing should not have a fractional part
    stablecoin.irma_in_circulation += (amount / curr_price).ceil() as u64;

    Ok(())
}

/// RedeemIRMA - user surrenders IRMA in irma_amount, expecting to get back quote_token according to redemption price.
/// FIXME: If resulting redemption price increases by more than 0.0000001, then actual redemption price 
/// should be updated immediately.
pub fn redeem_irma(ctx: Context<Maint>, quote_token: &str, irma_amount: u64) -> Result<()> {
    let state_map = &mut ctx.accounts.state;
    validate_params(&(*state_map), quote_token)?;

    if irma_amount == 0 { return Ok(()) };

    let state = state_map.get_stablecoin(quote_token).unwrap();
    // There is a redemption rule: every redemption is limited to 100k IRMA or 10% of the IRMA in circulation (for
    // the quote token) whichever is smaller.
    let circulation: u64 = state.irma_in_circulation;
    // let circulation: u64 = state.irma_in_circulation * (10u64.pow(state.backing_decimals as u32));
    // require!(circulation > 0, CustomError::InsufficientCirculation);
    let irma_amount = (irma_amount as f64 / (10.0_f64).powf(IRMA.backing_decimals as f64)) as f64;
    require!((irma_amount <= 100_000.0) && (irma_amount <= circulation as f64 / 10.0), CustomError::InvalidIrmaAmount);

    ctx.accounts.state.distribute(quote_token, irma_amount.ceil() as u64)?;

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
    let backing_reserves = stablecoin.backing_reserves as f64;
    let irma_in_circulation = stablecoin.irma_in_circulation as f64;
    
    if irma_in_circulation == 0.0 {
        return Ok(1.0); // Default to 1.0 if no IRMA in circulation
    }
    
    let redemption_price = backing_reserves / irma_in_circulation;
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

// #[account]
// #[derive(InitSpace)]
// #[derive(Debug)]
// pub struct State {
//     #[max_len(BACKING_COUNT)]
//     pub mint_price: Vec<f64>,
//     #[max_len(BACKING_COUNT)]
//     pub backing_reserves: Vec<u64>,
//     #[max_len(BACKING_COUNT)]
//     pub backing_decimals: Vec<u8>,
//     #[max_len(BACKING_COUNT)]
//     pub irma_in_circulation: Vec<u64>,
//     pub bump: u8,
// }

/// Alternative implementation that allows for easy addition of new stablecoins
/// Each stablecoin struct uses 80 bytes.
// #[account]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Debug)]
pub struct StableState {
    pub symbol: String, // symbol of the stablecoin, e.g. "USDT"
    pub mint_address: Pubkey, // mint address of the stablecoin
    pub backing_decimals: u64, // need only u8, but for alignment reasons we use u64
    pub mint_price: f64, // mint price of IRMA in terms of the backing stablecoin
    pub backing_reserves: u64, // backing reserves is in whole numbers (no decimals)
    pub irma_in_circulation: u64, // in whole numbers (no decimals)
    pub active: bool, // whether the stablecoin is active or not
    pub extra: [u8; 7], // padding to make the size of the struct 25 * EnumCount + 8
}

const_assert_eq!(
    size_of::<StableState>(), 96
);

#[account]
#[derive(PartialEq, Debug)]
pub struct StateMap {
    pub reserves: Vec<StableState>,
    pub bump: u8, // Bump seed for PDA
    pub padding: [u8; 7], // padding to make the size of the struct 25 * EnumCount + 8
}

/// Immutable data for IRMA itself.
pub const IRMA: StableState = StableState {
    symbol: String::new(), // "IRMA".to_string(), <-- can't use to_string() in a const?
    mint_address: pubkey!("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1"), // IRMA mint address on Solana
    backing_decimals: 6,
    mint_price: 1.0,
    backing_reserves: 0u64,
    irma_in_circulation: 1u64,
    active: false, // IRMA cannot be a reserve backing of itself
    extra: [0; 7], // padding
};
/*
#[derive(Accounts)]
pub struct Init<'info> {
        #[account(init, space=32 + 8 + size_of::<StableState>()*MAX_BACKING_COUNT, payer=irma_admin, seeds=[b"state".as_ref()], bump)]
    pub state: Account<'info, StateMap>,
    #[account(mut)]
    pub irma_admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Common<'info> {
    #[account(mut)]
    pub state: Account<'info, StateMap>,
    pub trader: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Maint<'info> {
    #[account(mut)]
    pub state: Account<'info, StateMap>,
    pub irma_admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}
*/
impl StableState {

    pub fn new(symbol: &str, mint_address: prelude::Pubkey, backing_decimals: u64) -> Result<Self> {
        // msg!("StableState size: {}", size_of::<StableState>());
        // const len: usize = symbol.to_bytes().len();
        require!(symbol.len() <= 8 && symbol.len() > 0, CustomError::InvalidBackingSymbol);
        require!(mint_address != prelude::Pubkey::default(), CustomError::InvalidBackingAddress);
        require!(backing_decimals > 0, CustomError::InvalidBacking);
        Ok(StableState {
            symbol: symbol.to_string(), // symbol of the stablecoin, e.g. "USDT"
            mint_address,
            backing_decimals,
            mint_price: 1.0, // default mint price is 1.0
            backing_reserves: 0,
            irma_in_circulation: 1,
            active: true,
            extra: [0; 7], // for future use
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

    pub fn get_stablecoin_symbol(&self, mint_address: prelude::Pubkey) -> Option<String> {
        for stablecoin in &self.reserves {
            if stablecoin.mint_address == mint_address {
                return Some(stablecoin.symbol.clone());
            }
        }
        None
    }

    pub fn remove_reserve(&mut self, symbol: &str) -> Option<StableState> {
        if !self.contains_reserve(symbol) {
            msg!("MapTrait {} not found in reserves.", symbol);
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

    pub fn init_reserves(&mut self) -> Result<()> {
        // This function is used to add initial stablecoins to the reserves.
        // It is called during the initialization of the IRMA program.

        let usdt = StableState::new(
            "USDT",
            pubkey!("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p"), // USDT mint address on Solana
            6,
        )?;
        self.add_reserve(usdt);

        //     symbol: Box::new(symbols[1].to_string()), // symbol of the stablecoin, e.g. "USDC"
        //     mint_address: pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"), // USDC mint address on Solana
        let usdc = StableState::new(
            "USDC",
            pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"), // USDC mint address on Solana
            6,
        )?;
        self.add_reserve(usdc);

        //     symbol: Box::new(symbols[2].to_string()),
        //     mint_address: pubkey!("2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo"), // PYUSD mint address on Solana
        let pyusd = StableState::new(
            "PYUSD",
            pubkey!("2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo"), // PYUSD mint address on Solana
            6,
        )?;
        self.add_reserve(pyusd);

        //     symbol: Box::new(symbols[3].to_string()),
        //     mint_address: pubkey!("USDSwr9ApdHk5bvJKMjzff41FfuX8bSxdKcR81vTwcA"), // USDS mint address on Solana
        let usds = StableState::new(
            "USDS",
            pubkey!("USDSwr9ApdHk5bvJKMjzff41FfuX8bSxdKcR81vTwcA"), // USDS mint address on Solana
            6,
        )?;
        self.add_reserve(usds);

        //     symbol: Box::new(symbols[4].to_string()),
        //     mint_address: pubkey!("2u1tszSeqZ3qBWF3uNGPFc8TzMk2tdiwknnRMWGWjGWH"), // USDG mint address on Solana
        let usdg = StableState::new(
            "USDG",
            pubkey!("2u1tszSeqZ3qBWF3uNGPFc8TzMk2tdiwknnRMWGWjGWH"), // USDG mint address on Solana
            6,
        )?;
        self.add_reserve(usdg);

        //     symbol: Box::new(symbols[5].to_string()),
        //     mint_address: pubkey!("9zNQRsGLjNKwCUU5Gq5LR8beUCPzQMVMqKAi3SSZh54u"), // FDUSD mint address on Solana
        let fdusd = StableState::new(
            "FDUSD",
            pubkey!("9zNQRsGLjNKwCUU5Gq5LR8beUCPzQMVMqKAi3SSZh54u"), // FDUSD mint address on Solana
            6,
        )?;
        self.add_reserve(fdusd);

        msg!("Initialized reserves {}", self.reserves.iter().map(|e| e.symbol.clone()).collect::<Vec<_>>().join(", "));
        self.reserves.sort_by_key(|e| e.symbol.clone()); // Sort reserves by symbol
        msg!("sorted reserves {}", self.reserves.iter().map(|e| e.symbol.clone()).collect::<Vec<_>>().join(", "));
        msg!("Vec length: {:?}", self.reserves.len());
        Ok(())
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
    fn distribute(&mut self, quote_token: &str, irma_amount: u64) -> Result<()> {

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
                let redemption_price = reserve.backing_reserves as f64 / circulation as f64;
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

        let ro_circulation: u64 = self.get_stablecoin(quote_token).unwrap().irma_in_circulation;
        let reserve: u64 = self.get_stablecoin(quote_token).unwrap().backing_reserves;
        let redemption_price: f64 = reserve as f64 / ro_circulation as f64;
        let subject_adjustment: u64 = (irma_amount as f64 * redemption_price).ceil() as u64; // irma_amount is in whole numbers, so we can use it directly

        // no matter what, we need to reduce the subject reserve (quote_token)
        require!(reserve >= subject_adjustment, CustomError::InsufficientReserve);
        let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
        mut_reserve.backing_reserves -= subject_adjustment;

        // if max price diff does not deviate much from average diff or all inflation-adjusted prices 
        // are less than the redemption prices, then reductions pertain to quote_token only.
        if (average_diff.abs() < min_diff) || (average_diff < 0.0) {
            // msg!("No significant price differences found");
            if price_differences[&quote_token.to_string()] >= 0.0 || *other_target == *quote_token {
                // msg!("If quote_token m price is larger than r price, then situation is normal.");
                // If the price difference is positive, it means that the mint price is higher than the redemption price;
                // in this case, we need to reduce IRMA in circulation by the irma_amount.
                // Note that this keeps price differences the same (it's minting that adjusts redemption price).
                let circulation: u64 = self.get_stablecoin(quote_token).unwrap().irma_in_circulation;
                require!(circulation >= irma_amount, CustomError::InsufficientCirculation);
                let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
                mut_reserve.irma_in_circulation -= irma_amount;
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

        let other_circulation: u64 = self.get_stablecoin(other_target).unwrap().irma_in_circulation;

        // if we don't have enough reserve to redeem the irma_amount, just error out;
        // we can't allow redemption from a reserve that is smaller than the irma_amount.
        // require!(irma_amount <= *circulation, CustomError::InsufficientCirculation);

        let other_price: f64 = self.get_stablecoin(other_target).unwrap().mint_price;
        let price: f64 = self.get_stablecoin(quote_token).unwrap().mint_price;
        let other_reserve: u64 = self.get_stablecoin(other_target).unwrap().backing_reserves;
        let reserve: u64 = self.get_stablecoin(quote_token).unwrap().backing_reserves;

        let other_price_diff: f64 = other_price - (other_reserve / other_circulation) as f64;
        let ro_circulation: u64 = self.get_stablecoin(quote_token).unwrap().irma_in_circulation;
        let post_price_diff: f64 = price - (reserve as f64 - irma_amount as f64 / price) / ro_circulation as f64;
        let post_other_price_diff: f64 = other_price - (other_reserve as f64 / (other_circulation - irma_amount) as f64);

        if other_price_diff <= post_other_price_diff {
            // msg!("--> Other price diff is less than or equal to post other price diff, adjusting second circulation only.");
            // if irma_amount is such that it could not improve the redemption price when applied to other stabecoin reserve,
            // we can just subtract from the circulation (same as normal case).
            // Note that the normal case does not change redemtion prices.
            let circulation: u64 = self.get_stablecoin(quote_token).unwrap().irma_in_circulation;
            require!(irma_amount <= circulation, CustomError::InsufficientCirculation);
            let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.irma_in_circulation -= irma_amount;
        } else
        if post_other_price_diff <= post_price_diff {
            // msg!("--> Post other price diff is less than or equal to second price diff, 
            //         adjusting other circulation only.");
            // if irma_amount is such that it would reduce discrepancy for other stablecoin more post 
            // adjustment, we can choose to subtract irma_amount from the other_circulation only
            require!(irma_amount <= other_circulation, CustomError::InsufficientCirculation);
            let mut_reserve = self.get_mut_stablecoin(other_target).unwrap();
            mut_reserve.irma_in_circulation -= irma_amount;
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
            mut_reserve.irma_in_circulation -= adjustment_amount.ceil() as u64;
            let mut_reserve = self.get_mut_stablecoin(quote_token).unwrap();
            mut_reserve.irma_in_circulation -= irma_amount - adjustment_amount.ceil() as u64;
        }

        return Ok(());
    }
}

#[error_code]
pub enum CustomError {
    #[msg("Invalid amount provided.")]
    InvalidAmount,
    #[msg("Mint price not set.")]
    MintPriceNotSet,
    #[msg("Invalid quote token.")]
    InvalidQuoteToken,
    #[msg("Insufficient circulation.")]
    InsufficientCirculation,
    #[msg("Insufficient reserve.")]
    InsufficientReserve,
    #[msg("Invalid reserve value.")]
    InvalidBacking,
    #[msg("Invalid IRMA amount.")]
    InvalidIrmaAmount,
    #[msg("No reserve list.")]
    InvalidReserveList,
    #[msg("Invalid backing symbol.")]
    InvalidBackingSymbol,
    #[msg("Invalid backing address.")]
    InvalidBackingAddress,
    #[msg("Symbol not found.")]
    SymbolNotFound,
}

