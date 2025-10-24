// programs/irma/src/protocol_state.rs
//
// This module defines the core ProtocolState account that manages
// the IRMA protocol's integration with Orca Whirlpools.
//
// The ProtocolState is the central source of truth for:
// - Current mint and redemption prices (based on inflation)
// - The Orca Whirlpool being managed
// - The liquidity position within that pool

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

/// The main protocol state account.
/// This account stores the critical parameters for managing
/// the IRMA/USDC liquidity position on Orca.
#[account]
pub struct ProtocolState {
    /// The protocol authority that can update prices and manage the position
    pub authority: Pubkey,
    
    /// The current mint price in lamports (scaled by 1e9 for precision)
    /// This is the price at which users can mint new IRMA tokens
    /// Example: 1.05 USDC = 1_050_000_000
    pub mint_price: u64,
    
    /// The current redemption price in lamports (scaled by 1e9 for precision)
    /// This is CALCULATED dynamically as: backing_reserves / irma_in_circulation
    /// It is NOT stored; instead it's computed from StateMap in pricing.rs
    /// This field is kept for reference but should not be used for calculations
    pub redemption_price: u64,
    
    /// The Pubkey of the Orca Whirlpool this protocol manages
    pub whirlpool: Pubkey,
    
    /// The Pubkey of the liquidity position within the Whirlpool
    pub position: Pubkey,
    
    /// The mint address of token A (typically IRMA)
    pub token_a_mint: Pubkey,
    
    /// The mint address of token B (typically USDC)
    pub token_b_mint: Pubkey,
    
    /// Last time the prices were updated (Unix timestamp)
    pub last_price_update: i64,
    
    /// Last time the liquidity position was rebalanced (Unix timestamp)
    pub last_rebalance: i64,
    
    /// Bump seed for PDA derivation
    pub bump: u8,
    
    /// Reserved space for future upgrades (64 bytes for alignment)
    pub _reserved: [u64; 8],
}

impl ProtocolState {
    /// Size of the ProtocolState account in bytes
    /// 32*6 pubkeys + 8*2 prices + 8*2 timestamps + 1 bump + 7 padding + 8*8 reserved
    pub const LEN: usize = 32 * 6 + 8 * 2 + 8 * 2 + 1 + 7 + 8 * 8;
    
    /// Price scaling factor (1e9 for 9 decimal places)
    pub const PRICE_SCALE: u64 = 1_000_000_000;
    
    /// Initialize a new ProtocolState
    /// Both mint_price and redemption_price should start EQUAL (both 1.0 USDC)
    pub fn initialize(
        &mut self,
        authority: Pubkey,
        initial_price: u64,  // Both prices start here (e.g., 1_000_000_000 for 1.0 USDC)
        whirlpool: Pubkey,
        position: Pubkey,
        token_a_mint: Pubkey,
        token_b_mint: Pubkey,
        bump: u8,
    ) -> Result<()> {
        require!(
            initial_price > 0,
            ProtocolError::ZeroPrice
        );
        
        self.authority = authority;
        self.mint_price = initial_price;
        self.redemption_price = initial_price;  // Both start equal
        self.whirlpool = whirlpool;
        self.position = position;
        self.token_a_mint = token_a_mint;
        self.token_b_mint = token_b_mint;
        self.last_price_update = Clock::get()?.unix_timestamp;
        self.last_rebalance = Clock::get()?.unix_timestamp;
        self.bump = bump;
        self._reserved = [0; 8];
        
        msg!("ProtocolState initialized:");
        msg!("  Authority: {}", authority);
        msg!("  Initial Mint Price: {} ({})", initial_price, Self::format_price(initial_price));
        msg!("  Initial Redemption Price: {} ({})", initial_price, Self::format_price(initial_price));
        msg!("  Whirlpool: {}", whirlpool);
        msg!("  Position: {}", position);
        
        Ok(())
    }
    
    /// Update the mint and redemption prices
    /// This simulates an oracle update with mock inflation data
    pub fn update_prices(
        &mut self,
        new_mint_price: u64,
        new_redemption_price: u64,
    ) -> Result<()> {
        require!(
            new_mint_price >= new_redemption_price,
            ProtocolError::InvalidPriceRelation
        );
        require!(
            new_redemption_price > 0,
            ProtocolError::ZeroPrice
        );
        
        // Sanity check: prices shouldn't change by more than 100% in a single update
        let max_price_change = self.mint_price * 2;
        require!(
            new_mint_price <= max_price_change,
            ProtocolError::PriceChangeTooLarge
        );
        
        let old_mint = self.mint_price;
        let old_redemption = self.redemption_price;
        
        self.mint_price = new_mint_price;
        self.redemption_price = new_redemption_price;
        self.last_price_update = Clock::get()?.unix_timestamp;
        
        msg!("Prices updated:");
        msg!("  Mint: {} -> {} ({})", old_mint, new_mint_price, Self::format_price(new_mint_price));
        msg!("  Redemption: {} -> {} ({})", old_redemption, new_redemption_price, Self::format_price(new_redemption_price));
        
        Ok(())
    }
    
    /// Mark that a rebalance occurred
    pub fn mark_rebalanced(&mut self) -> Result<()> {
        self.last_rebalance = Clock::get()?.unix_timestamp;
        msg!("Position rebalanced at timestamp: {}", self.last_rebalance);
        Ok(())
    }
    
    /// Convert scaled price to human-readable format
    /// Example: 1_050_000_000 -> "1.05"
    fn format_price(price: u64) -> String {
        let dollars = price / Self::PRICE_SCALE;
        let cents = (price % Self::PRICE_SCALE) / (Self::PRICE_SCALE / 100);
        format!("{}.{:02}", dollars, cents)
    }
    
    /// Verify that the caller is the protocol authority
    pub fn verify_authority(&self, authority: &Signer) -> Result<()> {
        require!(
            self.authority == authority.key(),
            ProtocolError::UnauthorizedCaller
        );
        Ok(())
    }

    pub fn apply_inflation_adjustment(
        &mut self,
        inflation_rate: f64, // e.g., 0.05 for 5% annual
    ) -> Result<()> {
        require!(
            inflation_rate >= 0.0 && inflation_rate <= 1.0,
            ProtocolError::InvalidInflationRate
        );
        
        let now = Clock::get()?.unix_timestamp;
        let time_elapsed_seconds = (now - self.last_price_update) as f64;
        let seconds_per_year = 365.25 * 24.0 * 60.0 * 60.0;
        
        // Calculate compounding factor
        let years_elapsed = time_elapsed_seconds / seconds_per_year;
        let multiplier = (1.0 + inflation_rate).powf(years_elapsed);
        
        // Apply inflation ONLY to mint price
        // Redemption price is calculated from backing_reserves / irma_in_circulation
        // and is updated whenever stablecoins are swapped/minted for IRMA
        let new_mint_price = (self.mint_price as f64 * multiplier) as u64;
        
        // Store current redemption as reference (actual value comes from StateMap)
        let current_redemption = self.redemption_price;
        
        msg!(
            "Inflation adjustment: time_elapsed={:.2} years, multiplier={:.6}, old_mint={}, new_mint={}",
            years_elapsed,
            multiplier,
            self.mint_price,
            new_mint_price
        );
        
        // Only update prices, redemption_price is just for reference
        self.update_prices(new_mint_price, current_redemption)?;
        
        Ok(())
    }
}

/// Custom errors for protocol state management
#[error_code]
pub enum ProtocolError {
    #[msg("Mint price must be greater than or equal to redemption price")]
    InvalidPriceRelation,
    
    #[msg("Price cannot be zero")]
    ZeroPrice,
    
    #[msg("Price change is too large (>100% in single update)")]
    PriceChangeTooLarge,
    
    #[msg("Unauthorized: only protocol authority can perform this action")]
    UnauthorizedCaller,
    
    #[msg("Invalid mint address")]
    InvalidMint,
    
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    
    #[msg("Amount cannot be zero")]
    ZeroAmount,
    
    #[msg("Math operation overflow")]
    MathOverflow,
    
    #[msg("Insufficient balance in protocol vault")]
    InsufficientVaultBalance,
    
    #[msg("Invalid inflation rate (must be between 0 and 1)")]
    InvalidInflationRate,
}
