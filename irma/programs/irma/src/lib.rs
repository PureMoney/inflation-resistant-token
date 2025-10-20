// In programs/irma/src/lib.rs

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use std::mem::size_of;

// Import the state structs from your modules, as they are used in the account definitions.
use pricing::{StateMap, StableState};
use orca_integration::OrcaPoolState;
use protocol_state::ProtocolState;

// Declare your program's ID
declare_id!("BqTQKeWmJ4btn3teLsvXTk84gpWUu5CMyGCmncptWfda");

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

// ====================================================================
// Protocol State Management Contexts
// ====================================================================

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + ProtocolState::LEN,
        seeds = [b"protocol_state"],
        bump
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateMockPrices<'info> {
    #[account(
        mut,
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct ApplyInflation<'info> {
    #[account(
        mut,
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    pub authority: Signer<'info>,
}

// ====================================================================
// Token Operations Contexts
// ====================================================================

#[derive(Accounts)]
pub struct MintIrma<'info> {
    #[account(
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    /// CHECK: Verified as SPL Token Mint via constraint
    #[account(
        mut,
        constraint = irma_mint.key() == protocol_state.token_a_mint,
    )]
    pub irma_mint: UncheckedAccount<'info>,
    
    /// CHECK: Verified as SPL Token Mint via constraint
    #[account(
        constraint = usdc_mint.key() == protocol_state.token_b_mint,
    )]
    pub usdc_mint: UncheckedAccount<'info>,
    
    /// CHECK: User's USDC token account
    #[account(mut)]
    pub user_usdc: UncheckedAccount<'info>,
    
    /// CHECK: User's IRMA token account
    #[account(mut)]
    pub user_irma: UncheckedAccount<'info>,
    
    /// CHECK: Protocol's USDC vault
    #[account(mut)]
    pub protocol_usdc_vault: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// CHECK: This is a PDA used as mint authority
    #[account(
        seeds = [b"mint_authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,
    
    /// CHECK: SPL Token program
    pub token_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct RedeemIrma<'info> {
    #[account(
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    /// CHECK: Verified as SPL Token Mint via constraint
    #[account(
        mut,
        constraint = irma_mint.key() == protocol_state.token_a_mint,
    )]
    pub irma_mint: UncheckedAccount<'info>,
    
    /// CHECK: Verified as SPL Token Mint via constraint
    #[account(
        constraint = usdc_mint.key() == protocol_state.token_b_mint,
    )]
    pub usdc_mint: UncheckedAccount<'info>,
    
    /// CHECK: User's IRMA token account
    #[account(mut)]
    pub user_irma: UncheckedAccount<'info>,
    
    /// CHECK: User's USDC token account
    #[account(mut)]
    pub user_usdc: UncheckedAccount<'info>,
    
    /// CHECK: Protocol's USDC vault
    #[account(mut)]
    pub protocol_usdc_vault: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// CHECK: This is a PDA used as vault authority
    #[account(
        seeds = [b"vault_authority"],
        bump,
    )]
    pub vault_authority: UncheckedAccount<'info>,
    
    /// CHECK: SPL Token program
    pub token_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct RemoveFreezeAuthority<'info> {
    /// CHECK: The IRMA mint
    #[account(mut)]
    pub irma_mint: UncheckedAccount<'info>,
    
    /// CHECK: The PDA that is currently the freeze authority
    #[account(
        seeds = [b"mint_authority"],
        bump,
    )]
    pub freeze_authority: UncheckedAccount<'info>,
    
    /// The authority that can invoke the freeze authority removal
    pub authority: Signer<'info>,
    
    /// CHECK: SPL Token program (or Token2022)
    pub token_program: UncheckedAccount<'info>,
}

// ====================================================================
// END: ACCOUNT STRUCT DEFINITIONS
// ====================================================================

// Declare your modules
// pub mod iopenbook;
pub mod orca_integration;
pub mod pricing;
pub mod protocol_state;
pub mod position_manager;
pub mod token_operations;

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

    pub fn update_mint_price_with_inflation(ctx: Context<Common>, quote_token: String, inflation_rate: f64) -> Result<()> {
        pricing::update_mint_price_with_inflation(ctx, &quote_token, inflation_rate)
    }

    pub fn get_redemption_price(ctx: Context<Common>, quote_token: String) -> Result<f64> {
        pricing::get_redemption_price(ctx, &quote_token)
    }

    pub fn get_prices(ctx: Context<Common>, quote_token: String) -> Result<(f64, f64)> {
        pricing::get_prices(ctx, &quote_token)
    }

    // ====================================================================
    // Protocol State Management Instructions
    // ====================================================================

    /// Initialize the protocol state with initial prices and Orca pool information
    /// This must be called once before any other protocol operations
    /// Both mint and redemption prices start equal at the initial_price
    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        initial_price: u64,  // Both prices start here (e.g., 1_000_000_000 for 1.0 USDC)
        whirlpool: Pubkey,
        position: Pubkey,
        token_a_mint: Pubkey,
        token_b_mint: Pubkey,
    ) -> Result<()> {
        let protocol_state = &mut ctx.accounts.protocol_state;
        
        protocol_state.initialize(
            ctx.accounts.authority.key(),
            initial_price,
            whirlpool,
            position,
            token_a_mint,
            token_b_mint,
            ctx.bumps.protocol_state,
        )?;
        
        msg!("Protocol successfully initialized!");
        Ok(())
    }

    /// Update mock inflation prices
    /// In production, this would be replaced by oracle data
    pub fn update_mock_prices(
        ctx: Context<UpdateMockPrices>,
        new_mint_price: u64,
        new_redemption_price: u64,
    ) -> Result<()> {
        let protocol_state = &mut ctx.accounts.protocol_state;
        
        // Verify the caller is authorized
        protocol_state.verify_authority(&ctx.accounts.authority)?;
        
        // Update the prices
        protocol_state.update_prices(new_mint_price, new_redemption_price)?;
        
        msg!("Mock prices updated successfully!");
        Ok(())
    }

    /// Apply inflation adjustment to prices based on inflation rate
    /// This simulates real-world inflation impact on IRMA mint/redemption prices
    pub fn apply_inflation(
        ctx: Context<ApplyInflation>,
        inflation_rate_bps: u32,  // e.g., 500 for 5% = 500 basis points
    ) -> Result<()> {
        let protocol_state = &mut ctx.accounts.protocol_state;
        
        // Verify the caller is authorized
        protocol_state.verify_authority(&ctx.accounts.authority)?;
        
        // Convert basis points to decimal (500 bps = 0.05)
        let inflation_rate = (inflation_rate_bps as f64) / 10_000.0;
        
        msg!("Applying inflation rate: {} bps ({:.2}%)", inflation_rate_bps, inflation_rate * 100.0);
        
        // Apply inflation adjustment
        protocol_state.apply_inflation_adjustment(inflation_rate)?;
        
        msg!("Inflation applied successfully!");
        Ok(())
    }

    // ====================================================================
    // Token Operations (Mint & Redeem)
    // ====================================================================
    
    /// Mint IRMA tokens by depositing USDC at the current mint price
    pub fn mint_irma(ctx: Context<MintIrma>, usdc_amount: u64) -> Result<()> {
        token_operations::mint_irma(ctx, usdc_amount)
    }
    
    /// Redeem IRMA tokens for USDC at the current redemption price
    pub fn redeem_irma(ctx: Context<RedeemIrma>, irma_amount: u64) -> Result<()> {
        token_operations::redeem_irma(ctx, irma_amount)
    }

    /// Remove the freeze authority from the IRMA token mint
    /// This ensures the token cannot be frozen after setup
    pub fn remove_irma_freeze_authority(ctx: Context<RemoveFreezeAuthority>) -> Result<()> {
        token_operations::remove_irma_freeze_authority(ctx)
    }

    // ====================================================================
    // Orca Integration Functions
    // ====================================================================
    pub fn create_orca_pool(
        ctx: Context<CreateOrcaPool>,
        pool_id: Pubkey,
        token_a_mint: Pubkey,
        token_b_mint: Pubkey,
        fee_rate: u64,
        tick_spacing: u16,
    ) -> Result<()> {
        orca_integration::create_orca_pool(ctx, pool_id, token_a_mint, token_b_mint, fee_rate, tick_spacing)
    }

    pub fn update_pool_state(
        ctx: Context<UpdatePoolState>,
        current_price: u64,
        liquidity: u64,
        volume_24h: u64,
    ) -> Result<()> {
        orca_integration::update_pool_state(ctx, current_price, liquidity, volume_24h)
    }

    pub fn get_pool_info(ctx: Context<GetPoolInfo>) -> Result<orca_integration::OrcaPoolState> {
        orca_integration::get_pool_info(ctx)
    }

    pub fn simulate_swap(
        ctx: Context<SimulateSwap>,
        amount_in: u64,
        token_in_mint: Pubkey,
        min_amount_out: u64,
    ) -> Result<u64> {
        orca_integration::simulate_swap(ctx, amount_in, token_in_mint, min_amount_out)
    }
}