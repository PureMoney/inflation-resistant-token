use super::utils::*;
use anchor_lang::prelude::*;
use anchor_spl::token::spl_token;
use commons::quote::*;
use commons::dlmm::accounts::*;
use std::collections::HashMap;

#[test]
fn test_swap_exact_out_on_chain() -> Result<()> {
    println!("Testing on-chain swap exact out logic...");

    // Create mock data for on-chain testing (no blockchain required)
    let token_x_mint = Pubkey::new_unique();
    let token_y_mint = Pubkey::new_unique();
    let lb_pair_key = Pubkey::new_unique();
    let reserve_x = Pubkey::new_unique();
    let reserve_y = Pubkey::new_unique();

    // Create mock LB pair data for testing on-chain logic
    let lb_pair_data = create_mock_lb_pair(
        token_x_mint,
        token_y_mint,
        reserve_x,
        reserve_y,
        lb_pair_key,
    );

    // Create mock bin arrays for on-chain testing
    let bin_arrays = create_mock_bin_arrays(lb_pair_key);

    // Test parameters
    let amount_out = 1000000; // 1 token (6 decimals)
    let swap_for_y = true;

    // Create mock AccountInfo using our utility functions
    let mut mint_x_lamports = 0u64;
    let mut mint_x_data = create_mock_mint_data(Some(Pubkey::new_unique()), 1000000000, 6, false);
    let mint_x_account = create_mock_account_info(
        &token_x_mint,
        false,
        false,
        &mut mint_x_lamports,
        &mut mint_x_data,
        &spl_token::ID,
    );

    let mut mint_y_lamports = 0u64;
    let mut mint_y_data = create_mock_mint_data(Some(Pubkey::new_unique()), 1000000000000, 9, false);
    let mint_y_account = create_mock_account_info(
        &token_y_mint,
        false,
        false,
        &mut mint_y_lamports,
        &mut mint_y_data,
        &spl_token::ID,
    );

    // Create mock clock for on-chain testing
    let clock = create_mock_clock(100, 1700000000);

    // Perform the on-chain quote calculation (pure program logic)
    let quote_result = quote_exact_out(
        lb_pair_key,
        &lb_pair_data,
        amount_out,
        swap_for_y,
        bin_arrays,
        None, // No bitmap extension for this test
        &clock,
        mint_x_account,
        mint_y_account,
    );

    match quote_result {
        Ok(quote) => {
            // Test on-chain logic assertions
            assert!(quote.amount_in > 0, "Amount in should be greater than 0");
            // assert!(quote.fee >= 0, "Fee should be non-negative");
            
            // The amount in should be reasonable (not too high)
            assert!(quote.amount_in < amount_out * 2, "Amount in should be reasonable");
        }
        Err(e) => {
            println!("On-chain quote failed: {:?}", e);
            return Err(e);
        }
    }

    println!("On-chain swap test completed successfully!");
    Ok(())
}

#[test]
fn test_swap_exact_in_on_chain() -> Result<()> {
    println!("Testing on-chain swap exact in logic...");

    // Create mock data for pure on-chain testing
    let token_x_mint = Pubkey::new_unique();
    let token_y_mint = Pubkey::new_unique();
    let lb_pair_key = Pubkey::new_unique();
    let reserve_x = Pubkey::new_unique();
    let reserve_y = Pubkey::new_unique();

    // Create mock LB pair data for on-chain logic testing
    let lb_pair_data = create_mock_lb_pair(
        token_x_mint,
        token_y_mint,
        reserve_x,
        reserve_y,
        lb_pair_key,
    );

    // Create mock bin arrays for on-chain testing
    let bin_arrays = create_mock_bin_arrays(lb_pair_key);

    // Test parameters
    let amount_in = 1000000; // 1 token (6 decimals)
    let swap_for_y = false; // Swap X for Y

    // Create mock AccountInfo using utility functions
    let mut mint_x_lamports = 0u64;
    let mut mint_x_data = create_mock_mint_data(Some(Pubkey::new_unique()), 1000000000, 6, false);
    let mint_x_account = create_mock_account_info(
        &token_x_mint,
        false,
        false,
        &mut mint_x_lamports,
        &mut mint_x_data,
        &spl_token::ID,
    );

    let mut mint_y_lamports = 0u64;
    let mut mint_y_data = create_mock_mint_data(Some(Pubkey::new_unique()), 1000000000000, 9, false);
    let mint_y_account = create_mock_account_info(
        &token_y_mint,
        false,
        false,
        &mut mint_y_lamports,
        &mut mint_y_data,
        &spl_token::ID,
    );

    // Create mock clock for on-chain testing
    let clock = create_mock_clock(100, 1700000000);
    // Perform the on-chain quote calculation (pure program logic)
    let quote_result = quote_exact_in(
        lb_pair_key,
        &lb_pair_data,
        amount_in,
        swap_for_y,
        bin_arrays,
        None,
        &clock,
        mint_x_account,
        mint_y_account,
    );

    match quote_result {
        Ok(quote) => {
            // Test on-chain logic assertions
            assert!(quote.amount_out > 0, "Amount out should be greater than 0");
            // assert!(quote.fee >= 0, "Fee should be non-negative");
            
            // The amount out should be less than amount in (due to fees)
            assert!(quote.amount_out <= amount_in, "Amount out should be less than or equal to amount in");
        }
        Err(e) => {
            println!("On-chain quote exact in failed: {:?}", e);
            return Err(e);
        }
    }

    println!("On-chain swap exact in test completed successfully!");
    Ok(())
}

/// Helper function to create mock LB pair data
fn create_mock_lb_pair(
    token_x_mint: Pubkey,
    token_y_mint: Pubkey,
    reserve_x: Pubkey,
    reserve_y: Pubkey,
    lb_pair_key: Pubkey,
) -> commons::dlmm::accounts::LbPair {
    use commons::dlmm::types::*;
    
    // Set up bitmap correctly - try setting the last bit (position 511)
    // This might correspond to bin_array_index closer to 0
    let mut bin_array_bitmap = [0u64; 16];
    
    // Set bit at position 511 (last bit)
    let word_index = 511 / 64; // = 7
    let bit_index = 511 % 64;   // = 63
    bin_array_bitmap[word_index] |= 1u64 << bit_index;
    
    let mut lb_pair = LbPair {
        parameters: StaticParameters {
            base_factor: 5000,
            filter_period: 30,
            decay_period: 600,
            reduction_factor: 5000,
            variable_fee_control: 40000,
            protocol_share: 1000,
            max_volatility_accumulator: 350000,
            min_bin_id: 0,
            max_bin_id: 143,
            base_fee_power_factor: 2,
            _padding: [0; 5],
        },
        v_parameters: VariableParameters {
            volatility_accumulator: 0,
            volatility_reference: 0,
            index_reference: 8388608,
            _padding: [0u8; 4],
            last_update_timestamp: 1700000000,
            _padding_1: [0; 8],
        },
        bump_seed: [0; 1],
        require_base_factor_seed: 0u8,
        base_factor_seed: [0u8; 2],
        status: PairStatus::Enabled as u8,
        bin_step: 25,
        pair_type: PairType::PermissionlessV2 as u8,
        active_id: -35, // active_id, // Use active_id = 8388608 to get bin_array_index = -1
        bin_step_seed: [0; 2],
        token_x_mint,
        token_y_mint,
        reserve_x,
        reserve_y,
        protocol_fee: ProtocolFee {
            amount_x: 0,
            amount_y: 0,
        },
        reward_infos: [RewardInfo::default(); 2],
        oracle: Pubkey::default(),
        bin_array_bitmap,
        last_updated_at: 1700000000,
        // whitelisted_wallet: Pubkey::default(),
        pre_activation_swap_address: Pubkey::default(),
        base_key: lb_pair_key,
        activation_type: ActivationType::Timestamp as u8,
        creator_pool_on_off_control: 0u8,
        // _padding: [0; 7],
        activation_point: 0,
        pre_activation_duration: 0,
        _padding_1: [0u8; 32],
        _padding_2: [0u8; 32],
        _padding_3: [0u8; 8],
        _padding_4: 0u64,
        creator: Pubkey::default(),
        token_mint_x_program_flag: 0u8,
        token_mint_y_program_flag: 0u8,
        _reserved: [0u8; 22],
    };

    // Set status to enabled
    let pair_status = PairStatus::Enabled as u8;
    lb_pair.pair_type = pair_status;

    lb_pair
}

/// Helper function to create mock bin arrays
fn create_mock_bin_arrays(lb_pair_key: Pubkey) -> HashMap<Pubkey, commons::dlmm::accounts::BinArray> {
    use commons::dlmm::types::*;
    use commons::dlmm::accounts::*;
    use commons::derive_bin_array_pda;
    
    
    let mut bin_arrays = HashMap::new();
    
    // Create a bin array at index -1 (which corresponds to bitmap position 511)
    let bin_array_index = -1i64;
    let bin_array_pubkey = derive_bin_array_pda(lb_pair_key, bin_array_index).0;
    
    let mut bins = [Bin::default(); 70];
    
    // Add liquidity to all bins to ensure we have enough liquidity
    for i in 0..70 {
        bins[i] = Bin {
            amount_x: 1000000000,
            amount_y: 1000000000000,
            amount_x_in: 0,
            amount_y_in: 0,
            price: 1000000,
            liquidity_supply: 1000000000,
            reward_per_token_stored: [0; 2],
            fee_amount_x_per_token_stored: 0,
            fee_amount_y_per_token_stored: 0,
        };
    }
    
    let bin_array = BinArray {
        index: bin_array_index,
        version: 0,
        lb_pair: lb_pair_key,
        _padding: [0; 7],
        bins,
    };
    
    bin_arrays.insert(bin_array_pubkey, bin_array);
    
    bin_arrays
}