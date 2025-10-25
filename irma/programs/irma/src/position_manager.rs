// programs/irma/src/position_manager.rs
//
// This module provides the account contexts for interacting with Orca Whirlpools.
// The actual price-to-tick conversion and liquidity management is handled by 
// Orca's Whirlpool program and TypeScript SDK.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

use crate::pricing::{StateMap, StableState};

use crate::orca_integration::*;
use whirlpool_idl::cpi::accounts::*;

/// Orca Whirlpools Program ID (same on mainnet, devnet, and localnet)
pub const WHIRLPOOL_PROGRAM_ID: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

/// Context for creating a position on Orca Whirlpool
#[derive(Accounts)]
pub struct CreatePosition<'info> {
    /// The pricing state that will own the position
    #[account(mut)]
    pub pricing_state: Account<'info, crate::pricing::StateMap>,
    
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

impl<'info> CreatePosition<'info> {
    /// Get the position account Pubkey
    pub fn position_key(&self) -> Pubkey {
        self.position.key()
    }

    fn open_whirlpool_position(&self) -> Result<()> {
        let authority_key = self.authority.key();
        let bump_bytes = [1u8];
        let seeds = [authority_key.as_ref(), &bump_bytes];
        let signer_seeds = [&seeds[..]];
        
        let context = CpiContext::new_with_signer(
            self.whirlpool_program.to_account_info(),
            OpenPosition {
                whirlpool: self.whirlpool.to_account_info(),
                position: self.position.to_account_info(),
                position_mint: self.position_mint.to_account_info(),
                position_token_account: self.position_token_account.to_account_info(),
                // authority: self.authority.to_account_info(),
                token_program: self.token_program.to_account_info(),
                associated_token_program: self.token_program.to_account_info(),
                system_program: self.system_program.to_account_info(),
                funder: self.authority.to_account_info(),
                owner: self.authority.to_account_info(),
                rent: self.rent.to_account_info(),
            },
            &signer_seeds,
        );
        // set tick indexes according to how we initialized the pool
        let index0: i32 = 0; // Placeholder for actual tick index calculation
        let index1: i32 = 0; // Placeholder for actual tick index calculation
        return crate::orca_integration::open_whirlpool_position(context, index0, index1);
    }
}

/// Context for modifying liquidity in a position
#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
    /// The pricing state
    #[account(mut)]
    pub pricing_state: Account<'info, crate::pricing::StateMap>,
    
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
