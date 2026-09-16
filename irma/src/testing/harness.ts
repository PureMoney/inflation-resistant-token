import {
  listDisabledPairs,
  listEnabledPairs,
  loadDevnetConfig,
  requireEnabledPair,
} from "./config";
import {
  planEnabledPairLifecycles,
  planLimitOrderLifecycle,
  refuseLiveExecution,
} from "./limit-order";
import { dualPersonas } from "./personas";
import type { DualPersonaScenario, LimitOrderPlan, LimitOrderSide } from "./types";
import { LIVE_EXECUTION_PREREQUISITES } from "./types";
import {
  assertNoEmbeddedSecrets,
  LP_WALLET_ENV,
  resolveWallet,
  SWAP_ONLY_WALLET_ENV,
  walletSourceFromEnv,
  type WalletSource,
} from "./wallets";

export type HarnessOptions = {
  configPath?: string;
  lp: WalletSource;
  swapOnly: WalletSource;
  env?: NodeJS.Dict<string>;
};

export type BSeriesHarness = {
  configPath?: string;
  personas: ReturnType<typeof dualPersonas>;
  enabledPairs: ReturnType<typeof listEnabledPairs>;
  disabledPairs: ReturnType<typeof listDisabledPairs>;
  planPair(pair: string, options?: PairPlanOptions): LimitOrderPlan;
  planEnabledPairs(options?: PairPlanOptions): LimitOrderPlan[];
  planDualPersonaScenario(options?: PairPlanOptions): DualPersonaScenario;
  executeLive(plan: LimitOrderPlan): never;
};

export type PairPlanOptions = {
  side?: LimitOrderSide;
  amount?: number;
  binId?: number;
  activeBinId?: number;
  limitOrder?: string;
  pairs?: string[];
};

export function createBSeriesHarness(options: HarnessOptions): BSeriesHarness {
  const env = options.env ?? process.env;
  const config = loadDevnetConfig(options.configPath);
  const personas = dualPersonas(
    resolveWallet(options.lp, env),
    resolveWallet(options.swapOnly, env)
  );
  const enabledPairs = listEnabledPairs(config);
  const disabledPairs = listDisabledPairs(config);

  const defaults: Required<Pick<PairPlanOptions, "side" | "amount">> = {
    side: "ask",
    amount: 0.1,
  };

  function planPair(pair: string, planOptions: PairPlanOptions = {}): LimitOrderPlan {
    requireEnabledPair(config, pair);
    return planLimitOrderLifecycle(config, {
      pair,
      side: planOptions.side ?? defaults.side,
      amount: planOptions.amount ?? defaults.amount,
      lp: personas.lp.publicKey,
      swapOnly: personas.swapOnly.publicKey,
      bin: planOptions.binId
        ? { kind: "absolute", binId: planOptions.binId }
        : undefined,
      activeBinId: planOptions.activeBinId,
      limitOrder: planOptions.limitOrder,
    });
  }

  function planEnabled(planOptions: PairPlanOptions = {}): LimitOrderPlan[] {
    return planEnabledPairLifecycles(config, {
      side: planOptions.side ?? defaults.side,
      amount: planOptions.amount ?? defaults.amount,
      lp: personas.lp.publicKey,
      swapOnly: personas.swapOnly.publicKey,
      bin: planOptions.binId
        ? { kind: "absolute", binId: planOptions.binId }
        : undefined,
      activeBinId: planOptions.activeBinId,
      limitOrder: planOptions.limitOrder,
      pairs: planOptions.pairs,
    });
  }

  function planDualPersonaScenario(
    planOptions: PairPlanOptions = {}
  ): DualPersonaScenario {
    const plans = planEnabled(planOptions);
    const scenario: DualPersonaScenario = {
      mode: "dry-run",
      lp: personas.lp,
      swapOnly: personas.swapOnly,
      enabledPairs,
      disabledPairs,
      plans,
      dependsOnPullRequest: LIVE_EXECUTION_PREREQUISITES.dependsOnPullRequest,
    };
    assertNoEmbeddedSecrets(scenario, "dual-persona scenario");
    return scenario;
  }

  return {
    configPath: options.configPath,
    personas,
    enabledPairs,
    disabledPairs,
    planPair,
    planEnabledPairs: planEnabled,
    planDualPersonaScenario,
    executeLive() {
      return refuseLiveExecution("scenario");
    },
  };
}

export function createBSeriesHarnessFromEnv(
  env: NodeJS.Dict<string> = process.env,
  configPath?: string
): BSeriesHarness {
  return createBSeriesHarness({
    configPath,
    env,
    lp: walletSourceFromEnv(env, LP_WALLET_ENV),
    swapOnly: walletSourceFromEnv(env, SWAP_ONLY_WALLET_ENV),
  });
}
