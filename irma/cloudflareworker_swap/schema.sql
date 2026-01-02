-- IRMA Worker D1 Database Schema
-- This schema stores all worker logs, events, and bin tracking data

-- Console logs table - stores all console.log outputs
CREATE TABLE IF NOT EXISTS console_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    level TEXT NOT NULL DEFAULT 'log',  -- log, warn, error, debug
    message TEXT NOT NULL,
    context TEXT,  -- JSON string with additional context (request_id, etc.)
    request_id TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Index for querying logs by timestamp and level
CREATE INDEX IF NOT EXISTS idx_console_logs_timestamp ON console_logs(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_console_logs_level ON console_logs(level);
CREATE INDEX IF NOT EXISTS idx_console_logs_request_id ON console_logs(request_id);

-- Swap events table - structured swap event data
CREATE TABLE IF NOT EXISTS swap_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    event_type TEXT NOT NULL,  -- MINT, REDEMPTION
    reserve_symbol TEXT NOT NULL,
    amount_atomic TEXT NOT NULL,  -- stored as string for large numbers
    amount_ui REAL NOT NULL,
    tx_signature TEXT,
    counter_swap_signature TEXT,
    liquidity_signature TEXT,
    mint_bin_id INTEGER,
    redemption_bin_id INTEGER,
    mint_price REAL,
    redemption_price REAL,
    success INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_swap_events_timestamp ON swap_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_swap_events_type ON swap_events(event_type);

-- Price update events table
CREATE TABLE IF NOT EXISTS price_updates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    inflation_rate REAL NOT NULL,
    quote_token_price_usd REAL NOT NULL,
    old_mint_price REAL,
    new_mint_price REAL NOT NULL,
    old_mint_bin_id INTEGER,
    new_mint_bin_id INTEGER,
    old_redemption_bin_id INTEGER,
    new_redemption_bin_id INTEGER,
    tx_signature TEXT,
    trigger_type TEXT NOT NULL,  -- scheduled, manual
    success INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_price_updates_timestamp ON price_updates(timestamp DESC);

-- Active bins table - tracks current active mint/redemption bins
CREATE TABLE IF NOT EXISTS active_bins (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Only one row allowed
    mint_bin_id INTEGER NOT NULL,
    redemption_bin_id INTEGER NOT NULL,
    mint_price REAL NOT NULL,
    redemption_price REAL NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Rebalancing events table
CREATE TABLE IF NOT EXISTS rebalancing_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    rebalance_type TEXT NOT NULL,  -- mint_bin, redemption_bin, both
    old_mint_bin_id INTEGER,
    new_mint_bin_id INTEGER,
    old_redemption_bin_id INTEGER,
    new_redemption_bin_id INTEGER,
    irma_amount_moved TEXT,  -- atomic amount as string
    usdc_amount_moved TEXT,  -- atomic amount as string
    remove_liquidity_signature TEXT,
    add_liquidity_signature TEXT,
    close_position_signature TEXT,
    trigger_type TEXT NOT NULL,  -- auto, manual
    success INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_rebalancing_events_timestamp ON rebalancing_events(timestamp DESC);

-- Positions table - tracks positions we've created/modified
CREATE TABLE IF NOT EXISTS positions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    position_pubkey TEXT NOT NULL UNIQUE,
    lower_bin_id INTEGER NOT NULL,
    upper_bin_id INTEGER NOT NULL,
    position_type TEXT NOT NULL,  -- mint, redemption
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_positions_type ON positions(position_type);
CREATE INDEX IF NOT EXISTS idx_positions_active ON positions(is_active);
