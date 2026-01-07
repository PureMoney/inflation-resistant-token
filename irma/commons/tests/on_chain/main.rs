mod mod_test;
mod test_swap;
mod test_swap_token2022;
mod utils;

pub use mod_test::*;

// Re-export common types for convenience in on-chain tests
pub use anchor_lang::prelude::*;
pub use commons::dlmm::accounts::*;
pub use commons::dlmm::types::*;
pub use commons::quote::*;

/// Documentation for On-Chain Tests
/// 
/// This module contains pure on-chain tests that test the program logic
/// directly without requiring blockchain infrastructure. These tests focus
/// on the computational logic that runs within Solana programs.
/// 
/// Key features:
/// - No RPC calls or blockchain connectivity required
/// - Tests run against mock data and simulated environments
/// - Fast execution and deterministic results
/// - Focus on program instruction logic and state transitions
/// 
/// Test modules:
/// - `test_swap` - Tests for swap calculation logic
/// - `test_swap_token2022` - Tests for Token 2022 specific logic
/// - `utils` - Common utilities for on-chain testing

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_compilation() {
        // Simple test to ensure all modules compile correctly
        println!("On-chain test modules loaded successfully");
    }

    #[test]
    fn test_exports_available() {
        // Test that key exports are available
        use crate::utils::create_mock_clock;
        use crate::utils::create_mock_account_info;
        
        // Create a basic clock to verify exports work
        let clock = create_mock_clock(100, 1700000000);
        assert_eq!(clock.slot, 100);
        
        // Create basic account info to verify exports work
        let key = Pubkey::new_unique();
        let mut lamports = 1000000u64;
        let mut data = vec![0u8; 100];
        let owner = Pubkey::new_unique();
        
        let _account_info = create_mock_account_info(
            &key,
            false,
            false,
            &mut lamports,
            &mut data,
            &owner,
        );
        
        println!("All exports are working correctly");
    }
}