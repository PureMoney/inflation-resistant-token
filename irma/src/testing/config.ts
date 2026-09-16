import * as fs from "fs";
import * as path from "path";
import type {
  DevnetConfig,
  DisabledPair,
  EnabledPair,
  PoolConfig,
} from "./types";

/** Pairs the harness keeps disabled unless config explicitly sets enabled: true. */
export const DEFAULT_DISABLED_PAIR_KEYS = ["usdc"] as const;

export const DEFAULT_DISABLED_PAIR_REASON: Record<string, string> = {
  usdc:
    "Inherited bad reserve (InvalidLbPairState / stale mint registration). Skip until fixed.",
};

export function findDevnetConfigPath(startDir = process.cwd()): string {
  const fromEnv = process.env.IRMA_DEVNET_CONFIG;
  if (fromEnv) {
    const resolved = path.resolve(fromEnv);
    if (!fs.existsSync(resolved)) {
      throw new Error(`IRMA_DEVNET_CONFIG does not exist: ${resolved}`);
    }
    return resolved;
  }

  const candidates = [
    path.resolve(startDir, "devnet-config.json"),
    path.resolve(startDir, "irma/devnet-config.json"),
  ];

  let dir = path.resolve(startDir);
  for (let i = 0; i < 8; i++) {
    candidates.push(path.join(dir, "devnet-config.json"));
    candidates.push(path.join(dir, "irma", "devnet-config.json"));
    const parent = path.dirname(dir);
    if (parent === dir) {
      break;
    }
    dir = parent;
  }

  const existing = candidates.find((candidate) => fs.existsSync(candidate));
  if (!existing) {
    throw new Error(
      `devnet-config.json not found from ${startDir}. Set IRMA_DEVNET_CONFIG.`
    );
  }
  return existing;
}

export function loadDevnetConfig(configPath?: string): DevnetConfig {
  const resolved = configPath ?? findDevnetConfigPath();
  const parsed = JSON.parse(fs.readFileSync(resolved, "utf-8")) as DevnetConfig;
  assertValidDevnetConfig(parsed, resolved);
  return parsed;
}

export function assertValidDevnetConfig(
  config: DevnetConfig,
  source = "config"
): void {
  if (!config || typeof config !== "object") {
    throw new Error(`${source} is not an object`);
  }
  if (!config.tokens || typeof config.tokens !== "object") {
    throw new Error(`${source} is missing tokens`);
  }
  if (!config.pools || typeof config.pools !== "object") {
    throw new Error(`${source} is missing pools`);
  }
  if (!config.program?.programId) {
    throw new Error(`${source} is missing program.programId`);
  }
  if (!config.tokens.irma) {
    throw new Error(`${source} is missing tokens.irma`);
  }
}

export function isPairEnabled(pairKey: string, pool: PoolConfig): boolean {
  if (typeof pool.enabled === "boolean") {
    return pool.enabled;
  }
  return !DEFAULT_DISABLED_PAIR_KEYS.includes(
    pairKey.toLowerCase() as (typeof DEFAULT_DISABLED_PAIR_KEYS)[number]
  );
}

export function pairDisabledReason(pairKey: string, pool: PoolConfig): string {
  if (pool.disabledReason) {
    return pool.disabledReason;
  }
  return (
    DEFAULT_DISABLED_PAIR_REASON[pairKey.toLowerCase()] ??
    `Pair ${pairKey} is marked disabled`
  );
}

export function listEnabledPairs(config: DevnetConfig): EnabledPair[] {
  return Object.entries(config.pools)
    .filter(([key, pool]) => isPairEnabled(key, pool))
    .map(([key, pool]) => {
      const token = config.tokens[key];
      if (!token) {
        throw new Error(`Pool ${key} has no matching tokens.${key} entry`);
      }
      return { key, token, pool };
    });
}

export function listDisabledPairs(config: DevnetConfig): DisabledPair[] {
  return Object.entries(config.pools)
    .filter(([key, pool]) => !isPairEnabled(key, pool))
    .map(([key, pool]) => ({
      key,
      token: config.tokens[key],
      pool,
      reason: pairDisabledReason(key, pool),
    }));
}

/**
 * Accepts pool keys (`usdt`), token symbols (`USDT`), or token names (`devUSDT`).
 */
export function resolvePairKey(config: DevnetConfig, input: string): string {
  const needle = input.trim().toLowerCase();
  if (!needle) {
    throw new Error("Pair identifier is empty");
  }

  if (config.pools[needle]) {
    return needle;
  }

  for (const [key, token] of Object.entries(config.tokens)) {
    if (key === "irma") {
      continue;
    }
    if (
      token.symbol.toLowerCase() === needle ||
      token.name.toLowerCase() === needle ||
      key.toLowerCase() === needle
    ) {
      if (config.pools[key]) {
        return key;
      }
    }
  }

  throw new Error(`Unknown pair: ${input}`);
}

export function requireEnabledPair(
  config: DevnetConfig,
  input: string
): EnabledPair {
  const key = resolvePairKey(config, input);
  const pool = config.pools[key];
  const token = config.tokens[key];
  if (!token) {
    throw new Error(`Pool ${key} has no matching tokens.${key} entry`);
  }
  if (!isPairEnabled(key, pool)) {
    throw new Error(`Pair ${key} is disabled: ${pairDisabledReason(key, pool)}`);
  }
  return { key, token, pool };
}
