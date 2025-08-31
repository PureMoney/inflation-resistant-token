#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Mint};
use std::collections::BTreeMap;
use anchor_lang::solana_program::log::sol_log_compute_units;
use std::str::FromStr;

declare_id!("AG3defYAaYd7xvqX92mnJ4VW1CBxx2A7pesi9u2GNxQd");

// IRMA Crank Bot - basically an automated market maker for the IRMA protocol

#[program]
pub mod crank_irma {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        market_address: Pubkey,
        irma_program_id: Pubkey,
        config: BotConfig,
    ) -> Result<()> {
        let crank_state = &mut ctx.accounts.crank_state;
        
        // Just setting up all the initial state 
        crank_state.authority = ctx.accounts.authority.key();
        crank_state.market_address = market_address;
        crank_state.irma_program_id = irma_program_id;
        crank_state.config = config;
        crank_state.metrics = BotMetrics::new();
        crank_state.bump = ctx.bumps.crank_state;
        crank_state.active = true; // Bot starts enabled by default
        
        msg!("IRMA Crank Bot initialized for market: {}", market_address);
        msg!("Config: {:?}", config);
        
        Ok(())
    }

    // the main crank function that does all the work

    pub fn crank_market(ctx: Context<CrankMarket>) -> Result<()> {
        let crank_state = &mut ctx.accounts.crank_state;
        let current_slot = Clock::get()?.slot;
        
        // Quick sanity check - make sure the bot is actually enabled
        require!(crank_state.active, ErrorCode::BotInactive);
        
        // Smart timing -
        if !crank_state.should_crank(current_slot) {
            msg!("Skipping crank - not enough time elapsed");
            return Ok(());
        }
        
        msg!("Starting crank at slot: {}", current_slot);
        
        // Pull market events from OpenBook 
        let events = process_market_events(&ctx)?;
        let event_count = events.len();
        
        if event_count == 0 {
            msg!("No events to process");
            crank_state.metrics.update_after_crank(current_slot, 0);
            return Ok(()); 
        }
        
        msg!("Processing {} market events", event_count);
        

        let mut total_mints = 0u64;
        let mut total_redeems = 0u64;
        
        for event in events {
            match event.action {
                IrmaAction::Mint { quote_token, amount } => {
                    execute_irma_mint(&ctx, &quote_token, amount)?;
                    total_mints += amount;
                    msg!("[MINT] {} IRMA for {} {}", amount, amount, quote_token);
                }
                IrmaAction::Redeem { quote_token, amount } => {
                    execute_irma_redeem(&ctx, &quote_token, amount)?;
                    total_redeems += amount;
                    msg!("[REDEEM] {} IRMA for {}", amount, quote_token);
                }
                IrmaAction::Skip => {
                   
                    continue;
                }
            }
        }
        
     
        crank_state.metrics.update_after_crank(current_slot, event_count as u16);
        crank_state.metrics.total_mints += total_mints;
        crank_state.metrics.total_redeems += total_redeems;
        crank_state.metrics.total_cranks += 1;
        
        msg!("Crank completed - Mints: {}, Redeems: {}", total_mints, total_redeems);
        
        Ok(())
    }

        ctx: Context<UpdateConfig>,
        new_config: BotConfig,
    ) -> Result<()> {
        let crank_state = &mut ctx.accounts.crank_state;
        
        // Only the original deployer can change settings
        require!(
            ctx.accounts.authority.key() == crank_state.authority,
            ErrorCode::Unauthorized
        );
        
        crank_state.config = new_config;
        
        msg!("Bot configuration updated: {:?}", new_config);
        
        Ok(())
    }


    pub fn set_active(
        ctx: Context<UpdateConfig>,
        active: bool,
    ) -> Result<()> {
        let crank_state = &mut ctx.accounts.crank_state;
        
        // Only admin can flip the switch
        require!(
            ctx.accounts.authority.key() == crank_state.authority,
            ErrorCode::Unauthorized
        );
        
        crank_state.active = active;
        
        msg!("Bot {} {}", if active { "enabled" } else { "disabled" }, crank_state.market_address);
        
        Ok(())
    }


    pub fn get_metrics(ctx: Context<ViewState>) -> Result<BotMetrics> {
        Ok(ctx.accounts.crank_state.metrics.clone())
    }


    pub fn get_irma_prices(
        ctx: Context<CrankMarket>,
        quote_token: String,
    ) -> Result<IrmaPriceInfo> {
        let price_info = get_current_irma_prices(&ctx, &quote_token)?;
        msg!("IRMA Prices for {}: Mint={}, Redeem={}, Spread={}", 
             quote_token, price_info.mint_price, price_info.redemption_price, price_info.spread);
        Ok(price_info)
    }

    pub fn set_irma_mint_price(
        ctx: Context<UpdateConfig>,
        quote_token: String,
        mint_price: f64,
    ) -> Result<()> {
        let crank_state = &ctx.accounts.crank_state;

        require!(
            ctx.accounts.authority.key() == crank_state.authority,
            ErrorCode::Unauthorized
        );
        

        require!(mint_price > 0.0 && mint_price < 100.0, ErrorCode::InvalidAmount);
        
        msg!("Setting {} mint price to {} via IRMA protocol", quote_token, mint_price);
        
 
        let cpi_accounts = irma_protocol::cpi::accounts::Common {
            state: ctx.accounts.irma_state.to_account_info(),
            trader: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        
        let cpi_program = ctx.accounts.irma_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        
        irma_protocol::cpi::set_mint_price(cpi_ctx, &quote_token, mint_price)?;
        
        msg!("✅ Set {} mint price to {}", quote_token, mint_price);
        Ok(())
    }
}

fn process_market_events(ctx: &Context<CrankMarket>) -> Result<Vec<ProcessedEvent>> {
    let mut events = Vec::new();
    
   
    let raw_events = read_openbook_events(ctx)?;
    
    for raw_event in raw_events {
        let processed = match raw_event.event_type {
            1 => {
                // Fill event - someone actually traded
                let taker_side = if raw_event.taker_side == 0 { "bid" } else { "ask" };
                let quantity = raw_event.quantity;
                let price = raw_event.price;
                
                // Get the actual quote token for this market
                let market_quote_mint = get_market_quote_mint(ctx)?;
                let quote_token = determine_quote_token(ctx, &market_quote_mint)?;
                
              
                let action = make_intelligent_decision(ctx, &raw_event, &quote_token).unwrap_or_else(|_| {
                    msg!("Failed to get IRMA prices, falling back to simple logic");
               
                    if taker_side == "ask" {
                        IrmaAction::Mint {
                            quote_token: quote_token.clone(),
                            amount: calculate_irma_amount(quantity, price),
                        }
                    } else {
                        IrmaAction::Redeem {
                            quote_token: quote_token.clone(),
                            amount: calculate_irma_amount(quantity, price),
                        }
                    }
                });
                
                ProcessedEvent {
                    event_type: "fill".to_string(),
                    action,
                    timestamp: Clock::get()?.unix_timestamp,
                }
            }
            0 => {
                // Out event - order cancelled, don't care about these
                ProcessedEvent {
                    event_type: "out".to_string(),
                    action: IrmaAction::Skip,
                    timestamp: Clock::get()?.unix_timestamp,
                }
            }
            _ => {
                // No idea what this is, just ignore it
                ProcessedEvent {
                    event_type: "unknown".to_string(),
                    action: IrmaAction::Skip,
                    timestamp: Clock::get()?.unix_timestamp,
                }
            }
        };
        
        events.push(processed);
    }
    
    Ok(events)
}

// Call the IRMA program to mint new tokens
fn execute_irma_mint(
    ctx: &Context<CrankMarket>,
    quote_token: &str,
    amount: u64,
) -> Result<()> {
    let crank_state = &ctx.accounts.crank_state;
    

    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(
        amount <= crank_state.config.max_irma_per_crank,
        ErrorCode::AmountTooLarge
    );
    
    msg!("Executing IRMA mint: {} {} tokens", amount, quote_token);
    
   
    let cpi_accounts = irma_protocol::cpi::accounts::Common {
        state: ctx.accounts.irma_state.to_account_info(),
        trader: ctx.accounts.cranker.to_account_info(), // Bot acts as trader
        system_program: ctx.accounts.system_program.to_account_info(),
    };
    
    let cpi_program = ctx.accounts.irma_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

    irma_protocol::cpi::mint_irma(cpi_ctx, quote_token, amount)?;
    
    msg!("✅ IRMA Mint successful: {} {} → IRMA tokens", amount, quote_token);
    
    Ok(())
}

// Call the IRMA program to redeem/burn tokens
fn execute_irma_redeem(
    ctx: &Context<CrankMarket>,
    quote_token: &str,
    amount: u64,
) -> Result<()> {
    let crank_state = &ctx.accounts.crank_state;
    

    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(
        amount <= crank_state.config.max_irma_per_crank,
        ErrorCode::AmountTooLarge
    );
    
    msg!("Executing IRMA redeem: {} IRMA for {}", amount, quote_token);
    

    let cpi_accounts = irma_protocol::cpi::accounts::Common {
        state: ctx.accounts.irma_state.to_account_info(),
        trader: ctx.accounts.cranker.to_account_info(), // Bot acts as trader
        system_program: ctx.accounts.system_program.to_account_info(),
    };
    
    let cpi_program = ctx.accounts.irma_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    

    irma_protocol::cpi::redeem_irma(cpi_ctx, quote_token, amount)?;
    
    msg!("✅ IRMA Redeem successful: {} IRMA → {} tokens", amount, quote_token);
    
    Ok(())
}

// Read and parse real events from OpenBook's event heap
fn read_openbook_events(ctx: &Context<CrankMarket>) -> Result<Vec<RawMarketEvent>> {
    let mut events = Vec::new();
    
    let event_heap_account = &ctx.accounts.event_heap;
    let event_heap_data = event_heap_account.try_borrow_data()?;
    
    if event_heap_data.len() < 16 {
        msg!("Event heap data too small");
        return Ok(events);
    }
    
    // EventHeapHeader structure from IDL:
    let used_head = u16::from_le_bytes([event_heap_data[2], event_heap_data[3]]);
    let count = u16::from_le_bytes([event_heap_data[4], event_heap_data[5]]);
    
    msg!("EventHeap: used_head={}, count={}", used_head, count);
    
    if count == 0 {
        msg!("No events in heap");
        return Ok(events);
    }
    
    // Calculate offset to event nodes (header is 16 bytes)
    let header_size = 16;
    let node_size = 152; 

    let max_events = std::cmp::min(count as usize, 10);
    let mut current_node = used_head;
    
    for i in 0..max_events {
        let node_offset = header_size + (current_node as usize * node_size);
        
        if node_offset + node_size > event_heap_data.len() {
            msg!("Node offset {} exceeds data length {}", node_offset, event_heap_data.len());
            break;
        }
        
        // Parse the EventNode
        let next_node = u16::from_le_bytes([
            event_heap_data[node_offset], 
            event_heap_data[node_offset + 1]
        ]);
        

        let event_offset = node_offset + 8;
        let event_type = event_heap_data[event_offset]; // First byte is event type
        
        match event_type {
            0 => {
                // OutEvent - order cancellation/removal
                msg!("Parsing OutEvent at node {}", i);
                
                if let Some(out_event) = parse_out_event(&event_heap_data[event_offset..]) {
                    events.push(RawMarketEvent {
                        event_type: 0,
                        taker_side: out_event.side,
                        quantity: out_event.quantity as u64,
                        price: 0, // Out events don't have price
                        timestamp: out_event.timestamp,
                    });
                }
            }
            1 => {
                // FillEvent - actual trade
                msg!("Parsing FillEvent at node {}", i);
                
                if let Some(fill_event) = parse_fill_event(&event_heap_data[event_offset..]) {
                    events.push(RawMarketEvent {
                        event_type: 1,
                        taker_side: fill_event.taker_side,
                        quantity: fill_event.quantity as u64,
                        price: fill_event.price as u64,
                        timestamp: fill_event.timestamp,
                    });
                }
            }
            _ => {
                msg!("Unknown event type: {}", event_type);
            }
        }
        
        // Move to next node in the linked list
        current_node = next_node;
        if current_node == used_head {
            // We've gone full circle
            break;
        }
    }
    
    msg!("Parsed {} events from OpenBook EventHeap", events.len());
    sol_log_compute_units();
    
    Ok(events)
}

// Parse FillEvent from raw bytes (based on OpenBook V2 IDL structure)
fn parse_fill_event(data: &[u8]) -> Option<ParsedFillEvent> {
    if data.len() < 144 { // FillEvent is 144 bytes total
        return None;
    }
    

    
    let taker_side = data[1];
    let timestamp = i64::from_le_bytes([
        data[8], data[9], data[10], data[11], 
        data[12], data[13], data[14], data[15]
    ]);
    
    // Price is at offset 80 (after pubkeys and other fields)
    let price = i64::from_le_bytes([
        data[80], data[81], data[82], data[83],
        data[84], data[85], data[86], data[87]
    ]);
    
    // Quantity is at offset 96
    let quantity = i64::from_le_bytes([
        data[96], data[97], data[98], data[99],
        data[100], data[101], data[102], data[103]
    ]);
    
    Some(ParsedFillEvent {
        taker_side,
        timestamp,
        price,
        quantity,
    })
}

// Parse OutEvent from raw bytes  
fn parse_out_event(data: &[u8]) -> Option<ParsedOutEvent> {
    if data.len() < 144 {
        return None;
    }

    
    let side = data[1];
    let timestamp = i64::from_le_bytes([
        data[8], data[9], data[10], data[11],
        data[12], data[13], data[14], data[15]
    ]);
    
    // Quantity is at offset 48 (after owner pubkey)
    let quantity = i64::from_le_bytes([
        data[48], data[49], data[50], data[51],
        data[52], data[53], data[54], data[55]
    ]);
    
    Some(ParsedOutEvent {
        side,
        timestamp,
        quantity,
    })
}

// Helper structures for parsing
#[derive(Debug)]
struct ParsedFillEvent {
    taker_side: u8,
    timestamp: i64,
    price: i64,
    quantity: i64,
}

#[derive(Debug)]
struct ParsedOutEvent {
    side: u8,
    timestamp: i64,
    quantity: i64,
}

fn simulate_market_events(_ctx: &Context<CrankMarket>) -> Result<Vec<RawMarketEvent>> {

    Ok(vec![
        RawMarketEvent {
            event_type: 1, // Fill event
            taker_side: 1, // Ask (someone sold)
            quantity: 1000,
            price: 105, // 1.05 with 2 decimal precision
            timestamp: Clock::get()?.unix_timestamp,
        },
        RawMarketEvent {
            event_type: 1, // Fill event  
            taker_side: 0, // Bid (someone bought)
            quantity: 500,
            price: 104, // 1.04 with 2 decimal precision
            timestamp: Clock::get()?.unix_timestamp,
        },
    ])
}

fn determine_quote_token(ctx: &Context<CrankMarket>, market_mint: &Pubkey) -> Result<String> {
  
    
    let cpi_accounts = irma_protocol::cpi::accounts::Common {
        state: ctx.accounts.irma_state.to_account_info(),
        trader: ctx.accounts.cranker.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
    };
    
    let cpi_program = ctx.accounts.irma_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
 
    match irma_protocol::cpi::get_stablecoin_symbol(cpi_ctx, *market_mint) {
        Ok(Some(symbol)) => {
            msg!("Found quote token: {} for mint: {}", symbol, market_mint);
            Ok(symbol)
        }
        Ok(None) => {
            msg!("Mint address {} not found in IRMA reserves, defaulting to USDT", market_mint);
            Ok("USDT".to_string()) // Fallback to USDT
        }
        Err(_) => {
            msg!("Error querying IRMA state, defaulting to USDT");
            Ok("USDT".to_string()) // Fallback to USDT
        }
    }
}

fn get_market_quote_mint(ctx: &Context<CrankMarket>) -> Result<Pubkey> {

    
    let market_account = &ctx.accounts.market;
    let market_data = market_account.try_borrow_data()?;
    

    if market_data.len() >= 250 {
        let quote_mint_bytes = &market_data[200..232]; // 32 bytes for Pubkey
        let quote_mint = Pubkey::try_from(quote_mint_bytes)
            .map_err(|_| ErrorCode::InvalidMarketAddress)?;
        Ok(quote_mint)
    } else {

        Ok(Pubkey::from_str("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p").unwrap())
    }
}

fn get_current_irma_prices(ctx: &Context<CrankMarket>, quote_token: &str) -> Result<IrmaPriceInfo> {
    let cpi_accounts = irma_protocol::cpi::accounts::Common {
        state: ctx.accounts.irma_state.to_account_info(),
        trader: ctx.accounts.cranker.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
    };
    
    let cpi_program = ctx.accounts.irma_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    
    // Get reserve info to calculate prices
    let reserve_info = irma_protocol::cpi::get_reserve_info(cpi_ctx, quote_token)?;
    
    let mint_price = reserve_info.mint_price;
    let redemption_price = if reserve_info.irma_in_circulation > 0 {
        reserve_info.backing_reserves as f64 / reserve_info.irma_in_circulation as f64
    } else {
        1.0 // Default if no circulation
    };
    
    let spread = mint_price - redemption_price;
    
    Ok(IrmaPriceInfo {
        mint_price,
        redemption_price,
        spread,
        backing_reserves: reserve_info.backing_reserves,
        irma_circulation: reserve_info.irma_in_circulation,
    })
}

fn make_intelligent_decision(
    ctx: &Context<CrankMarket>,
    market_event: &RawMarketEvent,
    quote_token: &str,
) -> Result<IrmaAction> {
    let irma_prices = get_current_irma_prices(ctx, quote_token)?;
    let market_price = market_event.price as f64 / 100.0; // Assuming 2 decimal precision
    
    msg!("Market Decision: price={}, mint_price={}, redeem_price={}, spread={}", 
         market_price, irma_prices.mint_price, irma_prices.redemption_price, irma_prices.spread);
    
    let action = if market_event.taker_side == 1 {
        // ASK (someone sold) - market pressure downward
        if market_price < irma_prices.redemption_price * 1.01 {
            // Market price below redemption price - great arbitrage opportunity
            IrmaAction::Mint {
                quote_token: quote_token.to_string(),
                amount: calculate_intelligent_amount(&irma_prices, market_event, true),
            }
        } else if irma_prices.spread > 0.02 {
            // Large spread available - mint to capture it
            IrmaAction::Mint {
                quote_token: quote_token.to_string(),
                amount: calculate_intelligent_amount(&irma_prices, market_event, true),
            }
        } else {
            IrmaAction::Skip // Small opportunity, skip to save costs
        }
    } else {
        if market_price > irma_prices.mint_price * 0.99 {
            // Market price above mint price - redeem opportunity
            IrmaAction::Redeem {
                quote_token: quote_token.to_string(),
                amount: calculate_intelligent_amount(&irma_prices, market_event, false),
            }
        } else if irma_prices.spread > 0.02 {
            // Large spread - redeem some IRMA
            IrmaAction::Redeem {
                quote_token: quote_token.to_string(),
                amount: calculate_intelligent_amount(&irma_prices, market_event, false),
            }
        } else {
            IrmaAction::Skip // Not profitable enough
        }
    };
    
    Ok(action)
}

fn calculate_intelligent_amount(irma_prices: &IrmaPriceInfo, market_event: &RawMarketEvent, is_mint: bool) -> u64 {
    let market_value = (market_event.quantity * market_event.price) / 100; // Basic value calculation
    
    let spread_multiplier = if irma_prices.spread > 0.05 {
        2.0 // Large spread - double the amount
    } else if irma_prices.spread > 0.02 {
        1.5 // Medium spread - 1.5x amount
    } else {
        1.0 // Small spread - normal amount
    };
    
    let base_amount = if is_mint {
        (market_value as f64 * spread_multiplier) as u64
    } else {
        (market_value as f64 * spread_multiplier * 0.8) as u64
    };
    
    // Safety limits
    std::cmp::min(base_amount, 50_000) // Max 50k per operation
}

fn calculate_irma_amount(quantity: u64, price: u64) -> u64 {

    (quantity * price) / 100 
}


#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + CrankState::LEN,
        seeds = [b"crank_state", market_address.as_ref()],
        bump
    )]
    pub crank_state: Account<'info, CrankState>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub market_address: UncheckedAccount<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CrankMarket<'info> {
    #[account(
        mut,
        seeds = [b"crank_state", crank_state.market_address.as_ref()],
        bump = crank_state.bump
    )]
    pub crank_state: Account<'info, CrankState>,
    
    pub market: UncheckedAccount<'info>,
    
    pub event_heap: UncheckedAccount<'info>,
    
    pub irma_state: UncheckedAccount<'info>,
    
    pub irma_program: UncheckedAccount<'info>,
    

    #[account(address = openbook_v2::ID)]
    pub openbook_program: Program<'info, openbook_v2::OpenBookV2>,
    
    #[account(mut)]
    pub cranker: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(
        mut,
        seeds = [b"crank_state", crank_state.market_address.as_ref()],
        bump = crank_state.bump
    )]
    pub crank_state: Account<'info, CrankState>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    /// CHECK: IRMA program for CPI calls
    pub irma_program: UncheckedAccount<'info>,
    
    /// CHECK: IRMA state account
    pub irma_state: UncheckedAccount<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ViewState<'info> {
    #[account(
        seeds = [b"crank_state", crank_state.market_address.as_ref()],
        bump = crank_state.bump
    )]
    pub crank_state: Account<'info, CrankState>,
}

/// State Structures
#[account]
pub struct CrankState {
    pub authority: Pubkey,              // Admin authority
    pub market_address: Pubkey,         // OpenBook market being monitored
    pub irma_program_id: Pubkey,        // IRMA protocol program ID
    pub config: BotConfig,              // Bot configuration
    pub metrics: BotMetrics,            // Performance metrics
    pub active: bool,                   // Bot enabled/disabled
    pub bump: u8,                       // PDA bump seed
}

impl CrankState {
    pub const LEN: usize = 32 + // authority
        32 + // market_address
        32 + // irma_program_id
        BotConfig::LEN +
        BotMetrics::LEN +
        1 + // active
        1; // bump
        
    // Smart timing logic - crank more often when there's lots of activity
    pub fn should_crank(&self, current_slot: u64) -> bool {
        let slots_since_last = current_slot.saturating_sub(self.metrics.last_crank_slot);
        
        // Look at recent activity to decide how often to check
        let recent_activity = self.metrics.recent_activity_level();
        
        // More activity = faster cranking to catch trades quickly
        let required_interval = if recent_activity > self.config.high_activity_threshold {
            1 // Lots happening: check every slot (~400ms)
        } else if recent_activity > self.config.medium_activity_threshold {
            self.config.check_interval_medium // Some activity: check every few slots
        } else {
            self.config.check_interval_low // Quiet: check every 100 slots (~40 seconds)
        };
        
        slots_since_last >= required_interval
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct BotConfig {
    pub check_interval_low: u64,         // Slots to wait during low activity
    pub check_interval_medium: u64,      // Slots to wait during medium activity
    pub high_activity_threshold: u16,    // Event count threshold for high activity
    pub medium_activity_threshold: u16,  // Event count threshold for medium activity
    pub max_irma_per_crank: u64,        // Maximum IRMA to mint/redeem per call
    pub min_trade_amount: u64,          // Minimum trade size to act on
    pub slot_duration_ms: u64,          // Estimated slot duration for timing
}

impl BotConfig {
    pub const LEN: usize = 8 + // check_interval_low
        8 + // check_interval_medium
        2 + // high_activity_threshold
        2 + // medium_activity_threshold
        8 + // max_irma_per_crank
        8 + // min_trade_amount
        8; // slot_duration_ms
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            check_interval_low: 100,        // When quiet: wait 100 slots (~40 seconds)
            check_interval_medium: 10,      // Some activity: wait 10 slots (~4 seconds)  
            high_activity_threshold: 10,    // 10+ events = busy market
            medium_activity_threshold: 3,   // 3+ events = moderate activity
            max_irma_per_crank: 100_000,   // Don't mint/redeem more than 100k IRMA at once
            min_trade_amount: 1,           // Ignore tiny trades
            slot_duration_ms: 400,         // Solana slots are roughly 400ms each
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct BotMetrics {
    pub last_crank_slot: u64,           // Last slot when crank was executed
    pub total_cranks: u64,              // Total number of crank calls
    pub total_mints: u64,               // Total IRMA minted
    pub total_redeems: u64,             // Total IRMA redeemed
    pub event_count_history: Vec<u16>,  // Recent event counts (last 10 cranks)
    pub last_update: i64,               
}

impl BotMetrics {
    pub const LEN: usize = 8 + // last_crank_slot
        8 + // total_cranks
        8 + // total_mints
        8 + // total_redeems
        4 + (2 * 10) + // event_count_history (max 10 entries)
        8; // last_update
        
    pub fn new() -> Self {
        Self {
            last_crank_slot: 0,
            total_cranks: 0,
            total_mints: 0,
            total_redeems: 0,
            event_count_history: Vec::new(),
            last_update: 0,
        }
    }
    
    pub fn update_after_crank(&mut self, slot: u64, event_count: u16) {
        self.last_crank_slot = slot;
        self.last_update = Clock::get().unwrap().unix_timestamp;
        
        // Add to history and keep only last 10 entries
        self.event_count_history.push(event_count);
        if self.event_count_history.len() > 10 {
            self.event_count_history.remove(0);
        }
    }
    
    pub fn recent_activity_level(&self) -> u16 {
        if self.event_count_history.is_empty() {
            return 0;
        }
        
        // Average events per crank over recent history
        let sum: u32 = self.event_count_history.iter().map(|&x| x as u32).sum();
        (sum / self.event_count_history.len() as u32) as u16
    }
}

/// Event Processing Structures
#[derive(Debug, Clone)]
pub struct RawMarketEvent {
    pub event_type: u8,        // 0 = out, 1 = fill
    pub taker_side: u8,        // 0 = bid, 1 = ask
    pub quantity: u64,      
    pub price: u64,      
    pub timestamp: i64,       
}

#[derive(Debug, Clone)]
pub struct ProcessedEvent {
    pub event_type: String,   // "fill", "out", "unknown"
    pub action: IrmaAction,   // Action to take
    pub timestamp: i64,       // Processing timestamp
}

#[derive(Debug, Clone)]
pub enum IrmaAction {
    Mint { quote_token: String, amount: u64 },
    Redeem { quote_token: String, amount: u64 },
    Skip,
}

/// IRMA pricing information for intelligent decision making
#[derive(Debug, Clone)]
pub struct IrmaPriceInfo {
    pub mint_price: f64,           // Pm - price to mint IRMA
    pub redemption_price: f64,     // Pr - current redemption value  
    pub spread: f64,               // Pm - Pr (profit opportunity)
    pub backing_reserves: u64,     // Total backing for this token
    pub irma_circulation: u64,     // IRMA backed by this token
}

/// Error Codes
#[error_code]
pub enum ErrorCode {
    #[msg("Bot is currently inactive")]
    BotInactive,
    
    #[msg("Unauthorized: only admin can perform this action")]
    Unauthorized,
    
    #[msg("Invalid amount: must be greater than 0")]
    InvalidAmount,
    
    #[msg("Amount too large: exceeds maximum per crank")]
    AmountTooLarge,
    
    #[msg("Market event processing failed")]
    EventProcessingFailed,
    
    #[msg("IRMA operation failed")]
    IrmaOperationFailed,
    
    #[msg("Invalid market address")]
    InvalidMarketAddress,
    
    #[msg("Configuration error")]
    ConfigurationError,
}
