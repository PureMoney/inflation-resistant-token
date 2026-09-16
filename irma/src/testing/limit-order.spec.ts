import { expect } from "chai";
import { Keypair, PublicKey } from "@solana/web3.js";
import { findDevnetConfigPath, loadDevnetConfig } from "./config";
import { createBSeriesHarness } from "./harness";
import {
  planCancelLimitOrder,
  planCloseLimitOrder,
  planFillLimitOrder,
  planPlaceLimitOrder,
  refuseLiveExecution,
} from "./limit-order";
import {
  binIdToBinArrayIndex,
  deriveBinArrayPda,
  deriveEventAuthorityPda,
  deriveIrmaPdas,
  loadDlmmProgramId,
} from "./pdas";

describe("limit-order planning and dual-persona harness", () => {
  const config = loadDevnetConfig(findDevnetConfigPath());
  const lp = Keypair.generate();
  const swapOnly = Keypair.generate();

  it("derives PDAs offline using the same seeds as test_limit_order.ts", () => {
    expect(binIdToBinArrayIndex(0)).to.equal(0);
    expect(binIdToBinArrayIndex(69)).to.equal(0);
    expect(binIdToBinArrayIndex(70)).to.equal(1);
    const dlmm = loadDlmmProgramId();
    expect(dlmm).to.equal("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
    const pda = deriveBinArrayPda(config.pools.usdt.address, 11, dlmm);
    expect(pda).to.be.instanceOf(PublicKey);
    const eventAuthority = deriveEventAuthorityPda(dlmm);
    expect(eventAuthority.toBase58()).to.be.a("string");
    const { statePda, corePda } = deriveIrmaPdas(config.program.programId);
    expect(statePda.toBase58()).to.not.equal(corePda.toBase58());
  });

  it("plans place/fill/cancel/close without an admin keypair or IRMA IDL", () => {
    const place = planPlaceLimitOrder(config, {
      pair: "devUSDT",
      side: "ask",
      amount: 0.1,
      actor: "lp",
      owner: lp.publicKey.toBase58(),
      activeBinId: 0,
    });
    expect(place.irmaInstruction).to.equal("placeLimitOrder");
    expect(place.opensPositionAccount).to.equal(false);
    expect(place.args.binId).to.equal(10);
    expect(place.accounts.some((account) => account.role === "reserve" && account.kind === "deferred")).to.equal(true);

    const fill = planFillLimitOrder(config, {
      pair: "usdt",
      actor: "swap-only",
      swapper: swapOnly.publicKey.toBase58(),
      side: "ask",
      amount: 0.1,
    });
    expect(fill.irmaInstruction).to.equal(null);
    expect(fill.opensPositionAccount).to.equal(false);
    expect(fill.args.method).to.equal("counterparty-swap");

    const cancel = planCancelLimitOrder(config, {
      pair: "usdt",
      actor: "lp",
      owner: lp.publicKey.toBase58(),
      limitOrder: lp.publicKey.toBase58(),
      binIds: [10],
    });
    expect(cancel.irmaInstruction).to.equal("cancelLimitOrder");

    const close = planCloseLimitOrder(config, {
      actor: "lp",
      owner: lp.publicKey.toBase58(),
      pair: "usdt",
      limitOrder: lp.publicKey.toBase58(),
    });
    expect(close.irmaInstruction).to.equal("closeLimitOrderIfEmpty");
  });

  it("refuses USDC and swap-only place attempts at plan time", () => {
    expect(() =>
      planPlaceLimitOrder(config, {
        pair: "usdc",
        side: "ask",
        amount: 0.1,
        actor: "lp",
        owner: lp.publicKey.toBase58(),
      })
    ).to.throw(/disabled/i);

    expect(() =>
      planPlaceLimitOrder(config, {
        pair: "usdt",
        side: "ask",
        amount: 0.1,
        actor: "swap-only",
        owner: swapOnly.publicKey.toBase58(),
      })
    ).to.throw(/cannot perform placeLimitOrder/);
  });

  it("builds a dry-run dual-persona scenario across enabled pairs only", () => {
    const harness = createBSeriesHarness({
      lp: { type: "publicKey", publicKey: lp.publicKey.toBase58() },
      swapOnly: { type: "publicKey", publicKey: swapOnly.publicKey.toBase58() },
    });

    expect(harness.enabledPairs.map((pair) => pair.key)).to.not.include("usdc");
    expect(harness.disabledPairs.map((pair) => pair.key)).to.include("usdc");

    const scenario = harness.planDualPersonaScenario({ amount: 0.25, activeBinId: 0 });
    expect(scenario.mode).to.equal("dry-run");
    expect(scenario.dependsOnPullRequest).to.equal(147);
    expect(scenario.plans.map((plan) => plan.pairKey)).to.not.include("usdc");
    expect(scenario.plans.length).to.equal(harness.enabledPairs.length);

    for (const plan of scenario.plans) {
      expect(plan.mode).to.equal("dry-run");
      expect(plan.steps.map((step) => step.op)).to.deep.equal([
        "place",
        "fill",
        "cancel",
        "close",
      ]);
      const fill = plan.steps.find((step) => step.op === "fill");
      expect(fill?.actor).to.equal("swap-only");
      expect(fill?.opensPositionAccount).to.equal(false);
      expect(plan.requiresAdminWalletForLiveExecution).to.equal(true);
    }

    expect(JSON.stringify(scenario)).to.not.include("secretKey");
    expect(() => harness.executeLive(scenario.plans[0])).to.throw(/Live scenario execution is disabled/);
    expect(() => refuseLiveExecution("place")).to.throw(/PR #147/);
  });
});
