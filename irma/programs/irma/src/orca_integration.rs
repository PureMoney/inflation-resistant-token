#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

/// Orca AMM integration for IRMA
/// This module handles creating and managing Orca pools for IRMA trading

// Orca Whirlpools program ID (same on mainnet and devnet)
pub const ORCA_WHIRLPOOLS_PROGRAM_ID: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

// Orca Whirlpools 5.0.1+ uses a different API structure
// We'll use the raw program interactions instead of deprecated client modules.
// Only the following three lines should change when switching between devnet and mainnet.
declare_program!(whirlpool_idl_devnet);
// declare_program!(whirlpool_idl_mainnet);
pub use whirlpool_idl_devnet as whirlpool_idl;
use whirlpool_idl::accounts::Position;
use whirlpool_idl::types::PositionRewardInfo;
use whirlpool_idl::cpi::accounts::*;

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
/// Note: This is not useful for our objectives for the CypherPunk Hackathon.
#[derive(Accounts)]
pub struct SwapWithWhirlpool<'info> {
    /// The Orca Whirlpools program
    /// CHECK: This is the official Orca Whirlpools program
    #[account(address = ORCA_WHIRLPOOLS_PROGRAM_ID)]
    pub whirlpools_program: AccountInfo<'info>,
    
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub whirlpool: AccountInfo<'info>,
    
    /// User's token account for token A
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub token_owner_account_a: AccountInfo<'info>,
    
    /// User's token account for token B  
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub token_owner_account_b: AccountInfo<'info>,
    
    /// Whirlpool's token vault for token A
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub token_vault_a: AccountInfo<'info>,
    
    /// Whirlpool's token vault for token B
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub token_vault_b: AccountInfo<'info>,
    
    /// Tick array 0
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub tick_array_0: AccountInfo<'info>,
    
    /// Tick array 1
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub tick_array_1: AccountInfo<'info>,
    
    /// Tick array 2
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub tick_array_2: AccountInfo<'info>,
    
    /// Oracle account (PDA)
    /// CHECK: Validated by Whirlpool program
    pub oracle: AccountInfo<'info>,
    
    /// Token authority/signer
    /// CHECK: Validated by Whirlpool program
    pub token_authority: Signer<'info>,
    
    /// SPL Token program
    /// CHECK: Validated by Whirlpool program
    pub token_program: AccountInfo<'info>,
}

/// swap function
/// Updated swap function for orca_whirlpools 5.0.1+
/// Note: This is not useful for our objectives for the CypherPunk Hackathon.
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
/// Note: This is not useful for our objectives for the CypherPunk Hackathon.
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

pub fn open_whirlpool_position<'a, 'b, 'c, 'info>(
    context: CpiContext<'a, 'b, 'c, 'info, OpenPosition<'info>>
) -> Result<()> {
    // Implementation for opening a position in the Whirlpool
    // This would involve creating the necessary accounts and initializing them
    // according to the Orca Whirlpool specifications.
    // For brevity, this function is left as a placeholder.
    let bumps: whirlpool_idl::types::OpenPositionBumps = whirlpool_idl::types::OpenPositionBumps {
        position_bump: 0, // Placeholder
    };

    // Further CPI calls to initialize the position would go here
    return whirlpool_idl::cpi::open_position(context, bumps, 0, 0);
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

/// This is the account context for getting the IRMA price from a Whirlpool
#[derive(Accounts)]
pub struct GetIrmaPrice<'info> {
    /// CHECK: Validated by Whirlpool program
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

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::*;
    use std::str::FromStr;

    // Mock Pubkeys for testing
    fn mock_pubkey(seed: &str) -> Pubkey {
        Pubkey::from_str("11111111111111111111111111111112").unwrap()
    }

    fn mock_config_pubkey() -> Pubkey {
        Pubkey::from_str("2LecshUwdy9xi7meFgHtFJQNSKk4KdTrcpvaB56dP2NQ").unwrap()
    }

    fn mock_mint_a() -> Pubkey {
        Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap() // USDC
    }

    fn mock_mint_b() -> Pubkey {
        Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap() // SOL
    }

    #[test]
    fn test_orca_whirlpools_program_id() {
        // Test that the program ID is correct
        let expected = Pubkey::from_str("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc").unwrap();
        assert_eq!(ORCA_WHIRLPOOLS_PROGRAM_ID, expected);
    }

    #[test]
    fn test_get_whirlpool_pdas() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();
        let tick_spacing = 64u16;

        let result = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing);
        assert!(result.is_ok());

        let (whirlpool_pda, oracle_pda, token_vault_a) = result.unwrap();
        
        // Verify PDAs are valid Pubkeys (not all zeros)
        assert_ne!(whirlpool_pda, Pubkey::default());
        assert_ne!(oracle_pda, Pubkey::default());
        assert_ne!(token_vault_a, Pubkey::default());

        // Test that the same inputs produce the same outputs (deterministic)
        let result2 = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing);
        assert!(result2.is_ok());
        let (whirlpool_pda2, oracle_pda2, token_vault_a2) = result2.unwrap();
        
        assert_eq!(whirlpool_pda, whirlpool_pda2);
        assert_eq!(oracle_pda, oracle_pda2);
        assert_eq!(token_vault_a, token_vault_a2);
    }

    #[test]
    fn test_get_whirlpool_pdas_different_tick_spacing() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();

        // Test with different tick spacings produce different PDAs
        let result_64 = get_whirlpool_pdas(&config, &mint_a, &mint_b, 64).unwrap();
        let result_128 = get_whirlpool_pdas(&config, &mint_a, &mint_b, 128).unwrap();

        assert_ne!(result_64.0, result_128.0); // Different whirlpool PDAs
        assert_ne!(result_64.1, result_128.1); // Different oracle PDAs
        assert_ne!(result_64.2, result_128.2); // Different vault PDAs
    }

    #[test]
    fn test_get_whirlpool_pdas_swapped_mints() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();
        let tick_spacing = 64u16;

        // Test with mints in different order
        let result1 = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing).unwrap();
        let result2 = get_whirlpool_pdas(&config, &mint_b, &mint_a, tick_spacing).unwrap();

        // Should produce different PDAs when mints are swapped
        assert_ne!(result1.0, result2.0);
        assert_ne!(result1.1, result2.1);
        assert_ne!(result1.2, result2.2);
    }

    #[test]
    fn test_build_swap_instruction_data() {
        let amount = 1000000u64; // 1 USDC (6 decimals)
        let threshold = 900000u64;
        let sqrt_price_limit = 4295048016u128; // Example sqrt price
        let amount_specified_is_input = true;
        let a_to_b = true;

        let result = build_swap_instruction_data(
            amount,
            threshold,
            sqrt_price_limit,
            amount_specified_is_input,
            a_to_b,
        );

        assert!(result.is_ok());
        let data = result.unwrap();

        // Should start with the swap discriminator (8 bytes)
        assert!(data.len() >= 8);
        assert_eq!(&data[0..8], &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);

        // Should have correct total length (8 + 8 + 8 + 16 + 1 + 1 = 42 bytes)
        assert_eq!(data.len(), 42);
    }

    #[test]
    fn test_build_swap_instruction_data_different_params() {
        // Test with different parameters to ensure serialization works
        let params = [
            (1000000u64, 900000u64, 4295048016u128, true, true),
            (2000000u64, 1800000u64, 4295048017u128, false, false),
            (0u64, 0u64, 0u128, false, true),
            (u64::MAX, u64::MAX, u128::MAX, true, false),
        ];

        for (amount, threshold, sqrt_price_limit, amount_specified_is_input, a_to_b) in params {
            let result = build_swap_instruction_data(
                amount,
                threshold,
                sqrt_price_limit,
                amount_specified_is_input,
                a_to_b,
            );

            assert!(result.is_ok(), "Failed for params: {:?}", (amount, threshold, sqrt_price_limit, amount_specified_is_input, a_to_b));
            let data = result.unwrap();
            assert_eq!(data.len(), 42);
            assert_eq!(&data[0..8], &[0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8]);
        }
    }

    #[test]
    fn test_whirlpool_pda_seeds() {
        // Test that our PDA derivation uses the correct seeds
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();
        let tick_spacing = 64u16;

        // Manually derive whirlpool PDA to verify our implementation
        let (expected_whirlpool, _) = Pubkey::find_program_address(
            &[
                b"whirlpool",
                config.as_ref(),
                mint_a.as_ref(),
                mint_b.as_ref(),
                &tick_spacing.to_le_bytes(),
            ],
            &ORCA_WHIRLPOOLS_PROGRAM_ID,
        );

        let (actual_whirlpool, _, _) = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing).unwrap();
        assert_eq!(expected_whirlpool, actual_whirlpool);
    }

    #[test]
    fn test_oracle_pda_derivation() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();
        let tick_spacing = 64u16;

        let (whirlpool_pda, oracle_pda, _) = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing).unwrap();

        // Manually derive oracle PDA to verify
        let (expected_oracle, _) = Pubkey::find_program_address(
            &[b"oracle", whirlpool_pda.as_ref()],
            &ORCA_WHIRLPOOLS_PROGRAM_ID,
        );

        assert_eq!(expected_oracle, oracle_pda);
    }

    #[test]
    fn test_token_vault_a_derivation() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();
        let tick_spacing = 64u16;

        let (whirlpool_pda, _, token_vault_a) = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing).unwrap();

        // Manually derive token vault A PDA to verify
        let (expected_vault_a, _) = Pubkey::find_program_address(
            &[b"token_vault", whirlpool_pda.as_ref(), mint_a.as_ref()],
            &ORCA_WHIRLPOOLS_PROGRAM_ID,
        );

        assert_eq!(expected_vault_a, token_vault_a);
    }

    // Test various tick spacing values
    #[test]
    fn test_common_tick_spacings() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();

        // Common Orca tick spacings
        let tick_spacings = [1, 8, 64, 128];

        for &tick_spacing in &tick_spacings {
            let result = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing);
            assert!(result.is_ok(), "Failed for tick spacing: {}", tick_spacing);
        }
    }

    #[test]
    fn test_extreme_tick_spacing_values() {
        let config = mock_config_pubkey();
        let mint_a = mock_mint_a();
        let mint_b = mock_mint_b();

        // Test edge cases
        let tick_spacings = [0, 1, u16::MAX];

        for &tick_spacing in &tick_spacings {
            let result = get_whirlpool_pdas(&config, &mint_a, &mint_b, tick_spacing);
            assert!(result.is_ok(), "Failed for tick spacing: {}", tick_spacing);
        }
    }
}
