// In programs/irma/src/lib.rs
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use std::mem::size_of;
use std::str::FromStr;

// Import the state structs from your modules, as they are used in the account definitions.
use pricing::{StateMap, StableState};

// declare_program!(dlmm);
// use commons::dlmm::borsh::*;

// Declare your program's ID
declare_id!("BqTQKeWmJ4btn3teLsvXTk84gpWUu5CMyGCmncptWfda");

use anchor_lang::context::Context;

use commons::dlmm::types::Bin;
use commons::dlmm::accounts::*;

pub const IRMA_ID: Pubkey = crate::ID;

#[derive(Clone, Debug, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub enum MarketMakingMode {
    ModeRight,
    ModeLeft,
    ModeBoth,
    ModeView,
}

impl Default for MarketMakingMode {
    fn default() -> Self {
        MarketMakingMode::ModeView
    }
}

impl FromStr for MarketMakingMode {
    type Err = anchor_lang::error::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "moderight" => Ok(MarketMakingMode::ModeRight),
            "modeleft" => Ok(MarketMakingMode::ModeLeft),
            "modeboth" => Ok(MarketMakingMode::ModeBoth),
            "modeview" => Ok(MarketMakingMode::ModeView),
            _ => Ok(MarketMakingMode::default()),
        }
    }
}

// ====================================================================
// START: DEFINE ALL INSTRUCTION ACCOUNT STRUCTS HERE
// ====================================================================

#[derive(Accounts)]
pub struct Init<'info> {
    // Note: We need to qualify MAX_BACKING_COUNT with its module
    #[account(init, space=32 + 8 + size_of::<StableState>()*pricing::MAX_BACKING_COUNT, payer=irma_admin, seeds=[b"state".as_ref()], bump)]
    pub state: Account<'info, StateMap>,
    #[account(mut)]
    pub irma_admin: Signer<'info>,
    #[account(mut)]
    pub core: Account<'info, Core>,
    pub system_program: Program<'info, System>,
    // pub bumps: InitBumps,
}

#[derive(Accounts)]
pub struct Maint<'info> {
    #[account(mut)]
    pub state: Account<'info, StateMap>,
    pub irma_admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    // pub bumps: MaintBumps,
}


// Declare your modules
pub mod pair_config;
// pub mod bin_array;
pub mod bin_array_manager;
pub mod meteora_integration;
pub mod pricing;
pub mod position_manager;
pub mod utils;

// ====================================================================
// START: DEFINE ALL CPI API
// ====================================================================
use crate::meteora_integration::Core;

#[program]
pub mod irma {
    use super::*; // This will now correctly bring Init, Maint, etc. into scope

    pub fn initialize(
        mut ctx: Context<Init>,
        owner: String,
        config_keys: Vec<String>
    ) -> Result<()> {
        let owner_pk = Pubkey::from_str(&owner).unwrap(); // map_err(|_| Error::InvalidPubkey)?;
        let config_pks: Vec<Pubkey> = config_keys.iter()
            .map(|key| Pubkey::from_str(key).unwrap()) // map_err(|_| Error::InvalidPubkey)
            .collect();

        // Initialize the pricing system first
        pricing::init_pricing(&mut ctx)?;
        
        // TODO: Initialize Core separately if needed
        // For now, just do basic initialization
        msg!("IRMA protocol initialized with owner: {}", owner_pk);
        msg!("Config keys count: {}", config_pks.len());

        let core = Core::create_core(owner_pk, config_pks)?;
        ctx.accounts.core.set_inner(core);
        Ok(())
    }

    pub fn add_reserve(ctx: Context<Maint>, symbol: String, mint_address: Pubkey, decimals: u8) -> Result<()> {
        msg!("Add stablecoin entry, size of StateMap: {}", size_of::<StateMap>());
        pricing::add_reserve(ctx, &symbol, mint_address, decimals)
    }

    pub fn remove_reserve(ctx: Context<Maint>, symbol: String) -> Result<()> {
        pricing::remove_reserve(ctx, &symbol)
    }

    pub fn disable_reserve(ctx: Context<Maint>, symbol: String) -> Result<()> {
        pricing::disable_reserve(ctx, &symbol)
    }

    pub fn get_redemption_price(ctx: Context<Maint>, quote_token: String) -> Result<f64> {
        pricing::get_redemption_price(ctx, &quote_token)
    }

    pub fn get_prices(ctx: Context<Maint>, quote_token: String) -> Result<(f64, f64)> {
        pricing::get_prices(ctx, &quote_token)
    }

    pub fn set_mint_price(ctx: Context<Maint>, quote_token: String, new_price: f64) -> Result<()> {
        pricing::set_mint_price(ctx, &quote_token, new_price)
    }

    // NOTE: In the two functions below, the Common accounts struct previously allowed the trader herself
    // to access IRMA. However, now we are changing it so that only the irma_admin (the program
    // maintainer) can call these functions to inform the pricing module of trade events. In other words,
    // the trader should be set to irma_admin in the Common context when calling these functions.
    // To avoid confusion, I have renamed the 'trader' field in Common to 'irma_admin' and replaced
    // all instances of 'Common' with "Maint".

    /// Let pricing know about a sale trade event
    /// Note that IRMA is what we are selling (minting).
    pub fn sale_trade_event(ctx: Context<Maint>, bought_token: String, bought_amount: u64) -> Result<()> {
        return pricing::mint_irma(ctx, &bought_token, bought_amount);
    }

    /// Let pricing know about a buy-back trade event
    /// Note that IRMA is what we are buying (burning).
    pub fn buy_trade_event(ctx: Context<Maint>, sold_token: String, bought_amount: u64) -> Result<()> {
        return pricing::redeem_irma(ctx, &sold_token, bought_amount);
    }
}
