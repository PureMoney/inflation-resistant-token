use anchor_lang::prelude::*;
use orca_whirlpools_client::{
    get_fee_tier_address,
    get_token_badge_address,
    get_whirlpool_address,
};

/// Create an Orca Whirlpool for IRMA token swaps
/// This derives the pool addresses and returns the configuration needed
pub fn create_orca_whirlpool(
    whirlpool_config: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    tick_spacing: u16,
    initial_sqrt_price: u128,
) -> Result<OrcaPoolConfig> {
    // Convert anchor_lang::Pubkey to solana_pubkey::Pubkey for Orca client
    let config_key = solana_pubkey::Pubkey::new_from_array(whirlpool_config.to_bytes());
    let mint_a_key = solana_pubkey::Pubkey::new_from_array(token_mint_a.to_bytes());
    let mint_b_key = solana_pubkey::Pubkey::new_from_array(token_mint_b.to_bytes());
    
    // Get token badges
    let (token_badge_a, _) = get_token_badge_address(&config_key, &mint_a_key)
        .map_err(|_| ProgramError::Custom(1001))?;
    let (token_badge_b, _) = get_token_badge_address(&config_key, &mint_b_key)
        .map_err(|_| ProgramError::Custom(1002))?;
    
    // Get whirlpool PDA
    let (whirlpool_pda, whirlpool_bump) = get_whirlpool_address(&config_key, &mint_a_key, &mint_b_key, tick_spacing)
        .map_err(|_| ProgramError::Custom(1003))?;
    
    // Get fee tier
    let (fee_tier, fee_tier_bump) = get_fee_tier_address(&config_key, tick_spacing)
        .map_err(|_| ProgramError::Custom(1004))?;
    
    Ok(OrcaPoolConfig {
        whirlpool_config,
        token_mint_a,
        token_mint_b,
        token_badge_a: Pubkey::new_from_array(token_badge_a.to_bytes()),
        token_badge_b: Pubkey::new_from_array(token_badge_b.to_bytes()),
        whirlpool_pda: Pubkey::new_from_array(whirlpool_pda.to_bytes()),
        whirlpool_bump,
        fee_tier: Pubkey::new_from_array(fee_tier.to_bytes()),
        fee_tier_bump,
        tick_spacing,
        initial_sqrt_price,
    })
}

#[derive(Clone, Debug)]
pub struct OrcaPoolConfig {
    pub whirlpool_config: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub token_badge_a: Pubkey,
    pub token_badge_b: Pubkey,
    pub whirlpool_pda: Pubkey,
    pub whirlpool_bump: u8,
    pub fee_tier: Pubkey,
    pub fee_tier_bump: u8,
    pub tick_spacing: u16,
    pub initial_sqrt_price: u128,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_create_orca_whirlpool() {
        let whirlpool_config = Pubkey::from_str("FcrweFY1G9HJAHG5inkGB6pKg1HZ6x9UC2WioAfWrGkR").unwrap();
        let token_mint_a = Pubkey::from_str("ADqpCiuXTnhDsXVaeZMbTpuriotmjGZUh4sptzzzmFmm").unwrap();
        let token_mint_b = Pubkey::from_str("BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k").unwrap();
        let tick_spacing = 8;
        let initial_sqrt_price = 7459106261056563200u128;
        
        let result = create_orca_whirlpool(
            whirlpool_config,
            token_mint_a,
            token_mint_b,
            tick_spacing,
            initial_sqrt_price,
        );
        
        assert!(result.is_ok(), "Failed to create Orca whirlpool");
        
        let pool_config = result.unwrap();
        println!("\n✅ Orca Pool Created Successfully!");
        println!("   Whirlpool Address: {}", pool_config.whirlpool_pda);
        println!("   Token A: {}", pool_config.token_mint_a);
        println!("   Token B: {}", pool_config.token_mint_b);
        println!("   Fee Tier: {}", pool_config.fee_tier);
        println!("   Token Badge A: {}", pool_config.token_badge_a);
        println!("   Token Badge B: {}", pool_config.token_badge_b);
        println!("   Tick Spacing: {}", pool_config.tick_spacing);
    }
}