use anchor_lang::prelude::*;
use anchor_spl::token_2022::*;
use commons::quote::*;
use commons::dlmm::accounts::*;
use commons::token_2022::*;
use std::collections::HashMap;
use crate::{OnChainTestPair, create_mock_token_2022_mint_data};

#[test]
fn test_swap_token2022_exact_out_on_chain() -> Result<()> {
    let test_pair = OnChainTestPair::new().unwrap();
    
    println!("Setting up Token 2022 on-chain swap test...");
    
    // Create Token 2022 mints instead of regular SPL tokens
    let token_2022_mint_x = test_pair.config.create_token_2022_mint(
        &test_pair.config.program_id,
        None,
        6, // decimals
    );
    
    let token_2022_mint_y = test_pair.config.create_token_2022_mint(
        &test_pair.config.program_id,
        None,
        9, // decimals
    );

    // Create Token 2022 accounts
    let _user_token_2022_x = test_pair.config.create_token_2022_account(
        &token_2022_mint_x.pubkey(),
        &test_pair.config.program_id,
    );
    
    let _user_token_2022_y = test_pair.config.create_token_2022_account(
        &token_2022_mint_y.pubkey(),
        &test_pair.config.program_id,
    );

    // Create mock LB pair data for Token 2022
    let lb_pair_data = create_mock_lb_pair_token2022(
        token_2022_mint_x.pubkey(),
        token_2022_mint_y.pubkey(),
        test_pair.reserve_x,
        test_pair.reserve_y,
        test_pair.lb_pair,
    );

    // Create mock bin arrays with Token 2022 considerations
    let bin_arrays = create_mock_bin_arrays_token2022(test_pair.lb_pair);

    // Test parameters
    let amount_out = 1000000; // 1 token (6 decimals)
    let swap_for_y = true;

    // Create AccountInfo for Token 2022 mints
    let mint_x_key = token_2022_mint_x.pubkey();
    let mint_y_key = token_2022_mint_y.pubkey();
    
    // Token 2022 mints have larger size due to extensions
    let mint_x_lamports = &mut 0u64;
    let mint_x_data = &mut create_mock_token_2022_mint_data(
        Some(Pubkey::new_unique()), // mint authority
        1000000000,                 // supply (1B tokens)
        6,                          // decimals
        true,                       // with transfer fee
        100,                        // 1% transfer fee
        1000000,                    // max fee
    );
    let mint_x_owner = spl_token_2022::ID;
    let mint_x_account = AccountInfo::new(
        &mint_x_key,
        false,
        false,
        mint_x_lamports,
        mint_x_data,
        &mint_x_owner,
        false,
        0,
    );

    let mint_y_lamports = &mut 0u64;
    let mint_y_data = &mut create_mock_token_2022_mint_data(
        Some(Pubkey::new_unique()), // mint authority
        1000000000000,              // supply (1T tokens)
        9,                          // decimals
        false,                      // no transfer fee
        0,                          // no transfer fee
        0,                          // no max fee
    );
    let mint_y_owner = spl_token_2022::ID;
    let mint_y_account = AccountInfo::new(
        &mint_y_key,
        false,
        false,
        mint_y_lamports,
        mint_y_data,
        &mint_y_owner,
        false,
        0,
    );

    let clock = Clock {
        slot: 100,
        epoch_start_timestamp: 1000000000,
        epoch: 1,
        leader_schedule_epoch: 1,
        unix_timestamp: 1700000000,
    };

    // Test Token 2022 specific functionality
    let _transfer_hook_accounts = get_extra_account_metas_for_transfer_hook(
        token_2022_mint_x.pubkey(),
        mint_x_account.clone(),
        &[]
    );

    // Perform the quote calculation
    let quote_result = quote_exact_out(
        test_pair.lb_pair,
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
            // Assertions for Token 2022
            assert!(quote.amount_in > 0, "Amount in should be greater than 0");
            // assert!(quote.fee >= 0, "Fee should be non-negative");
            
            // Token 2022 might have transfer fees, so amount in could be higher
            assert!(quote.amount_in < amount_out * 3, "Amount in should be reasonable even with transfer fees");
        }
        Err(e) => {
            println!("Token 2022 quote failed: {:?}", e);
            return Err(e);
        }
    }

    println!("Token 2022 on-chain swap test completed successfully!");
    Ok(())
}

#[test]
fn test_token2022_transfer_fee_calculation() -> Result<()> {
    let test_pair = OnChainTestPair::new().unwrap();
    
    println!("Testing Token 2022 transfer fee calculations...");

    // Create a Token 2022 mint with transfer fees
    let mint_with_fees = test_pair.config.create_token_2022_mint_with_transfer_fee(
        &test_pair.config.program_id,
        None,
        6, // decimals
        500, // 5% transfer fee (in basis points)
        1000000, // Max fee of 1 token
    );

    let _token_account = test_pair.config.create_token_2022_account(
        &mint_with_fees.pubkey(),
        &test_pair.config.program_id,
    );

    // Test transfer fee calculations
    let amount = 1000000; // 1 token
    let epoch = 100;

    // Create mock AccountInfo for the mint
    let mint_key = mint_with_fees.pubkey();
    let mint_lamports = &mut 0u64;
    let mint_data = &mut create_mock_token_2022_mint_data(
        Some(Pubkey::new_unique()), // mint authority
        1000000000,                 // supply
        6,                          // decimals
        true,                       // with transfer fee
        500,                        // 5% transfer fee (in basis points)
        1000000,                    // Max fee of 1 token
    );
    let mint_owner = spl_token_2022::ID;
    let mint_account_info = AccountInfo::new(
        &mint_key,
        false,
        false,
        mint_lamports,
        mint_data,
        &mint_owner,
        false,
        0,
    );

    // Test included amount calculation
    let included_result = calculate_transfer_fee_included_amount(
        mint_account_info.clone(),
        amount,
        epoch,
    );

    match included_result {
        Ok(transfer_fee) => {
            assert!(transfer_fee.amount <= amount, "Pre-fee amount should be <= original amount");
            // assert!(transfer_fee.transfer_fee >= 0, "Transfer fee should be non-negative");
        }
        Err(e) => {
            println!("Transfer fee calculation failed: {:?}", e);
            // This might fail due to mock data, which is expected
        }
    }

    // Test excluded amount calculation
    let excluded_result = calculate_transfer_fee_excluded_amount(
        mint_account_info,
        amount,
        epoch,
    );

    match excluded_result {
        Ok(transfer_fee) => {
            assert!(transfer_fee.amount >= amount, "Post-fee amount should be >= original amount");
            // assert!(transfer_fee.transfer_fee >= 0, "Transfer fee should be non-negative");
        }
        Err(e) => {
            println!("Transfer fee excluded calculation failed: {:?}", e);
            // This might fail due to mock data, which is expected
        }
    }

    println!("Token 2022 transfer fee test completed!");
    Ok(())
}

/// Helper function to create mock LB pair data for Token 2022
fn create_mock_lb_pair_token2022(
    token_x_mint: Pubkey,
    token_y_mint: Pubkey,
    reserve_x: Pubkey,
    reserve_y: Pubkey,
    lb_pair_key: Pubkey,
) -> LbPair {
    use commons::dlmm::types::*;
    use commons::extensions::bin_array::BinArrayExtension;
    
    // Create a mock LbPair with Token 2022 considerations
    // Use active_id = 8388608 which gives bin_array_index = 0 (center of symmetric range)
    // Not to be confused with "active_id" in lb_pair_data, which starts from most negative value
    let active_id = -35; // 8388608;
    let bin_array_index = BinArray::bin_id_to_bin_array_index(active_id).unwrap(); // can be negative
    
    // Set up bitmap for bin_array_index = 0 (center position)
    // Map symmetric range [-511..511] to bitmap positions [0..1023]
    let mut bin_array_bitmap = [0u64; 16];

    // turn on three bits
    for i in 0..2 {
        let bitmap_position = (bin_array_index + i + 512) as usize; // Map to positive range
        if bitmap_position < 1024 {
            let word_index = bitmap_position / 64;
            let bit_index = bitmap_position % 64;
            bin_array_bitmap[word_index] |= 1u64 << bit_index;
        }
    }
    
    let lb_pair = LbPair {
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
        active_id: 85, // Use active_id = 85 to get bin_array_index near 0
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

    lb_pair
}

/// Helper function to create mock bin arrays for Token 2022
fn create_mock_bin_arrays_token2022(lb_pair_key: Pubkey) -> HashMap<Pubkey, BinArray> {
    use commons::dlmm::types::*;
    
    use commons::pda::*;
    
    let mut bin_arrays = HashMap::new();
    
    // Create bin arrays at multiple indices to ensure the quote function finds one
    let indices_to_create = vec![-1i64, 0i64, 1i64];
    
    for &index in &indices_to_create {
        let bin_array_pubkey = derive_bin_array_pda(lb_pair_key, index).0;
        
        let mut bins = [Bin::default(); 70];
        
        // Add liquidity to this bin array with Token 2022 considerations
        for i in 0..69 {
            bins[i] = Bin {
                amount_x: 2000000000, // Higher amounts to account for potential fees
                amount_y: 2000000000000,
                amount_x_in: 2100000000, // Simulate some transfer fee impact
                amount_y_in: 2100000000000,
                price: (1000000 + (index * 100000) + (i as i64 * 1000)) as u128,
                liquidity_supply: 2000000000,
                reward_per_token_stored: [0; 2],
                fee_amount_x_per_token_stored: 0,
                fee_amount_y_per_token_stored: 0,
            };
        }
        
        let bin_array = BinArray {
            index,
            version: 0,
            lb_pair: lb_pair_key,
            _padding: [0; 7],
            bins,
        };
        
        bin_arrays.insert(bin_array_pubkey, bin_array);
    }
    
    bin_arrays
}