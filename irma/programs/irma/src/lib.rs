// In programs/irma/src/lib.rs
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use std::mem::size_of;
use std::str::FromStr;
// use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
// use anchor_spl::token_2022::ID as TOKEN_2022_PROGRAM_ID;

// Module declarations
pub mod errors;
pub mod pricing;
pub mod position_manager;
pub mod meteora_integration;
pub mod pair_config;
pub mod bin_array_manager;
pub mod utils;

// Import the state structs from your modules, as they are used in the account definitions.
pub use pricing::{StateMap, StableState};
use errors::CustomError;

// declare_program!(dlmm);
// use commons::dlmm::borsh::*;

// Declare your program's ID
// declare_id!("BqTQKeWmJ4btn3teLsvXTk84gpWUu5CMyGCmncptWfda");
declare_id!("E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs");

// use anchor_lang::context::Context;

use commons::dlmm::accounts::*;
use commons::{fetch_lb_pair_state, get_price_from_id};
use commons::{BIN_ARRAY_BITMAP_SEED, SCALE_OFFSET};

// Re-export types for IDL generation
pub use position_manager::{AllPosition, SinglePosition, MintInfo, MintWithProgramId, TokenEntry};
pub use meteora_integration::Core;
pub use pair_config::*;

pub const IRMA_ID: Pubkey = crate::ID;

#[derive(Clone, Debug, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub enum MarketMakingMode {
    ModeRight,
    ModeLeft,
    ModeBoth,
    ModeView,
}

impl Default for MarketMakingMode {
    fn default() -> Self {
        MarketMakingMode::ModeView
    }
}

impl FromStr for MarketMakingMode {
    type Err = anchor_lang::error::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "moderight" => Ok(MarketMakingMode::ModeRight),
            "modeleft" => Ok(MarketMakingMode::ModeLeft),
            "modeboth" => Ok(MarketMakingMode::ModeBoth),
            "modeview" => Ok(MarketMakingMode::ModeView),
            _ => Ok(MarketMakingMode::default()),
        }
    }
}

// ====================================================================
// START: DEFINE ALL INSTRUCTION ACCOUNT STRUCTS HERE
// ====================================================================

#[derive(Accounts)]
pub struct Init<'info> {
    // Note: We need to qualify MAX_BACKING_COUNT with its module
    #[account(
        init,
        space=32 + 8 + size_of::<StableState>()*pricing::MAX_BACKING_COUNT,
        payer=irma_admin, seeds=[b"state_v5".as_ref()],
        bump
    )]
    pub state: Account<'info, StateMap>,
    #[account(mut)]
    pub irma_admin: Signer<'info>,
    // Space calculation for Core:
    // 8 bytes for discriminator
    // 32 bytes for owner (Pubkey)
    // 4 bytes for config Vec length + (max 10 configs * ~200 bytes each) = 2004 bytes
    // AllPosition struct:
    //   - 4 bytes for all_positions Vec length + (max 10 positions * ~300 bytes each) = 3004 bytes  
    //   - 4 bytes for tokens Vec length + (max 20 tokens * ~200 bytes each) = 4004 bytes
    // Total: 8 + 32 + 2004 + 3004 + 4004 + buffer = ~10000 bytes
    #[account(
        init,
        space=8 + 10000,
        payer=irma_admin,
        seeds=[b"core_v5".as_ref()],
        bump
    )]
    pub core: Account<'info, Core>,
    pub system_program: Program<'info, System>,
    // pub bumps: InitBumps,
}

#[derive(Accounts)]
pub struct Maint<'info> {
    #[account(mut, seeds=[b"state_v5".as_ref()], bump)]
    pub state: Account<'info, StateMap>,
    pub irma_admin: Signer<'info>,
    #[account(mut, seeds=[b"core_v5".as_ref()], bump)]
    pub core: Account<'info, Core>,
    pub system_program: Program<'info, System>,
    // pub bumps: MaintBumps,
}

/// Context to force Core and related types into IDL
#[derive(Accounts)]
pub struct GetCoreData<'info> {
    #[account(mut)]
    pub core: Account<'info, Core>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(lb_pair: Pubkey)]
pub struct CreateBitmapExtension<'a> {
    #[account(
        init,
        space=8 + size_of::<BinArrayBitmapExtension>(),
        payer=irma_admin,
        seeds=[BIN_ARRAY_BITMAP_SEED, lb_pair.as_ref()],
        bump
    )]
    /// CHECK: This account will be initialized as a BinArrayBitmapExtension
    pub bitmap_extension: AccountInfo<'a>,
    #[account(mut)]
    pub irma_admin: Signer<'a>,
    pub system_program: Program<'a, System>,
}



// Declare your modules (NOTE: already declared above)
// pub mod pair_config;
// pub mod bin_array_manager;
// pub mod meteora_integration;
// pub mod pricing;
// pub mod position_manager;
// pub mod utils;

// ====================================================================
// START: DEFINE ALL CPI API
// ====================================================================
// use crate::meteora_integration::Core;

#[program]
pub mod irma {
    use super::*; // This will now correctly bring Init, Maint, etc. into scope

    /// Initialize the IRMA protocol
    /// The context accounts will be initialized by pricing to conttain reserves in alphabetical order.
    /// The remaining_accounts should contain the LbPair accounts in the same order as the reserves.
    pub fn initialize(
        mut ctx: Context<Init>,
        owner: String,
        config_keys: Vec<String>
    ) -> Result<()> {
        let owner_pk = Pubkey::from_str(&owner).unwrap();

        assert_eq!(config_keys.len() == 0, true);

        let config_pks: Vec<Pubkey> = config_keys.iter()
            .map(|key| Pubkey::from_str(key).unwrap())
            .collect();

        // Initialize the pricing system first
        pricing::init_pricing(&mut ctx)?;

        // TODO: Initialize Core separately if needed
        // For now, just do basic initialization
        msg!("IRMA protocol initialized with owner: {}", owner_pk);
        msg!("Config keys count: {}", config_pks.len());

        assert_eq!(config_pks.len() == 0, true);

        let core = Core::create_core(owner_pk, config_pks)?;
        ctx.accounts.core.set_inner(core);
        Ok(())
    }

    pub fn add_reserve(
        ctx: Context<Maint>,
        symbol: String,
        mint_address: Pubkey,
        decimals: u8
    ) -> Result<()> {
        msg!("Add stablecoin entry, size of StateMap: {}", ctx.accounts.state.reserves.len());
        pricing::add_reserve(ctx, &symbol, mint_address, decimals)
    }

    pub fn remove_reserve(
        ctx: Context<Maint>,
        symbol: String
    ) -> Result<()> {
        pricing::remove_reserve(ctx, &symbol)
    }

    pub fn disable_reserve(
        ctx: Context<Maint>,
        symbol: String
    ) -> Result<()> {
        pricing::disable_reserve(ctx, &symbol)
    }

    /// This connects a reserve stablecoin to its corresponding LBPair.
    /// There can only be a single LbPair per stablecoin reserve.
    pub fn update_reserve_lbpair<'info>(
        ctx: Context<'_, '_, 'info, 'info, Maint<'info>>, symbol: String, lb_pair: String
    ) -> Result<()> {
        // msg!("Update reserve LB pair for symbol: {}, lb_pair: {}", symbol, lb_pair);
        let lb_pair_key: Pubkey = Pubkey::from_str(&lb_pair).unwrap();
        {
            let stablecoin = ctx.accounts.state.reserves.iter().find(|r| r.symbol == symbol)
                .ok_or(error!(CustomError::ReserveNotFound))?;
            let lb_pair_state = fetch_lb_pair_state(
                &ctx.remaining_accounts,
                &lb_pair_key,
            )?;
            // check that the input LbPair is valid and matches the reserve stablecoin mint
            if !lb_pair_state.token_y_mint.eq(&stablecoin.mint_address) {
                return Err(error!(CustomError::InvalidLbPairState));
            }
        }
        // add the LbPair to the core config if not already present
        // let remaining_accounts = &ctx.remaining_accounts;
        let core_mut = &mut ctx.accounts.core;
        if core_mut.config.len() == 0 {
            // msg!("Core config is empty, adding all reserves' LB pairs");
            let reserves = &ctx.accounts.state.reserves;
            for reserve in reserves.iter() {
                // msg!("Core config length b4: {}", core_mut.config.len());
                let pool_id = reserve.pool_id.clone();
                core_mut.config.push(PairConfig {
                    pair_address: pool_id.to_string(),
                    x_amount: 0,
                    y_amount: 0,
                    mode: MarketMakingMode::ModeView,
                });
                // msg!("Core config length after: {}", core_mut.config.len());
                if core_mut.position_data.all_positions.iter().all(|p| p.lb_pair != pool_id) {
                    core_mut.position_data.all_positions.push(
                        position_manager::SinglePosition::new(pool_id.clone())
                    );
                }
            }
        }
        else if !core_mut.config.iter().any(|pairc: &PairConfig| pairc.pair_address == lb_pair) {
            core_mut.config.push(PairConfig {
                pair_address: lb_pair.clone(),
                x_amount: 0,
                y_amount: 0,
                mode: MarketMakingMode::ModeBoth,
            });
            core_mut.position_data.all_positions.push(
                position_manager::SinglePosition::new(lb_pair_key.clone())
            );
        }
        // else {
        //     msg!("LB pair already in core config, clean up core config and position data");
        //     core_mut.clean_up_config_and_positions()?;
        // }
        let _ = core_mut.fetch_token_info(&ctx.remaining_accounts)?;
        // msg!("Core config length after update: {}", core_mut.config.len());
        // finally, update the pool_id for the given stablecoin symbol
        let reserves = &mut ctx.accounts.state.reserves;
        let stablecoin_mut = &mut reserves.iter_mut().find(|r| r.symbol == symbol)
            .ok_or(error!(CustomError::ReserveNotFound))?;
        stablecoin_mut.pool_id = lb_pair_key.clone();
        Ok(())
    }

    pub fn list_reserves(ctx: Context<Maint>) -> Result<String> {
        Ok(pricing::list_reserves(ctx))
    }

    pub fn get_redemption_price(ctx: Context<Maint>, quote_token: String) -> Result<f64> {
        pricing::get_redemption_price(&ctx.accounts.state.reserves, &quote_token)
    }
    
    pub fn get_prices(ctx: Context<Maint>, quote_token: String) -> Result<(f64, f64)> {
        pricing::get_prices(&ctx.accounts.state.reserves, &quote_token)
    }

    pub fn set_mint_price(ctx: Context<Maint>, quote_token: String, new_price: f64) -> Result<()> {
        pricing::set_mint_price(ctx, &quote_token, new_price)
    }

    // NOTE: In the two functions below, the Common accounts struct previously allowed the trader herself
    // to access IRMA. However, now we are changing it so that only the irma_admin (the program
    // maintainer) can call these functions to inform the pricing module of trade events. In other words,
    // the trader should be set to irma_admin in the Common context when calling these functions.
    // To avoid confusion, I have renamed the 'trader' field in Common to 'irma_admin' and replaced
    // all instances of 'Common' with "Maint".

    /// Let pricing know about a sale trade event
    /// Note that IRMA is what we are selling (minting).
    /// bought_amount is the amount of bought_token we received from the sale.
    pub fn sale_trade_event<'info>(
        ctx: Context<'_, '_, 'info, 'info, Maint<'info>>, bought_token: String, bought_amount: u64
    ) -> Result<()> {
        // Extract references to avoid double mutable borrow
        let reserves = &ctx.accounts.state.reserves;
        let lb_pair_key = reserves.iter().find(|stablecoin| stablecoin.symbol == bought_token)
            .ok_or(Error::from(CustomError::ReserveNotFound))?
            .pool_id.clone();
        
        // Get the core reference and work with it consistently
        let core = &mut ctx.accounts.core;
        
        // Find positions for this lb_pair and clone them to avoid borrowing conflicts
        let mut filtered_positions: Vec<SinglePosition> = core.position_data.all_positions
            .iter()
            .filter(|p| p.lb_pair == lb_pair_key)
            .cloned()
            .collect();
    
        // if there are zero positions, warn and return
        if filtered_positions.is_empty() {
            msg!("Warning: No SinglePositions found for lb_pair: {}", lb_pair_key);
            msg!("Please call update_reserve_lbpair to link the reserve to its LbPair.");
            return Ok(());
        }
        if filtered_positions.len() > 1 {
            msg!("Warning: Found {} SinglePositions for lb_pair: {}, keeping only the non-empty one", 
                 filtered_positions.len(), lb_pair_key);
            // Keep only non-empty position
            filtered_positions.retain(|p| !p.position_pks.is_empty());
            // note: the following can remove SinglePositions from other LbPairs
            core.position_data.all_positions.retain(
                |p: &SinglePosition| !p.position_pks.is_empty()
            );
        }
        msg!("   position_pks.len(): {}", core.position_data.all_positions[0].position_pks.len());
        
        let state = &mut ctx.accounts.state;
        let remaining_accounts = ctx.remaining_accounts;

        core.refresh_position_data_with_accounts(state, &mut filtered_positions, &remaining_accounts, bought_token, bought_amount, true)?;
        
        // Update the positions back in core
        if !filtered_positions.is_empty() {
            msg!("   filtered_positions[0].position_pks.len(): {}", filtered_positions[0].position_pks.len());
            if let Some(existing_pos) = core.position_data.all_positions.iter_mut().find(|p| p.lb_pair == lb_pair_key) {
                *existing_pos = filtered_positions[0].clone();
            }
        }
        
        Ok(())
    }

    /// Let pricing know about a buy-back trade event
    /// Note that IRMA is what we are buying back (burning) and we just sold the backing token.
    pub fn buy_trade_event<'info>(
        ctx: Context<'_, '_, 'info, 'info, Maint<'info>>, sold_token: String, irma_amount: u64
    ) -> Result<()> {
        let reserves = &ctx.accounts.state.reserves;
        let lb_pair_key = reserves.iter().find(|stablecoin| stablecoin.symbol == sold_token)
            .ok_or(Error::from(CustomError::ReserveNotFound))?
            .pool_id.clone();
        
        // Get the core reference and work with it consistently
        let core = &mut ctx.accounts.core;
        
        // Find positions for this lb_pair and clone them to avoid borrowing conflicts
        let mut filtered_positions: Vec<SinglePosition> = core.position_data.all_positions
            .iter()
            .filter(|p| p.lb_pair == lb_pair_key)
            .cloned()
            .collect();

        // if there are zero positions, warn and return
        if filtered_positions.is_empty() {
            msg!("Warning: No SinglePositions found for lb_pair: {}", lb_pair_key);
            msg!("Please call update_reserve_lbpair to link the reserve to its LbPair.");
            return Ok(());
        }
        if filtered_positions.len() > 1 {
            msg!("Warning: Found {} SinglePositions for lb_pair: {}, keeping only the non-empty one", 
                 filtered_positions.len(), lb_pair_key);
            // Keep only non-empty position
            filtered_positions.retain(|p| !p.position_pks.is_empty());
            // note: the following can remove SinglePositions from other LbPairs
            core.position_data.all_positions.retain(
                |p: &SinglePosition| !p.position_pks.is_empty()
            );
        }
        
        let state = &mut ctx.accounts.state;
        let remaining_accounts = ctx.remaining_accounts;

        core.refresh_position_data_with_accounts(state, &mut filtered_positions, &remaining_accounts, sold_token, irma_amount, false)?;
        
        // Update the positions back in core
        if !filtered_positions.is_empty() {
            if let Some(existing_pos) = core.position_data.all_positions.iter_mut().find(|p| p.lb_pair == lb_pair_key) {
                *existing_pos = filtered_positions[0].clone();
            }
        }
        
        Ok(())
    }

    /// Send swap instruction to Meteora DLMM
    /// (This can be used by us only. In practice, traders will interact directly with Meteora or Jupiter.)
    pub fn swap<'info>(
        ctx: Context<'_, '_, 'info, 'info, Maint<'info>>, symbol: String, amount: u64, swap_for_reserve: bool
    ) -> Result<()> {
        // Extract references to avoid double mutable borrow
        let corei = &ctx.accounts.core.clone();
        let core = &mut ctx.accounts.core;
        let payer = &mut ctx.accounts.irma_admin; // this is wrong; payer should be the trader
        let reserves = &mut ctx.accounts.state.reserves;
        let remaining_accounts: &[AccountInfo<'info>] = &ctx.remaining_accounts;

        let lb_pair_key = reserves.iter().find(|r| r.symbol == symbol)
            .ok_or(error!(CustomError::ReserveNotFound))?
            .pool_id.clone();

        // look for positions matching the symbol
        let position = core.position_data.all_positions.iter_mut().find(|p| p.lb_pair == lb_pair_key)
            .ok_or(error!(CustomError::PositionNotFound))?;
        corei.swap(
            payer,
            remaining_accounts,
            position,
            amount,
            swap_for_reserve,
        )?;
        msg!("swap called for symbol: {}, amount: {}, swap_for_reserve: {}", symbol, amount, swap_for_reserve);
        Ok(())
    }

    /// Check all LB pair positions and update from pricing.rs/
    /// This is used to periodically sync all positions for a single reserve (single pool).
    pub fn check_shift_price_ranges<'a>(
        ctx: Context<'_, '_, 'a, 'a, Maint<'a>>, reserve_token: String, position1: Pubkey, position2: Pubkey
    ) -> Result<()> {
        // Process this position - borrow everything we need in one go
        let payer = &mut ctx.accounts.irma_admin; // this should be the-fed
        let reserves = &mut ctx.accounts.state.reserves;
        let remaining_accounts: &[AccountInfo] = &ctx.remaining_accounts;

        let lb_pair_key = reserves.iter().find(|stablecoin| stablecoin.symbol == reserve_token)
            .ok_or(Error::from(CustomError::ReserveNotFound))?
            .pool_id.clone();
        
        // Get the core reference and work with it consistently
        let core = &mut ctx.accounts.core;
        
        // Find the core_position index for this lb_pair
        let pos_index = core.position_data.all_positions.iter()
            .position(|p| p.lb_pair == lb_pair_key)
            .ok_or(error!(CustomError::SinglePositionNotFound))?;
        
        // Clone the core_position to avoid borrowing conflicts
        let mut core_position = core.position_data.all_positions[pos_index].clone();

        // boxed position is a big kludge to satisfy lifetime requirements
        // can't figure out how to pass position: &Pubkey directly
        let boxed_position: &'static Pubkey = Box::leak(Box::new(position));
            
        // following code is supposed to be in meteora_integration.rs
        // moved here to avoid stack memory allocation issues

        {
            // Find the reserve coin for this LBPair
            let reserve_coin = reserves.iter().find(|stablecoin| stablecoin.pool_id == lb_pair_key).unwrap();
            
            let (mint_price, redemption_price) = pricing::get_prices(
                reserves, &reserve_token)?;

            // convert prices from f64 to u128 using token decimals
            // msg!("   --> backing decimals: {}", backing_decimals);
            // msg!("    --> reserve coin: {}, mint_price: {}, redemption_price: {}", 
            //     reserve_symbol, mint_price, redemption_price);
            
            // Convert prices to token units (multiply by 10^decimals)
            let backing_decimals = reserve_coin.backing_decimals as i32;
            let mint_price_u128 = (mint_price * 10.0f64.powi(backing_decimals)) as u128;
            let redemption_price_u128 = (redemption_price * 10.0f64.powi(backing_decimals)) as u128;
            let mint_price_u128 = (mint_price_u128 << SCALE_OFFSET)
                                    .checked_div(1_000_000u128)
                                    .ok_or(Error::from(CustomError::PriceConversionError))?;
            let redemption_price_u128 = (redemption_price_u128 << SCALE_OFFSET)
                                    .checked_div(1_000_000u128)
                                    .ok_or(Error::from(CustomError::PriceConversionError))?;
            // msg!("    --> mint price: {}, redemption price: {}", mint_price_u128, redemption_price_u128);

            let lb_pair_state = fetch_lb_pair_state(
                remaining_accounts, 
                &lb_pair_key
            )?;
            let creator = lb_pair_state.creator;
            let bin_step = lb_pair_state.bin_step;
            let min_bin_id = lb_pair_state.parameters.min_bin_id;
            let max_bin_id = lb_pair_state.parameters.max_bin_id;
            // msg!("    --> lb pair bin step: {}, min bin id: {}, max bin id: {}", bin_step, min_bin_id, max_bin_id);
            let mut mint_price_bin_id = match SinglePosition::search_bin_given_price(&lb_pair_state, mint_price_u128) {
                Ok(bin_id) => bin_id,
                Err(_) => {
                    // if out of range, set to max or min bin id
                    if mint_price_u128 < get_price_from_id(0i32, bin_step).unwrap() {
                        min_bin_id
                    } else {
                        max_bin_id
                    }
                }
            };
            let redemption_price_bin_id = match SinglePosition::search_bin_given_price(&lb_pair_state, redemption_price_u128) {
                Ok(bin_id) => bin_id,
                Err(_) => {
                    // if out of range, set to max or min bin id
                    if redemption_price_u128 < get_price_from_id(0i32, bin_step).unwrap() {
                        min_bin_id
                    } else {
                        max_bin_id
                    }
                }
            };
            // msg!("    --> mint price bin id: {}, redemption price bin id: {}", mint_price_bin_id, redemption_price_bin_id);
            // ensure that mint bin id is higher than redemption bin id
            if mint_price_bin_id <= redemption_price_bin_id {
                // adjust mint price bin id by one to ensure they are different
                mint_price_bin_id = redemption_price_bin_id.saturating_add(1);
            }
            
            // check whether out of price range - handle each shift separately to avoid borrowing conflicts
            let needs_mint_shift = mint_price_bin_id != core_position.max_bin_id;
            let needs_redeem_shift = redemption_price_bin_id != core_position.min_bin_id;
            
            // modify core_position in place

            if needs_mint_shift {
                core.shift_mint_position(payer, remaining_accounts, &mut core_position, mint_price_bin_id, boxed_position)?;
                // Only refresh position data - let caller handle rebalance time increment
                core.refresh_position_data(&creator, remaining_accounts, &mut core_position, true)?;
            }

            if needs_redeem_shift {
                core.shift_redeem_position(payer, remaining_accounts, &mut core_position, redemption_price_bin_id, boxed_position)?;
                // Only refresh position data - let caller handle rebalance time increment
                core.refresh_position_data(&creator, remaining_accounts, &mut core_position, false)?;
            }
        }
        
        // Update the core_position back in core
        core.position_data.all_positions[pos_index] = core_position;

        msg!("check_shift_price_ranges called");
        Ok(())
    }

    pub fn init_bitmap_extension<'info>(
        ctx: Context<'_, '_, 'info, 'info, CreateBitmapExtension<'info>>,
        lb_pair: Pubkey,
    ) -> Result<()> {
        let bitmap_extension_acct = &mut ctx.accounts.bitmap_extension;
        // let bitmap_extension: &mut BinArrayBitmapExtension = get_bytemuck_account_ref::<BinArrayBitmapExtension>(
        //     bitmap_extension_acct).ok_or(error!(CustomError::InvalidAccountData))?;
        // bitmap_extension.lb_pair = lb_pair.clone();

        msg!("Initializing bitmap extension for LB pair: {}", lb_pair.clone());
        // the bitmap extension should already be initialized by anchor at this point
        msg!("Bitmap extension account key: {:?}", bitmap_extension_acct.key());
        Ok(())
    }

    /// Helper instruction to ensure Core type is included in IDL
    /// Returns the Core account data for debugging
    // pub fn get_core_data<'a>(ctx: Context<'a, Core>) -> Result<Account<'a, Core>> {
    //     // Simple read operation to include Core type in IDL
    //     let core = &ctx.accounts.core;
    //     Ok(Account::from(*core))
    // }

    /// Helper instruction to force AllPosition type into IDL
    pub fn get_position_info(
        _ctx: Context<Maint>
    ) -> Result<position_manager::AllPosition> {
        // This forces AllPosition to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force SinglePosition type into IDL
    pub fn get_single_position(
        _ctx: Context<Maint>
    ) -> Result<position_manager::SinglePosition> {
        // This forces SinglePosition to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force MintInfo type into IDL
    pub fn get_mint_info(
        _ctx: Context<Maint>
    ) -> Result<position_manager::MintInfo> {
        // This forces MintInfo to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force MintWithProgramId type into IDL
    pub fn get_mint_with_program_id(
        _ctx: Context<Maint>
    ) -> Result<position_manager::MintWithProgramId> {
        // This forces MintWithProgramId to be included in IDL as a return type  
        Err(error!(CustomError::InvalidAmount))
    }

    /// Helper instruction to force TokenEntry type into IDL
    pub fn get_token_entry(
        _ctx: Context<Maint>
    ) -> Result<position_manager::TokenEntry> {
        // This forces TokenEntry to be included in IDL as a return type
        Err(error!(CustomError::InvalidAmount))
    }
}
