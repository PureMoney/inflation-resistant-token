// programs/irma/src/position_manager.rs
//
// This module provides the account contexts for interacting with Orca Whirlpools.
// The actual price-to-tick conversion and liquidity management is handled by 
// Orca's Whirlpool program and TypeScript SDK.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

/// Orca Whirlpools Program ID (same on mainnet, devnet, and localnet)
pub const WHIRLPOOL_PROGRAM_ID: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

/// Context for creating a position on Orca Whirlpool
#[derive(Accounts)]
pub struct CreatePosition<'info> {
    /// The protocol state that will own the position
    #[account(mut)]
    pub protocol_state: Account<'info, crate::protocol_state::ProtocolState>,
    
    /// The Whirlpool in which to open the position
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub whirlpool: UncheckedAccount<'info>,
    
    /// The position account to be created
    /// CHECK: Will be created by Whirlpool program
    #[account(mut)]
    pub position: UncheckedAccount<'info>,
    
    /// Position mint (NFT representing the position)
    /// CHECK: Created by Whirlpool program
    #[account(mut)]
    pub position_mint: UncheckedAccount<'info>,
    
    /// Position token account
    /// CHECK: Created by Whirlpool program
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,
    
    /// The authority that can manage the position
    #[account(mut)]
    pub authority: Signer<'info>,
    
    /// Whirlpool program
    /// CHECK: Validated against constant
    #[account(address = WHIRLPOOL_PROGRAM_ID)]
    pub whirlpool_program: UncheckedAccount<'info>,
    
    /// CHECK: Token program
    pub token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Context for modifying liquidity in a position
#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
    /// The protocol state
    #[account(mut)]
    pub protocol_state: Account<'info, crate::protocol_state::ProtocolState>,
    
    /// The Whirlpool
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub whirlpool: UncheckedAccount<'info>,
    
    /// The position
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub position: UncheckedAccount<'info>,
    
    /// Position token account
    /// CHECK: Token account for the position
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,
    
    /// Token A vault
    /// CHECK: Token vault A
    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,
    
    /// Token B vault
    /// CHECK: Token vault B
    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,
    
    /// Tick array lower
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    
    /// Tick array upper
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    
    /// Authority
    pub authority: Signer<'info>,
    
    /// Whirlpool program
    /// CHECK: Validated against constant
    #[account(address = WHIRLPOOL_PROGRAM_ID)]
    pub whirlpool_program: UncheckedAccount<'info>,
    
    /// CHECK: Token program
    pub token_program: UncheckedAccount<'info>,
}

/// Errors for position management
#[error_code]
pub enum PositionError {
    #[msg("Invalid price range: mint price must be >= redemption price")]
    InvalidPriceRange,
    
    #[msg("Price cannot be zero")]
    ZeroPrice,
}
