#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
// use anchor_lang::solana_program;
// use std::str::FromStr;

// Orca Whirlpools 5.0.1+ uses a different API structure
// We'll use the raw program interactions instead of deprecated client modules

/// Orca AMM integration for IRMA
/// This module handles creating and managing Orca pools for IRMA trading

// Orca Whirlpools program ID (same on mainnet and devnet)
pub const ORCA_WHIRLPOOLS_PROGRAM_ID: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

/// Modern helper function to get Whirlpool program derived addresses
/// These are needed for interacting with Orca Whirlpools 5.0.1+
pub fn get_whirlpool_pdas(
    whirlpools_config: &Pubkey,
    token_mint_a: &Pubkey,
    token_mint_b: &Pubkey,
    tick_spacing: u16,
) -> Result<(Pubkey, Pubkey, Pubkey)> {
    // Derive whirlpool PDA
    let (whirlpool_pda, _) = Pubkey::find_program_address(
        &[
            b"whirlpool",
            whirlpools_config.as_ref(),
            token_mint_a.as_ref(),
            token_mint_b.as_ref(),
            &tick_spacing.to_le_bytes(),
        ],
        &ORCA_WHIRLPOOLS_PROGRAM_ID,
    );
    
    // Derive oracle PDA
    let (oracle_pda, _) = Pubkey::find_program_address(
        &[b"oracle", whirlpool_pda.as_ref()],
        &ORCA_WHIRLPOOLS_PROGRAM_ID,
    );
    
    // Derive token vault PDAs
    let (token_vault_a, _) = Pubkey::find_program_address(
        &[b"token_vault", whirlpool_pda.as_ref(), token_mint_a.as_ref()],
        &ORCA_WHIRLPOOLS_PROGRAM_ID,
    );
    
    Ok((whirlpool_pda, oracle_pda, token_vault_a))
}



/// Updated swap instruction context for Orca Whirlpools 5.0.1+
#[derive(Accounts)]
pub struct SwapWithWhirlpool<'info> {
    /// The Orca Whirlpools program
    /// CHECK: This is the official Orca Whirlpools program
    #[account(address = ORCA_WHIRLPOOLS_PROGRAM_ID)]
    pub whirlpools_program: AccountInfo<'info>,
    
    /// The whirlpool account
    #[account(mut)]
    pub whirlpool: AccountInfo<'info>,
    
    /// User's token account for token A
    #[account(mut)]
    pub token_owner_account_a: AccountInfo<'info>,
    
    /// User's token account for token B  
    #[account(mut)]
    pub token_owner_account_b: AccountInfo<'info>,
    
    /// Whirlpool's token vault for token A
    #[account(mut)]
    pub token_vault_a: AccountInfo<'info>,
    
    /// Whirlpool's token vault for token B
    #[account(mut)]
    pub token_vault_b: AccountInfo<'info>,
    
    /// Tick array 0
    #[account(mut)]
    pub tick_array_0: AccountInfo<'info>,
    
    /// Tick array 1
    #[account(mut)]
    pub tick_array_1: AccountInfo<'info>,
    
    /// Tick array 2
    #[account(mut)]
    pub tick_array_2: AccountInfo<'info>,
    
    /// Oracle account (PDA)
    pub oracle: AccountInfo<'info>,
    
    /// Token authority/signer
    pub token_authority: Signer<'info>,
    
    /// SPL Token program
    pub token_program: AccountInfo<'info>,
}

/// Updated swap function for orca_whirlpools 5.0.1+
pub fn swap_with_whirlpool(
    ctx: Context<SwapWithWhirlpool>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<()> {
    use anchor_lang::solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke,
    };
    
    // Build the swap instruction manually using the current orca_whirlpools API
    let swap_instruction_data = build_swap_instruction_data(
        amount,
        other_amount_threshold,
        sqrt_price_limit,
        amount_specified_is_input,
        a_to_b,
    )?;
    
    let accounts = vec![
        AccountMeta::new_readonly(ctx.accounts.whirlpools_program.key(), false),
        AccountMeta::new(ctx.accounts.whirlpool.key(), false),
        AccountMeta::new(ctx.accounts.token_owner_account_a.key(), false),
        AccountMeta::new(ctx.accounts.token_vault_a.key(), false),
        AccountMeta::new(ctx.accounts.token_owner_account_b.key(), false),
        AccountMeta::new(ctx.accounts.token_vault_b.key(), false),
        AccountMeta::new(ctx.accounts.tick_array_0.key(), false),
        AccountMeta::new(ctx.accounts.tick_array_1.key(), false),
        AccountMeta::new(ctx.accounts.tick_array_2.key(), false),
        AccountMeta::new_readonly(ctx.accounts.oracle.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_authority.key(), true),
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
    ];
    
    let swap_instruction = Instruction {
        program_id: ORCA_WHIRLPOOLS_PROGRAM_ID,
        accounts,
        data: swap_instruction_data,
    };
    
    // Prepare account infos for invoke
    let account_infos = vec![
        ctx.accounts.whirlpools_program.clone(),
        ctx.accounts.whirlpool.clone(),
        ctx.accounts.token_owner_account_a.clone(),
        ctx.accounts.token_vault_a.clone(),
        ctx.accounts.token_owner_account_b.clone(),
        ctx.accounts.token_vault_b.clone(),
        ctx.accounts.tick_array_0.clone(),
        ctx.accounts.tick_array_1.clone(),
        ctx.accounts.tick_array_2.clone(),
        ctx.accounts.oracle.clone(),
        ctx.accounts.token_authority.to_account_info(),
        ctx.accounts.token_program.clone(),
    ];
    
    // Execute the swap via CPI
    invoke(&swap_instruction, &account_infos)?;
    
    msg!(
        "Executed Whirlpool swap: amount={}, threshold={}, a_to_b={}", 
        amount, other_amount_threshold, a_to_b
    );
    
    Ok(())
}

/// Helper function to build swap instruction data
/// This creates the instruction data in the format expected by Orca Whirlpools
fn build_swap_instruction_data(
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit: u128,
    amount_specified_is_input: bool,
    a_to_b: bool,
) -> Result<Vec<u8>> {
    use anchor_lang::AnchorSerialize;
    
    // Orca Whirlpools swap instruction discriminator
    // This is typically the first 8 bytes of sha256("global:swap")
    let discriminator: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
    
    let mut data = Vec::new();
    data.extend_from_slice(&discriminator);
    
    // Serialize swap parameters
    amount.serialize(&mut data)?;
    other_amount_threshold.serialize(&mut data)?;
    sqrt_price_limit.serialize(&mut data)?;
    amount_specified_is_input.serialize(&mut data)?;
    a_to_b.serialize(&mut data)?;
    
    Ok(data)
}

/// Function to read current price from an Orca Whirlpool using modern API
/// Updated for orca_whirlpools 5.0.1+
pub fn get_irma_price_from_whirlpool(
    ctx: Context<GetIrmaPrice>
) -> Result<u64> {
    // For orca_whirlpools 5.0.1+, we need to manually deserialize the whirlpool state
    // since the crate doesn't export the state structs for on-chain use
    
    let whirlpool_data = ctx.accounts.whirlpool.data.borrow();
    
    // Skip discriminator (first 8 bytes) and read sqrt_price
    // Whirlpool state layout: discriminator (8) + other fields + sqrt_price (16 bytes, u128)
    if whirlpool_data.len() < 24 {
        return Err(CustomError::InvalidPoolConfig.into());
    }
    
    // Extract sqrt_price from the account data (offset may need adjustment based on actual layout)
    let sqrt_price_bytes = &whirlpool_data[8..24]; // Assuming sqrt_price is at offset 8
    let sqrt_price = u128::from_le_bytes([
        sqrt_price_bytes[0], sqrt_price_bytes[1], sqrt_price_bytes[2], sqrt_price_bytes[3],
        sqrt_price_bytes[4], sqrt_price_bytes[5], sqrt_price_bytes[6], sqrt_price_bytes[7],
        sqrt_price_bytes[8], sqrt_price_bytes[9], sqrt_price_bytes[10], sqrt_price_bytes[11],
        sqrt_price_bytes[12], sqrt_price_bytes[13], sqrt_price_bytes[14], sqrt_price_bytes[15],
    ]);
    
    // Convert sqrt_price to regular price
    // For a Q64.64 sqrt price, the price is sqrt_price^2 / 2^64
    let price = if sqrt_price > 0 {
        // Avoid overflow by doing the calculation carefully
        let price_128 = (sqrt_price as u128).saturating_pow(2);
        let price_64 = (price_128 >> 64) as u64;
        price_64
    } else {
        0
    };
    
    msg!("IRMA price from Whirlpool sqrt_price: {}, calculated price: {}", sqrt_price, price);
    Ok(price)
}

#[derive(Accounts)]
pub struct GetIrmaPrice<'info> {
    /// The whirlpool account containing price data
    pub whirlpool: AccountInfo<'info>,
}

/// Custom error codes for Orca integration
#[error_code]
pub enum CustomError {
    #[msg("Insufficient amount out")]
    InsufficientAmountOut,
    #[msg("Invalid pool configuration")]
    InvalidPoolConfig,
    #[msg("Pool not active")]
    PoolNotActive,
}
