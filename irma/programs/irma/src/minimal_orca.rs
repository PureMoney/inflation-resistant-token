/// Minimal Orca Whirlpool account structures for on-chain program use only
/// This avoids pulling in the full orca_whirlpools crate with its client dependencies

use anchor_lang::prelude::*;

/// Whirlpool account structure - minimal version for reading pool state
#[account]
#[derive(Debug)]
pub struct MinimalWhirlpool {
    pub whirlpools_config: Pubkey,          // 32
    pub whirlpool_bump: [u8; 1],           // 1
    
    pub tick_spacing: u16,                  // 2
    pub tick_spacing_seed: [u8; 2],        // 2
    
    // Token info
    pub token_mint_a: Pubkey,              // 32
    pub token_vault_a: Pubkey,             // 32
    pub fee_growth_global_a: u128,         // 16
    
    pub token_mint_b: Pubkey,              // 32
    pub token_vault_b: Pubkey,             // 32
    pub fee_growth_global_b: u128,         // 16
    
    // Pool state
    pub reward_last_updated_timestamp: u64, // 8
    pub fee_rate: u16,                     // 2
    pub protocol_fee_rate: u16,            // 2
    pub liquidity: u128,                   // 16
    pub sqrt_price: u128,                  // 16
    pub tick_current_index: i32,           // 4
    pub protocol_fee_owed_a: u64,          // 8
    pub protocol_fee_owed_b: u64,          // 8
    
    // Rewards (simplified - usually 3 reward tokens)
    pub reward_infos: [RewardInfo; 3],     // 384 (128 * 3)
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
#[derive(Copy)]
pub struct RewardInfo {
    pub mint: Pubkey,                      // 32
    pub vault: Pubkey,                     // 32
    pub authority: Pubkey,                 // 32
    pub emissions_per_second_x64: u128,    // 16
    pub growth_global_x64: u128,           // 16
}

impl MinimalWhirlpool {
    pub const LEN: usize = 8 + 32 + 1 + 2 + 2 + 32 + 32 + 16 + 32 + 32 + 16 + 8 + 2 + 2 + 16 + 16 + 4 + 8 + 8 + (128 * 3);

    pub fn get_price_a_to_b(&self) -> u128 {
        self.sqrt_price
    }
    
    pub fn get_current_tick(&self) -> i32 {
        self.tick_current_index
    }
}

/// Position account structure - minimal version
#[account]
#[derive(Debug)]
pub struct MinimalPosition {
    pub whirlpool: Pubkey,          // 32
    pub position_mint: Pubkey,      // 32
    pub liquidity: u128,            // 16
    pub tick_lower_index: i32,      // 4
    pub tick_upper_index: i32,      // 4
    pub fee_growth_checkpoint_a: u128, // 16
    pub fee_growth_checkpoint_b: u128, // 16
    pub fee_owed_a: u64,           // 8
    pub fee_owed_b: u64,           // 8
    pub reward_infos: [PositionRewardInfo; 3], // 48 (16 * 3)
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
#[derive(Copy)]
pub struct PositionRewardInfo {
    pub growth_inside_checkpoint: u128, // 16
    pub amount_owed: u64,              // 8
}

impl MinimalPosition {
    pub const LEN: usize = 8 + 32 + 32 + 16 + 4 + 4 + 16 + 16 + 8 + 8 + (24 * 3);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn mock_pubkey() -> Pubkey {
        Pubkey::from_str("11111111111111111111111111111112").unwrap()
    }

    #[test]
    fn test_minimal_whirlpool_len() {
        // Verify the calculated length matches expected struct size
        // This is important for account deserialization
        let expected_len = 8 + // discriminator
            32 + 1 + 2 + 2 + // config, bump, tick_spacing, tick_spacing_seed
            32 + 32 + 16 + // token_mint_a, token_vault_a, fee_growth_global_a
            32 + 32 + 16 + // token_mint_b, token_vault_b, fee_growth_global_b
            8 + 2 + 2 + 16 + 16 + 4 + 8 + 8 + // pool state fields
            (32 + 32 + 32 + 16 + 16) * 3; // reward_infos array

        assert_eq!(MinimalWhirlpool::LEN, expected_len);
    }

    #[test]
    fn test_minimal_position_len() {
        // Verify the calculated length matches expected struct size
        let expected_len = 8 + // discriminator
            32 + 32 + 16 + 4 + 4 + 16 + 16 + 8 + 8 + // position fields
            (16 + 8) * 3; // reward_infos array

        assert_eq!(MinimalPosition::LEN, expected_len);
    }

    #[test]
    fn test_whirlpool_price_methods() {
        let mut whirlpool = MinimalWhirlpool {
            whirlpools_config: mock_pubkey(),
            whirlpool_bump: [1],
            tick_spacing: 64,
            tick_spacing_seed: [0, 1],
            token_mint_a: mock_pubkey(),
            token_vault_a: mock_pubkey(),
            fee_growth_global_a: 0,
            token_mint_b: mock_pubkey(),
            token_vault_b: mock_pubkey(),
            fee_growth_global_b: 0,
            reward_last_updated_timestamp: 0,
            fee_rate: 300, // 0.3%
            protocol_fee_rate: 100, // 0.1%
            liquidity: 1000000,
            sqrt_price: 4295048016, // Example sqrt price
            tick_current_index: -2000,
            protocol_fee_owed_a: 0,
            protocol_fee_owed_b: 0,
            reward_infos: [
                RewardInfo {
                    mint: Pubkey::default(),
                    vault: Pubkey::default(),
                    authority: Pubkey::default(),
                    emissions_per_second_x64: 0,
                    growth_global_x64: 0,
                };
                3
            ],
        };

        // Test price getter
        assert_eq!(whirlpool.get_price_a_to_b(), 4295048016);

        // Test tick getter
        assert_eq!(whirlpool.get_current_tick(), -2000);

        // Test with different values
        whirlpool.sqrt_price = 5000000000;
        whirlpool.tick_current_index = 1500;

        assert_eq!(whirlpool.get_price_a_to_b(), 5000000000);
        assert_eq!(whirlpool.get_current_tick(), 1500);
    }

    #[test]
    fn test_reward_info_serialization() {
        let reward_info = RewardInfo {
            mint: mock_pubkey(),
            vault: mock_pubkey(),
            authority: mock_pubkey(),
            emissions_per_second_x64: 1000000,
            growth_global_x64: 2000000,
        };

        // Test that the struct can be serialized/deserialized
        let mut data = Vec::new();
        reward_info.serialize(&mut data).unwrap();
        assert!(!data.is_empty());

        let deserialized = RewardInfo::deserialize(&mut &data[..]).unwrap();
        assert_eq!(reward_info.mint, deserialized.mint);
        assert_eq!(reward_info.vault, deserialized.vault);
        assert_eq!(reward_info.authority, deserialized.authority);
        assert_eq!(reward_info.emissions_per_second_x64, deserialized.emissions_per_second_x64);
        assert_eq!(reward_info.growth_global_x64, deserialized.growth_global_x64);
    }

    #[test]
    fn test_position_reward_info_serialization() {
        let position_reward_info = PositionRewardInfo {
            growth_inside_checkpoint: 1500000,
            amount_owed: 750000,
        };

        // Test that the struct can be serialized/deserialized
        let mut data = Vec::new();
        position_reward_info.serialize(&mut data).unwrap();
        assert!(!data.is_empty());

        let deserialized = PositionRewardInfo::deserialize(&mut &data[..]).unwrap();
        assert_eq!(position_reward_info.growth_inside_checkpoint, deserialized.growth_inside_checkpoint);
        assert_eq!(position_reward_info.amount_owed, deserialized.amount_owed);
    }

    #[test]
    fn test_minimal_position_creation() {
        let position = MinimalPosition {
            whirlpool: mock_pubkey(),
            position_mint: mock_pubkey(),
            liquidity: 500000,
            tick_lower_index: -1000,
            tick_upper_index: 1000,
            fee_growth_checkpoint_a: 100000,
            fee_growth_checkpoint_b: 200000,
            fee_owed_a: 1000,
            fee_owed_b: 2000,
            reward_infos: [
                PositionRewardInfo {
                    growth_inside_checkpoint: 0,
                    amount_owed: 0,
                };
                3
            ],
        };

        // Verify position data integrity
        assert_eq!(position.liquidity, 500000);
        assert_eq!(position.tick_lower_index, -1000);
        assert_eq!(position.tick_upper_index, 1000);
        assert_eq!(position.fee_owed_a, 1000);
        assert_eq!(position.fee_owed_b, 2000);
    }

    #[test]
    fn test_extreme_values() {
        // Test with extreme values to ensure no overflow
        let whirlpool = MinimalWhirlpool {
            whirlpools_config: mock_pubkey(),
            whirlpool_bump: [255],
            tick_spacing: u16::MAX,
            tick_spacing_seed: [255, 255],
            token_mint_a: mock_pubkey(),
            token_vault_a: mock_pubkey(),
            fee_growth_global_a: u128::MAX,
            token_mint_b: mock_pubkey(),
            token_vault_b: mock_pubkey(),
            fee_growth_global_b: u128::MAX,
            reward_last_updated_timestamp: u64::MAX,
            fee_rate: u16::MAX,
            protocol_fee_rate: u16::MAX,
            liquidity: u128::MAX,
            sqrt_price: u128::MAX,
            tick_current_index: i32::MAX,
            protocol_fee_owed_a: u64::MAX,
            protocol_fee_owed_b: u64::MAX,
            reward_infos: [
                RewardInfo {
                    mint: mock_pubkey(),
                    vault: mock_pubkey(),
                    authority: mock_pubkey(),
                    emissions_per_second_x64: u128::MAX,
                    growth_global_x64: u128::MAX,
                };
                3
            ],
        };

        // Should handle extreme values without panicking
        assert_eq!(whirlpool.get_price_a_to_b(), u128::MAX);
        assert_eq!(whirlpool.get_current_tick(), i32::MAX);
    }

    #[test]
    fn test_negative_tick_values() {
        let whirlpool = MinimalWhirlpool {
            whirlpools_config: mock_pubkey(),
            whirlpool_bump: [1],
            tick_spacing: 64,
            tick_spacing_seed: [0, 1],
            token_mint_a: mock_pubkey(),
            token_vault_a: mock_pubkey(),
            fee_growth_global_a: 0,
            token_mint_b: mock_pubkey(),
            token_vault_b: mock_pubkey(),
            fee_growth_global_b: 0,
            reward_last_updated_timestamp: 0,
            fee_rate: 300,
            protocol_fee_rate: 100,
            liquidity: 1000000,
            sqrt_price: 4295048016,
            tick_current_index: i32::MIN, // Test minimum tick value
            protocol_fee_owed_a: 0,
            protocol_fee_owed_b: 0,
            reward_infos: [
                RewardInfo {
                    mint: Pubkey::default(),
                    vault: Pubkey::default(),
                    authority: Pubkey::default(),
                    emissions_per_second_x64: 0,
                    growth_global_x64: 0,
                };
                3
            ],
        };

        // Should handle negative tick values correctly
        assert_eq!(whirlpool.get_current_tick(), i32::MIN);
    }
}