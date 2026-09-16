import { expect } from "chai";
import * as path from "path";
import {
  findDevnetConfigPath,
  listDisabledPairs,
  listEnabledPairs,
  loadDevnetConfig,
  requireEnabledPair,
  resolvePairKey,
  isPairEnabled,
} from "./config";
import type { DevnetConfig } from "./types";

function fixtureConfig(overrides?: Partial<DevnetConfig["pools"]["usdc"]>): DevnetConfig {
  const config = loadDevnetConfig(findDevnetConfigPath());
  if (overrides) {
    config.pools.usdc = { ...config.pools.usdc, ...overrides };
  }
  return config;
}

describe("B1/B2 config loader", () => {
  it("loads irma/devnet-config.json without network access", () => {
    const config = loadDevnetConfig(findDevnetConfigPath());
    expect(config.network).to.equal("devnet");
    expect(config.program.programId).to.be.a("string").and.not.empty;
    expect(config.tokens.irma.symbol).to.equal("IRMA");
  });

  it("marks USDC disabled and skips it from enabled pairs", () => {
    const config = loadDevnetConfig(findDevnetConfigPath());
    expect(isPairEnabled("usdc", config.pools.usdc)).to.equal(false);
    const enabled = listEnabledPairs(config).map((pair) => pair.key);
    const disabled = listDisabledPairs(config);
    expect(enabled).to.not.include("usdc");
    expect(enabled).to.include.members(["usdt", "pyusd", "usds", "usdg", "fdusd"]);
    expect(disabled.map((pair) => pair.key)).to.include("usdc");
    expect(disabled[0].reason).to.match(/bad reserve/i);
  });

  it("keeps USDC disabled when the enabled flag is omitted", () => {
    const config = fixtureConfig();
    delete config.pools.usdc.enabled;
    expect(isPairEnabled("usdc", config.pools.usdc)).to.equal(false);
    expect(() => requireEnabledPair(config, "usdc")).to.throw(/disabled/i);
  });

  it("resolves pair aliases and rejects unknown pairs", () => {
    const config = loadDevnetConfig(findDevnetConfigPath());
    expect(resolvePairKey(config, "USDT")).to.equal("usdt");
    expect(resolvePairKey(config, "devUSDT")).to.equal("usdt");
    expect(() => resolvePairKey(config, "not-a-pair")).to.throw(/Unknown pair/);
  });
});
