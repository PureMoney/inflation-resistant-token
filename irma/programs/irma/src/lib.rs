// In programs/irma/src/lib.rs
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use std::mem::size_of;
use std::str::FromStr;

// use crate::borsh::{BorshSerialize, BorshDeserialize};

// Import the state structs from your modules, as they are used in the account definitions.
use pricing::{StateMap, StableState};

// declare_program!(dlmm);
// use commons::dlmm::borsh::*;

// Declare your program's ID
declare_id!("BqTQKeWmJ4btn3teLsvXTk84gpWUu5CMyGCmncptWfda");

use anchor_lang::AccountDeserialize;
use anchor_lang::AnchorDeserialize;

use commons::dlmm::types::Bin;
use crate::error::Error;
use commons::dlmm::accounts::*;

// impl Zeroable {
//     fn zeroed() -> Self {
//         Self {
//             liquidity: 0,
//             fee_growth_inside_x: 0,
//             fee_growth_inside_y: 0,
//             reward_growth_inside_x: 0,
//             reward_growth_inside_y: 0,
//             reward_owed_x: 0,
//             reward_owed_y: 0,
//         }
//     }
// }

// impl Pod for Bin {}

#[derive(Clone, Debug, PartialEq)]
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

/*
#[derive(Accounts)]
pub struct CreateOrcaPool<'info> {
    #[account(init, payer = admin, space = 8 + 256)]
    pub pool_state: Account<'info, OrcaPoolState>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdatePoolState<'info> {
    #[account(mut)]
    pub pool_state: Account<'info, OrcaPoolState>,
    #[account(mut)]
    pub updater: Signer<'info>,
}

#[derive(Accounts)]
pub struct GetPoolInfo<'info> {
    pub pool_state: Account<'info, OrcaPoolState>,
}

#[derive(Accounts)]
pub struct SimulateSwap<'info> {
    pub pool_state: Account<'info, OrcaPoolState>,
    #[account(mut)]
    pub trader: Signer<'info>,
}
*/

// ====================================================================
// Token Operations Contexts
// ====================================================================

// #[derive(Accounts)]
// pub struct MintIrma<'info> {
//     #[account(
//         seeds = [b"protocol_state"],
//         bump = protocol_state.bump,
//     )]
//     pub protocol_state: Account<'info, ProtocolState>,
    
//     /// CHECK: Verified as SPL Token Mint via constraint
//     #[account(
//         mut,
//         constraint = irma_mint.key() == protocol_state.token_a_mint,
//     )]
//     pub irma_mint: UncheckedAccount<'info>,
    
//     /// CHECK: Verified as SPL Token Mint via constraint
//     #[account(
//         constraint = usdc_mint.key() == protocol_state.token_b_mint,
//     )]
//     pub usdc_mint: UncheckedAccount<'info>,
    
//     /// CHECK: User's USDC token account
//     #[account(mut)]
//     pub user_usdc: UncheckedAccount<'info>,
    
//     /// CHECK: User's IRMA token account
//     #[account(mut)]
//     pub user_irma: UncheckedAccount<'info>,
    
//     /// CHECK: Protocol's USDC vault
//     #[account(mut)]
//     pub protocol_usdc_vault: UncheckedAccount<'info>,
    
//     #[account(mut)]
//     pub user: Signer<'info>,
    
//     /// CHECK: This is a PDA used as mint authority
//     #[account(
//         seeds = [b"mint_authority"],
//         bump,
//     )]
//     pub mint_authority: UncheckedAccount<'info>,
    
//     /// CHECK: SPL Token program
//     pub token_program: UncheckedAccount<'info>,
// }

// #[derive(Accounts)]
// pub struct RedeemIrma<'info> {
//     #[account(
//         seeds = [b"protocol_state"],
//         bump = protocol_state.bump,
//     )]
//     pub protocol_state: Account<'info, ProtocolState>,
    
//     /// CHECK: Verified as SPL Token Mint via constraint
//     #[account(
//         mut,
//         constraint = irma_mint.key() == protocol_state.token_a_mint,
//     )]
//     pub irma_mint: UncheckedAccount<'info>,
    
//     /// CHECK: Verified as SPL Token Mint via constraint
//     #[account(
//         constraint = usdc_mint.key() == protocol_state.token_b_mint,
//     )]
//     pub usdc_mint: UncheckedAccount<'info>,
    
//     /// CHECK: User's IRMA token account
//     #[account(mut)]
//     pub user_irma: UncheckedAccount<'info>,
    
//     /// CHECK: User's USDC token account
//     #[account(mut)]
//     pub user_usdc: UncheckedAccount<'info>,
    
//     /// CHECK: Protocol's USDC vault
//     #[account(mut)]
//     pub protocol_usdc_vault: UncheckedAccount<'info>,
    
//     #[account(mut)]
//     pub user: Signer<'info>,
    
//     /// CHECK: This is a PDA used as vault authority
//     #[account(
//         seeds = [b"vault_authority"],
//         bump,
//     )]
//     pub vault_authority: UncheckedAccount<'info>,
    
//     /// CHECK: SPL Token program
//     pub token_program: UncheckedAccount<'info>,
// }

// #[derive(Accounts)]
// pub struct RemoveFreezeAuthority<'info> {
//     /// CHECK: The IRMA mint
//     #[account(mut)]
//     pub irma_mint: UncheckedAccount<'info>,
    
//     /// CHECK: The PDA that is currently the freeze authority
//     #[account(
//         seeds = [b"mint_authority"],
//         bump,
//     )]
//     pub freeze_authority: UncheckedAccount<'info>,
    
//     /// The authority that can invoke the freeze authority removal
//     pub authority: Signer<'info>,
    
//     /// CHECK: SPL Token program (or Token2022)
//     pub token_program: UncheckedAccount<'info>,
// }

// ====================================================================
// END: ACCOUNT STRUCT DEFINITIONS
// ====================================================================

// Declare your modules
pub mod pair_config;
// pub mod bin_array;
pub mod bin_array_manager;
pub mod meteora_integration;
pub mod pricing;
pub mod position_manager;
// pub mod utils;
// pub mod u64x64_math;
// pub mod bin;
// pub mod position;
// pub mod math;
// pub mod u128x128_math;
// pub mod pda;
// pub mod token_2022;

#[program]
pub mod irma {
    use super::*; // This will now correctly bring Init, Maint, Common, etc. into scope

    pub fn initialize(ctx: Context<Init>) -> Result<()> {
        pricing::init_pricing(ctx)
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

    pub fn get_redemption_price(ctx: Context<Common>, quote_token: String) -> Result<f64> {
        pricing::get_redemption_price(ctx, &quote_token)
    }

    pub fn get_prices(ctx: Context<Common>, quote_token: String) -> Result<(f64, f64)> {
        pricing::get_prices(ctx, &quote_token)
    }

    /// Let pricing know about a sale trade event
    /// Note that IRMA is what we are selling (minting).
    pub fn sale_trade_event(ctx: Context<Common>, bought_token: String, bought_amount: u64) -> Result<()> {
        return pricing::mint_irma(ctx, &bought_token, bought_amount);
    }

    /// Let pricing know about a buy-back trade event
    /// Note that IRMA is what we are buying (burning).
    pub fn buy_trade_event(ctx: Context<Common>, sold_token: String, bought_amount: u64) -> Result<()> {
        return pricing::redeem_irma(ctx, &sold_token, bought_amount);
    }
}
