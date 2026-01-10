
// ==================================================================
// LOGGING UTILITIES (D1 Database)
// ==================================================================

// Generate unique request ID for log correlation
function generateRequestId() {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Logger class that writes to both console and D1
export class Logger {
  constructor(db, requestId = null) {
    this.db = db;
    this.requestId = requestId || generateRequestId();
    this.buffer = [];
  }

  log(message, context = null) {
    const timestamp = Date.now();
    console.log(message);
    this.buffer.push({ timestamp, level: 'log', message: String(message), context, requestId: this.requestId });
  }

  warn(message, context = null) {
    const timestamp = Date.now();
    console.warn(message);
    this.buffer.push({ timestamp, level: 'warn', message: String(message), context, requestId: this.requestId });
  }

  error(message, context = null) {
    const timestamp = Date.now();
    console.error(message);
    this.buffer.push({ timestamp, level: 'error', message: String(message), context, requestId: this.requestId });
  }

  debug(message, context = null) {
    const timestamp = Date.now();
    console.log(`[DEBUG] ${message}`);
    this.buffer.push({ timestamp, level: 'debug', message: String(message), context, requestId: this.requestId });
  }

  // Flush all buffered logs to D1 (non-blocking, fire-and-forget)
  // Stores all messages from this request as a single row with JSON array
  async flush() {
    if (!this.db || this.buffer.length === 0) return Promise.resolve();
    
    const logsToFlush = [...this.buffer];
    this.buffer = []; // Clear buffer immediately
    
    // Aggregate all logs into a single row with messages as JSON array
    const firstLog = logsToFlush[0];
    const lastLog = logsToFlush[logsToFlush.length - 1];
    
    // Create messages array with timestamp, level, and message for each log
    const messages = logsToFlush.map(log => ({
      timestamp: log.timestamp,
      level: log.level,
      message: log.message,
      context: log.context
    }));
    
    // Insert single row with all messages
    return this.db.prepare(
      'INSERT INTO console_logs (timestamp, level, message, context, request_id) VALUES (?, ?, ?, ?, ?)'
    ).bind(
      firstLog.timestamp, // Start timestamp
      'batch', // Special level indicating this is a batch
      `${logsToFlush.length} log entries`, // Summary message
      JSON.stringify({ messages, duration: lastLog.timestamp - firstLog.timestamp }), // All messages in context
      firstLog.requestId
    ).run().catch(err => {
      console.error('Failed to flush logs to D1:', err.message);
    });
  }
}

// Event logging functions (non-blocking)
export async function logSwapEvent(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
    INSERT INTO swap_events (
      timestamp, event_type, reserve_symbol, amount_atomic, amount_ui,
      tx_signature, counter_swap_signature, liquidity_signature,
      mint_bin_id, redemption_bin_id, mint_price, redemption_price,
      success, error_message
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).bind(
    eventData.timestamp || Date.now(),
    eventData.eventType,
    eventData.reserveSymbol,
    eventData.amountAtomic,
    eventData.amountUi,
    eventData.txSignature || null,
    eventData.counterSwapSignature || null,
    eventData.liquiditySignature || null,
    eventData.mintBinId || null,
    eventData.redemptionBinId || null,
    eventData.mintPrice || null,
    eventData.redemptionPrice || null,
    eventData.success ? 1 : 0,
    eventData.errorMessage || null
  ).run().catch(err => {
    console.error('Failed to log swap event:', err.message);
  });
}

export async function logPriceUpdate(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
      INSERT INTO price_updates (
        timestamp, inflation_rate, quote_token_price_usd,
        old_mint_price, new_mint_price,
        old_mint_bin_id, new_mint_bin_id,
        old_redemption_bin_id, new_redemption_bin_id,
        tx_signature, trigger_type, success, error_message
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
      eventData.timestamp || Date.now(),
      eventData.inflationRate,
      eventData.quoteTokenPriceUsd,
      eventData.oldMintPrice || null,
      eventData.newMintPrice,
      eventData.oldMintBinId || null,
      eventData.newMintBinId || null,
      eventData.oldRedemptionBinId || null,
      eventData.newRedemptionBinId || null,
      eventData.txSignature || null,
      eventData.triggerType,
      eventData.success ? 1 : 0,
      eventData.errorMessage || null
    ).run().catch(err => {
      console.error('Failed to log price update:', err.message);
    });
}

export async function logRebalancingEvent(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
      INSERT INTO rebalancing_events (
        timestamp, rebalance_type,
        old_mint_bin_id, new_mint_bin_id,
        old_redemption_bin_id, new_redemption_bin_id,
        irma_amount_moved, usdc_amount_moved,
        remove_liquidity_signature, add_liquidity_signature, close_position_signature,
        trigger_type, success, error_message
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
      eventData.timestamp || Date.now(),
      eventData.rebalanceType,
      eventData.oldMintBinId || null,
      eventData.newMintBinId || null,
      eventData.oldRedemptionBinId || null,
      eventData.newRedemptionBinId || null,
      eventData.irmaAmountMoved || null,
      eventData.usdcAmountMoved || null,
      eventData.removeLiquiditySignature || null,
      eventData.addLiquiditySignature || null,
      eventData.closePositionSignature || null,
      eventData.triggerType,
      eventData.success ? 1 : 0,
      eventData.errorMessage || null
    ).run().catch(err => {
      console.error('Failed to log rebalancing event:', err.message);
    });
}

// Active bins management
export async function getActiveBins(db) {
  if (!db) return null;
  try {
    const result = await db.prepare('SELECT * FROM active_bins WHERE id = 1').first();
    return result;
  } catch (err) {
    console.error('Failed to get active bins:', err.message);
    return null;
  }
}

export async function updateActiveBins(db, binsData) {
  if (!db) return Promise.resolve();
  
  // Validate that all required fields are present
  if (binsData.mintBinId === undefined || binsData.mintBinId === null ||
      binsData.redemptionBinId === undefined || binsData.redemptionBinId === null ||
      binsData.mintPrice === undefined || binsData.mintPrice === null ||
      binsData.redemptionPrice === undefined || binsData.redemptionPrice === null) {
    console.error('Failed to update active bins: missing required fields', {
      mintBinId: binsData.mintBinId,
      redemptionBinId: binsData.redemptionBinId,
      mintPrice: binsData.mintPrice,
      redemptionPrice: binsData.redemptionPrice
    });
    return Promise.resolve();
  }
  
  return db.prepare(`
      INSERT INTO active_bins (id, mint_bin_id, redemption_bin_id, mint_price, redemption_price, updated_at)
      VALUES (1, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        mint_bin_id = excluded.mint_bin_id,
        redemption_bin_id = excluded.redemption_bin_id,
        mint_price = excluded.mint_price,
        redemption_price = excluded.redemption_price,
        updated_at = excluded.updated_at
    `).bind(
      binsData.mintBinId,
      binsData.redemptionBinId,
      binsData.mintPrice,
      binsData.redemptionPrice,
      Date.now()
    ).run().catch(err => {
      console.error('Failed to update active bins:', err.message);
    });
}

// Query logs endpoint helper
export async function queryLogs(db, type, limit = 100, offset = 0, expand = false) {
  if (!db) return { error: 'Database not configured' };
  
  try {
    let query, results;
    
    switch (type) {
      case 'console':
        results = await db.prepare(
          'SELECT * FROM console_logs ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        
        // Expand batched messages if requested
        if (expand && results.results) {
          results.results = results.results.flatMap(row => {
            if (row.level === 'batch' && row.context) {
              try {
                const parsed = JSON.parse(row.context);
                return parsed.messages.map(msg => ({
                  ...row,
                  timestamp: msg.timestamp,
                  level: msg.level,
                  message: msg.message,
                  context: msg.context ? JSON.stringify(msg.context) : null,
                  _batch_id: row.request_id
                }));
              } catch (e) {
                return [row];
              }
            }
            return [row];
          });
        }
        break;
      case 'swaps':
        results = await db.prepare(
          'SELECT * FROM swap_events ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'prices':
        results = await db.prepare(
          'SELECT * FROM price_updates ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'rebalancing':
        results = await db.prepare(
          'SELECT * FROM rebalancing_events ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'bins':
        results = await db.prepare('SELECT * FROM active_bins').all();
        break;
      default:
        return { error: 'Unknown log type. Use: console, swaps, prices, rebalancing, bins' };
    }
    
    return { success: true, data: results.results, meta: results.meta };
  } catch (err) {
    return { error: err.message };
  }
}
