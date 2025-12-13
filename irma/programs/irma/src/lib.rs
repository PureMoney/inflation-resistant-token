// In programs/irma/src/lib.rs
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use std::mem::size_of;
use std::str::FromStr;
// use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
// use anchor_spl::token_2022::ID as TOKEN_2022_PROGRAM_ID;

// Module declarations
pub mod errors;
pub mod pricing;
pub mod position_manager;
pub mod meteora_integration;
pub mod pair_config;
pub mod bin_array_manager;
pub mod utils;

// Import the state structs from your modules, as they are used in the account definitions.
pub use pricing::{StateMap, StableState};
use errors::CustomError;

// declare_program!(dlmm);
// use commons::dlmm::borsh::*;

// Declare your program's ID
// declare_id!("BqTQKeWmJ4btn3teLsvXTk84gpWUu5CMyGCmncptWfda");
declare_id!("E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs");

// use anchor_lang::context::Context;

use commons::dlmm::accounts::*;
use commons::fetch_lb_pair_state;

// Re-export types for IDL generation
pub use position_manager::{AllPosition, SinglePosition, MintInfo, MintWithProgramId, TokenEntry};
pub use meteora_integration::Core;
pub use pair_config::*;

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
    #[account(
        init,
        space=32 + 8 + size_of::<StableState>()*pricing::MAX_BACKING_COUNT,
        payer=irma_admin, seeds=[b"state_v5".as_ref()],
        bump
    )]
    pub state: Account<'info, StateMap>,
    #[account(mut)]
    pub irma_admin: Signer<'info>,
    // Space calculation for Core:
    // 8 bytes for discriminator
    // 32 bytes for owner (Pubkey)
    // 4 bytes for config Vec length + (max 10 configs * ~200 bytes each) = 2004 bytes
    // AllPosition struct:
    //   - 4 bytes for all_positions Vec length + (max 10 positions * ~300 bytes each) = 3004 bytes  
    //   - 4 bytes for tokens Vec length + (max 20 tokens * ~200 bytes each) = 4004 bytes
    // Total: 8 + 32 + 2004 + 3004 + 4004 + buffer = ~10000 bytes
    #[account(
        init,
        space=8 + 10000,
        payer=irma_admin,
        seeds=[b"core_v5".as_ref()],
        bump
    )]
    pub core: Account<'info, Core>,
    pub system_program: Program<'info, System>,
    // pub bumps: InitBumps,
}

#[derive(Accounts)]
pub struct Maint<'info> {
    #[account(mut, seeds=[b"state_v5".as_ref()], bump)]
    pub state: Account<'info, StateMap>,
    pub irma_admin: Signer<'info>,
    #[account(mut, seeds=[b"core_v5".as_ref()], bump)]
    pub core: Account<'info, Core>,
    pub system_program: Program<'info, System>,
    // pub bumps: MaintBumps,
}

/// Context to force Core and related types into IDL
#[derive(Accounts)]
pub struct GetCoreData<'info> {
    #[account(mut)]
    pub core: Account<'info, Core>,
    pub signer: Signer<'info>,
}


// Declare your modules (NOTE: already declared above)
// pub mod pair_config;
// pub mod bin_array_manager;
// pub mod meteora_integration;
// pub mod pricing;
// pub mod position_manager;
// pub mod utils;

// ====================================================================
// START: DEFINE ALL CPI API
// ====================================================================
// use crate::meteora_integration::Core;

#[program]
pub mod irma {
    use super::*; // This will now correctly bring Init, Maint, etc. into scope

    /// Initialize the IRMA protocol
    /// The context accounts will be initialized by pricing to conttain reserves in alphabetical order.
    /// The remaining_accounts should contain the LbPair accounts in the same order as the reserves.
    pub fn initialize(
        mut ctx: Context<Init>,
        owner: String,
        config_keys: Vec<String>
    ) -> Result<()> {
        let owner_pk = Pubkey::from_str(&owner).unwrap();

        assert_eq!(config_keys.len() == 0, true);

        let config_pks: Vec<Pubkey> = config_keys.iter()
            .map(|key| Pubkey::from_str(key).unwrap())
            .collect();

        // Initialize the pricing system first
        pricing::init_pricing(&mut ctx)?;

        // TODO: Initialize Core separately if needed
        // For now, just do basic initialization
        msg!("IRMA protocol initialized with owner: {}", owner_pk);
        msg!("Config keys count: {}", config_pks.len());

        assert_eq!(config_pks.len() == 0, true);

        let core = Core::create_core(owner_pk, config_pks)?;
        ctx.accounts.core.set_inner(core);
        Ok(())
    }

    pub fn add_reserve(
        ctx: Context<Maint>,
        symbol: String,
        mint_address: Pubkey,
        decimals: u8
    ) -> Result<()> {
        msg!("Add stablecoin entry, size of StateMap: {}", ctx.accounts.state.reserves.len());
        pricing::add_reserve(ctx, &symbol, mint_address, decimals)
    }

    pub fn remove_reserve(
        ctx: Context<Maint>,
        symbol: String
    ) -> Result<()> {
        pricing::remove_reserve(ctx, &symbol)
    }

    pub fn disable_reserve(
        ctx: Context<Maint>,
        symbol: String
    ) -> Result<()> {
        pricing::disable_reserve(ctx, &symbol)
    }

    /// This connects a reserve stablecoin to its corresponding LBPair.
    /// There can only be a single LbPair per stablecoin reserve.
    pub fn update_reserve_lbpair(
        ctx: Context<Maint>, symbol: String, lb_pair: String
    ) -> Result<()> {
        let lb_pair_key: Pubkey = Pubkey::from_str(&lb_pair).unwrap();
        let remaining_accounts: &[AccountInfo<'_>] = &ctx.remaining_accounts;
        {
            let stablecoin = ctx.accounts.state.reserves.iter().find(|r| r.symbol == symbol)
                .ok_or(error!(CustomError::ReserveNotFound))?;
            let lb_pair_state = fetch_lb_pair_state(
                remaining_accounts,
                &lb_pair_key,
            )?;
            // check that the input LbPair is valid and matches the reserve stablecoin mint
            if !lb_pair_state.token_y_mint.eq(&stablecoin.mint_address) {
                return Err(error!(CustomError::InvalidLbPairState));
            }
        }
        // add the LbPair to the core config if not already present
        // let remaining_accounts = &ctx.remaining_accounts;
        let core = &mut ctx.accounts.core;
        if !core.config.iter().any(|pairc: &PairConfig| pairc.pair_address == lb_pair) {
            core.config.push(PairConfig {
                pair_address: lb_pair.clone(),
                x_amount: 0,
                y_amount: 0,
                mode: MarketMakingMode::ModeBoth,
            });
            core.position_data.all_positions.push(
                position_manager::SinglePosition::new(lb_pair_key.clone())
            );
            let _ = core.fetch_token_info(remaining_accounts)?;
            // remove extraneous LbPair configs if any
            let reserves = &ctx.accounts.state.reserves;
            for i in (0..core.config.len()).rev() {
                let pair_config = &core.config[i];
                if !reserves.iter().any(|r| r.pool_id.to_string() == pair_config.pair_address) {
                    core.config.remove(i);
                }
            }
        }
        // finally, update the pool_id for the given stablecoin symbol
        let reserves = &mut ctx.accounts.state.reserves;
        let stablecoin_mut = &mut reserves.iter_mut().find(|r| r.symbol == symbol)
            .ok_or(error!(CustomError::ReserveNotFound))?;
        stablecoin_mut.pool_id = lb_pair_key.clone();
        Ok(())
    }

    pub fn list_reserves(ctx: Context<Maint>) -> Result<String> {
        Ok(pricing::list_reserves(ctx))
    }

    pub fn get_redemption_price(ctx: Context<Maint>, quote_token: String) -> Result<f64> {
        pricing::get_redemption_price(&ctx.accounts.state.reserves, &quote_token)
    }
    
    pub fn get_prices(ctx: Context<Maint>, quote_token: String) -> Result<(f64, f64)> {
        pricing::get_prices(&ctx.accounts.state.reserves, &quote_token)
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
    /// bought_amount is the amount of bought_token we received from the sale.
    pub fn sale_trade_event(
        ctx: Context<Maint>, bought_token: String, bought_amount: u64
    ) -> Result<()> {
        // Extract references to avoid double mutable borrow
        let core = &mut ctx.accounts.core;
        let state = &mut ctx.accounts.state;
        let remaining_accounts = ctx.remaining_accounts;

        core.refresh_position_data_with_accounts(state, &remaining_accounts, bought_token, bought_amount, true)
    }

    /// Let pricing know about a buy-back trade event
    /// Note that IRMA is what we are buying back (burning) and we just sold the backing token.
    pub fn buy_trade_event(
        ctx: Context<Maint>, sold_token: String, irma_amount: u64
    ) -> Result<()> {
        // Extract references to avoid double mutable borrow
        let core = &mut ctx.accounts.core;
        let state = &mut ctx.accounts.state;
        let remaining_accounts = ctx.remaining_accounts;

        core.refresh_position_data_with_accounts(state, &remaining_accounts, sold_token, irma_amount, false)
    }

    /// Check all LB pair positions and update from pricing.rs/
    /// This is used to periodically sync all positions.
    pub fn check_shift_price_ranges(
        ctx: Context<Maint>
    ) -> Result<()> {
        // Extract references to avoid double mutable borrow
        let corei = &mut ctx.accounts.core.clone();
        let core = &mut ctx.accounts.core;
        let payer = &mut ctx.accounts.irma_admin;
        let reserves = &mut ctx.accounts.state.reserves;
        let remaining_accounts: &[AccountInfo<'_>] = &ctx.remaining_accounts;

        for position in corei.position_data.all_positions.iter_mut() {
            Core::check_shift_price_range(
                core,
                payer,
                remaining_accounts,
                reserves,
                position,
            )?;
        }
        Ok(())
    }

    /// Helper instruction to ensure Core type is included in IDL
    /// Returns the Core account data for debugging
    // pub fn get_core_data<'a>(ctx: Context<'a, Core>) -> Result<Account<'a, Core>> {
    //     // Simple read operation to include Core type in IDL
    //     let core = &ctx.accounts.core;
    //     Ok(Account::from(*core))
    // }

    /// Helper instruction to force AllPosition type into IDL
    pub fn get_position_info(
        _ctx: Context<Maint>
    ) -> Result<position_manager::AllPosition> {
        // This forces AllPosition to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force SinglePosition type into IDL
    pub fn get_single_position(
        _ctx: Context<Maint>
    ) -> Result<position_manager::SinglePosition> {
        // This forces SinglePosition to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force MintInfo type into IDL
    pub fn get_mint_info(
        _ctx: Context<Maint>
    ) -> Result<position_manager::MintInfo> {
        // This forces MintInfo to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force MintWithProgramId type into IDL
    pub fn get_mint_with_program_id(
        _ctx: Context<Maint>
    ) -> Result<position_manager::MintWithProgramId> {
        // This forces MintWithProgramId to be included in IDL as a return type  
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force TokenEntry type into IDL
    pub fn get_token_entry(
        _ctx: Context<Maint>
    ) -> Result<position_manager::TokenEntry> {
        // This forces TokenEntry to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }
}
