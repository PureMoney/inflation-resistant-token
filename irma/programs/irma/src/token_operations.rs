// programs/irma/src/token_operations.rs
//
// This module handles minting and redemption of IRMA tokens.
// 
// Minting: User deposits USDC at the current mint_price, receives IRMA
// Redemption: User burns IRMA at the current redemption_price, receives USDC

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::instruction::{Instruction, AccountMeta};
use crate::protocol_state::ProtocolError;
use crate::{MintIrma, RedeemIrma};

// SPL Token Program ID
declare_id!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// Number of decimals for IRMA token (same as USDC)
pub const IRMA_DECIMALS: u8 = 6;

/// Price scaling factor (1e9 for precise calculations)
pub const PRICE_SCALE: u64 = 1_000_000_000;

// SPL Token instruction discriminators
const TOKEN_TRANSFER: u8 = 3;
const TOKEN_MINT_TO: u8 = 7;
const TOKEN_BURN: u8 = 8;

/// Build a Transfer instruction manually
fn create_transfer_instruction(
    source: &Pubkey,
    destination: &Pubkey,
    authority: &Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![TOKEN_TRANSFER];
    data.extend_from_slice(&amount.to_le_bytes());
    
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(*source, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

/// Build a MintTo instruction manually
fn create_mint_to_instruction(
    mint: &Pubkey,
    destination: &Pubkey,
    authority: &Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![TOKEN_MINT_TO];
    data.extend_from_slice(&amount.to_le_bytes());
    
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(*mint, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

/// Build a Burn instruction manually
fn create_burn_instruction(
    account: &Pubkey,
    mint: &Pubkey,
    authority: &Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![TOKEN_BURN];
    data.extend_from_slice(&amount.to_le_bytes());
    
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(*account, false),
            AccountMeta::new(*mint, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

// ====================================================================
// Mint IRMA Instruction
// ====================================================================

/// Mint IRMA tokens by depositing USDC
/// 
/// Formula: irma_amount = (usdc_amount * PRICE_SCALE) / mint_price
/// 
/// Example:
/// - User deposits 105 USDC (105_000_000 with 6 decimals)
/// - Mint price is 1.05 USDC (1_050_000_000 with 1e9 scale)
/// - User receives: (105_000_000 * 1_000_000_000) / 1_050_000_000 = 100_000_000 IRMA (100 IRMA)
pub fn mint_irma(ctx: Context<MintIrma>, usdc_amount: u64) -> Result<()> {
    let protocol_state = &ctx.accounts.protocol_state;
    
    require!(usdc_amount > 0, ProtocolError::ZeroAmount);
    
    // Get current mint price (scaled by 1e9)
    let mint_price = protocol_state.mint_price;
    
    // Calculate IRMA amount to mint
    // irma_amount = (usdc_amount * PRICE_SCALE) / mint_price
    let irma_amount = (usdc_amount as u128)
        .checked_mul(PRICE_SCALE as u128)
        .ok_or(ProtocolError::MathOverflow)?
        .checked_div(mint_price as u128)
        .ok_or(ProtocolError::MathOverflow)?
        as u64;
    
    require!(irma_amount > 0, ProtocolError::ZeroAmount);
    
    msg!("Minting IRMA:");
    msg!("  USDC deposited: {} (scaled)", usdc_amount);
    msg!("  Mint price: {} (scaled by 1e9)", mint_price);
    msg!("  IRMA to mint: {} (scaled)", irma_amount);
    
    // Transfer USDC from user to protocol vault using SPL Token CPI
    let transfer_instruction = create_transfer_instruction(
        ctx.accounts.user_usdc.key,
        ctx.accounts.protocol_usdc_vault.key,
        ctx.accounts.user.key,
        usdc_amount,
    );
    
    anchor_lang::solana_program::program::invoke(
        &transfer_instruction,
        &[
            ctx.accounts.user_usdc.to_account_info(),
            ctx.accounts.protocol_usdc_vault.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
    
    // Mint IRMA to user using SPL Token CPI
    let mint_authority_seeds = &[
        b"mint_authority".as_ref(),
        &[ctx.bumps.mint_authority],
    ];
    let signer_seeds = &[&mint_authority_seeds[..]];
    
    let mint_instruction = create_mint_to_instruction(
        ctx.accounts.irma_mint.key,
        ctx.accounts.user_irma.key,
        ctx.accounts.mint_authority.key,
        irma_amount,
    );
    
    invoke_signed(
        &mint_instruction,
        &[
            ctx.accounts.irma_mint.to_account_info(),
            ctx.accounts.user_irma.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
        signer_seeds,
    )?;
    
    msg!("✅ Successfully minted {} IRMA for {} USDC", irma_amount, usdc_amount);
    
    Ok(())
}

// ====================================================================
// Redeem IRMA Instruction
// ====================================================================

/// Redeem IRMA tokens for USDC
/// 
/// Formula: usdc_amount = (irma_amount * redemption_price) / PRICE_SCALE
/// 
/// Example:
/// - User burns 100 IRMA (100_000_000 with 6 decimals)
/// - Redemption price is 1.00 USDC (1_000_000_000 with 1e9 scale)
/// - User receives: (100_000_000 * 1_000_000_000) / 1_000_000_000 = 100_000_000 USDC (100 USDC)
pub fn redeem_irma(ctx: Context<RedeemIrma>, irma_amount: u64) -> Result<()> {
    let protocol_state = &ctx.accounts.protocol_state;
    
    require!(irma_amount > 0, ProtocolError::ZeroAmount);
    
    // Get current redemption price (scaled by 1e9)
    let redemption_price = protocol_state.redemption_price;
    
    // Calculate USDC amount to return
    // usdc_amount = (irma_amount * redemption_price) / PRICE_SCALE
    let usdc_amount = (irma_amount as u128)
        .checked_mul(redemption_price as u128)
        .ok_or(ProtocolError::MathOverflow)?
        .checked_div(PRICE_SCALE as u128)
        .ok_or(ProtocolError::MathOverflow)?
        as u64;
    
    require!(usdc_amount > 0, ProtocolError::ZeroAmount);
    
    msg!("Redeeming IRMA:");
    msg!("  IRMA to burn: {} (scaled)", irma_amount);
    msg!("  Redemption price: {} (scaled by 1e9)", redemption_price);
    msg!("  USDC to return: {} (scaled)", usdc_amount);
    
    // Burn IRMA from user using SPL Token CPI
    let burn_instruction = create_burn_instruction(
        ctx.accounts.user_irma.key,
        ctx.accounts.irma_mint.key,
        ctx.accounts.user.key,
        irma_amount,
    );
    
    anchor_lang::solana_program::program::invoke(
        &burn_instruction,
        &[
            ctx.accounts.user_irma.to_account_info(),
            ctx.accounts.irma_mint.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
    )?;
    
    // Transfer USDC from protocol vault to user using SPL Token CPI
    let vault_authority_seeds = &[
        b"vault_authority".as_ref(),
        &[ctx.bumps.vault_authority],
    ];
    let signer_seeds = &[&vault_authority_seeds[..]];
    
    let transfer_instruction = create_transfer_instruction(
        ctx.accounts.protocol_usdc_vault.key,
        ctx.accounts.user_usdc.key,
        ctx.accounts.vault_authority.key,
        usdc_amount,
    );
    
    invoke_signed(
        &transfer_instruction,
        &[
            ctx.accounts.protocol_usdc_vault.to_account_info(),
            ctx.accounts.user_usdc.to_account_info(),
            ctx.accounts.vault_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
        signer_seeds,
    )?;
    
    msg!("✅ Successfully redeemed {} IRMA for {} USDC", irma_amount, usdc_amount);
    
    Ok(())
}

/// Remove the freeze authority from the IRMA mint
/// This ensures tokens cannot be frozen after deployment
pub fn remove_irma_freeze_authority(ctx: Context<crate::RemoveFreezeAuthority>) -> Result<()> {
    msg!("🔒 Removing freeze authority from IRMA mint...");
    
    // SetAuthority instruction for SPL Token / Token2022
    // Discriminator: 6
    // Data: [6, authority_type(u8), new_authority(Option<Pubkey>)]
    // authority_type for FreezeAccount = 2
    
    let mut data = vec![6u8]; // SetAuthority discriminator
    data.push(2u8); // FreezeAccount authority type
    
    // None variant for Option<Pubkey> (to remove authority)
    data.push(0u8); // None discriminant
    
    let set_authority_ix = Instruction {
        program_id: ctx.accounts.token_program.key(),
        accounts: vec![
            AccountMeta::new(ctx.accounts.irma_mint.key(), false),
            AccountMeta::new_readonly(ctx.accounts.freeze_authority.key(), true),
        ],
        data,
    };
    
    // Create signer seeds for the freeze_authority PDA
    let freeze_authority_seeds = &[
        b"mint_authority".as_ref(),
        &[ctx.bumps.freeze_authority],
    ];
    let signer_seeds = &[&freeze_authority_seeds[..]];
    
    invoke_signed(
        &set_authority_ix,
        &[
            ctx.accounts.irma_mint.to_account_info(),
            ctx.accounts.freeze_authority.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        ],
        signer_seeds,
    )?;
    
    msg!("✅ Successfully removed freeze authority from IRMA mint");
    Ok(())
}

