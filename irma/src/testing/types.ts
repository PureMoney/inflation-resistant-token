/**
 * Shared types for IRMA B-series test-harness prep.
 * Planning and validation only — no live transaction execution.
 */

export type TokenConfig = {
  mint: string;
  name: string;
  symbol: string;
  decimals: number;
  program: string;
};

export type PoolConfig = {
  address: string;
  tokenX: string;
  tokenY: string;
  /** When omitted, the harness disables USDC and enables every other pair. */
  enabled?: boolean;
  disabledReason?: string;
};

export type DevnetConfig = {
  network: string;
  tokens: Record<string, TokenConfig>;
  program: { programId: string };
  pools: Record<string, PoolConfig>;
};

export type EnabledPair = {
  key: string;
  token: TokenConfig;
  pool: PoolConfig;
};

export type DisabledPair = {
  key: string;
  token?: TokenConfig;
  pool: PoolConfig;
  reason: string;
};

export type PersonaKind = "lp" | "swap-only";

export type HarnessAction =
  | "swap"
  | "fillLimitOrder"
  | "placeLimitOrder"
  | "cancelLimitOrder"
  | "closeLimitOrder"
  | "openPosition"
  | "closePosition"
  | "addLiquidity"
  | "removeLiquidity";

export type Persona = {
  id: string;
  kind: PersonaKind;
  publicKey: string;
};

export type LimitOrderSide = "ask" | "bid";

export type LimitOrderOp = "place" | "fill" | "cancel" | "close";

export type BinTarget =
  | { kind: "absolute"; binId: number }
  | { kind: "relativeToActive"; offset: number };

export type PlannedAccount =
  | {
      kind: "resolved";
      role: string;
      pubkey: string;
      isSigner: boolean;
      isWritable: boolean;
    }
  | {
      kind: "deferred";
      role: string;
      source: string;
      isSigner: boolean;
      isWritable: boolean;
    };

export type PlannedStep = {
  op: LimitOrderOp;
  actor: PersonaKind;
  pairKey: string;
  tokenName: string;
  irmaInstruction: string | null;
  opensPositionAccount: boolean;
  accounts: PlannedAccount[];
  args: Record<string, unknown>;
  notes: string[];
};

export type LimitOrderPlan = {
  mode: "dry-run";
  pairKey: string;
  poolAddress: string;
  steps: PlannedStep[];
  dependsOnPullRequest: number;
  requiresAdminWalletForLiveExecution: boolean;
  requiresIrmaIdlForLiveExecution: boolean;
};

export type DualPersonaScenario = {
  mode: "dry-run";
  lp: Persona;
  swapOnly: Persona;
  enabledPairs: EnabledPair[];
  disabledPairs: DisabledPair[];
  plans: LimitOrderPlan[];
  dependsOnPullRequest: number;
};

export const LIVE_EXECUTION_PREREQUISITES = {
  dependsOnPullRequest: 147,
  irmaIdlPath: "target/idl/irma.json",
  mutatesDevnet: true,
} as const;

export const POSITION_ACCOUNT_ROLES = [
  "position",
  "positionPk",
  "positionPks",
  "newPosition",
  "oldPosition",
] as const;
