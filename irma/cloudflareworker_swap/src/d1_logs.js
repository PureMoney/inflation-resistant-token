
// ==================================================================
// LOGGING UTILITIES (console only — Cloudflare Observability)
// ==================================================================

function generateRequestId() {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

export class Logger {
  constructor(requestId = null) {
    this.requestId = requestId || generateRequestId();
  }

  log(message, context = null) {
    console.log(context ? `[${this.requestId}] ${message}` : `[${this.requestId}] ${message}`, context ?? '');
  }

  warn(message, context = null) {
    console.warn(`[${this.requestId}] ${message}`, context ?? '');
  }

  error(message, context = null) {
    console.error(`[${this.requestId}] ${message}`, context ?? '');
  }

  debug(message, context = null) {
    console.log(`[${this.requestId}] [DEBUG] ${message}`, context ?? '');
  }

  // No-op: retained so callers don't need updating
  async flush() {}
}

export async function logSwapEvent(_db, eventData) {
  console.log('[swap_event]', JSON.stringify(eventData));
}

export async function logPriceUpdate(_db, eventData) {
  console.log('[price_update]', JSON.stringify(eventData));
}

export async function logRebalancingEvent(_db, eventData) {
  console.log('[rebalancing_event]', JSON.stringify(eventData));
}

// Returns null — callers already guard against null
export async function getActiveBins(_db) {
  return null;
}

export async function updateActiveBins(_db, _binsData) {}

export async function queryLogs(_db) {
  return { message: 'Logs are available in Cloudflare Observability dashboard.' };
}
