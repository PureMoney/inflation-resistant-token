import { PublicKey } from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { listEnabledPairs, requireEnabledPair } from "./config";
import {
  amountToRaw,
  deriveBinArrayPda,
  deriveEventAuthorityPda,
  deriveIrmaPdas,
  loadDlmmProgramId,
  MEMO_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from "./pdas";
import {
  assertPersonaCan,
  assertSwapOnlyNeverOpensPositions,
} from "./personas";
import type { ResolvedWallet } from "./wallets";
import { assertNoEmbeddedSecrets, requirePublicKey } from "./wallets";
import type {
  BinTarget,
  DevnetConfig,
  EnabledPair,
  LimitOrderOp,
  LimitOrderPlan,
  LimitOrderSide,
  PersonaKind,
  PlannedAccount,
  PlannedStep,
} from "./types";
import { LIVE_EXECUTION_PREREQUISITES } from "./types";

export type PlaceLimitOrderInput = {
  pair: string;
  side: LimitOrderSide;
  amount: number;
  actor: PersonaKind;
  owner: string;
  bin?: BinTarget;
  limitOrder?: string;
  tokenProgram?: string;
  reserve?: string;
  activeBinId?: number;
};

export type CancelLimitOrderInput = {
  pair: string;
  actor: PersonaKind;
  owner: string;
  limitOrder: string;
  binIds?: number[];
  tokenXProgram?: string;
  tokenYProgram?: string;
  reserveX?: string;
  reserveY?: string;
};

export type CloseLimitOrderInput = {
  actor: PersonaKind;
  owner: string;
  limitOrder: string;
  pair?: string;
};

export type FillLimitOrderInput = {
  pair: string;
  actor: PersonaKind;
  swapper: string;
  side: LimitOrderSide;
  amount: number;
  bin?: BinTarget;
  limitOrder?: string;
};

export type LifecycleInput = {
  pair: string;
  side: LimitOrderSide;
  amount: number;
  lp: ResolvedWallet | string;
  swapOnly: ResolvedWallet | string;
  bin?: BinTarget;
  limitOrder?: string;
  activeBinId?: number;
};

const PLACE_ACCOUNT_ROLES = [
  "lbPair",
  "reserve",
  "tokenMint",
  "limitOrder",
  "owner",
  "userToken",
  "tokenProgram",
  "systemProgram",
  "eventAuthority",
  "dlmmProgram",
  "binArray",
] as const;

const CANCEL_ACCOUNT_ROLES = [
  "lbPair",
  "reserveX",
  "reserveY",
  "tokenXMint",
  "tokenYMint",
  "limitOrder",
  "ownerTokenX",
  "ownerTokenY",
  "owner",
  "tokenXProgram",
  "tokenYProgram",
  "memoProgram",
  "eventAuthority",
  "dlmmProgram",
] as const;

function ownerKey(owner: ResolvedWallet | string): string {
  return typeof owner === "string"
    ? requirePublicKey(owner).toBase58()
    : owner.publicKey;
}

function defaultBinTarget(side: LimitOrderSide): BinTarget {
  return { kind: "relativeToActive", offset: side === "ask" ? 10 : -10 };
}

function resolveBinId(
  bin: BinTarget,
  activeBinId?: number
): number | undefined {
  if (bin.kind === "absolute") {
    if (!Number.isInteger(bin.binId)) {
      throw new Error(`binId must be an integer, got ${bin.binId}`);
    }
    return bin.binId;
  }
  if (typeof activeBinId === "number") {
    if (!Number.isInteger(bin.offset)) {
      throw new Error(`bin offset must be an integer, got ${bin.offset}`);
    }
    return activeBinId + bin.offset;
  }
  return undefined;
}

function resolvedAccount(
  role: string,
  pubkey: string,
  isSigner: boolean,
  isWritable: boolean
): PlannedAccount {
  return { kind: "resolved", role, pubkey, isSigner, isWritable };
}

function deferredAccount(
  role: string,
  source: string,
  isSigner: boolean,
  isWritable: boolean
): PlannedAccount {
  return { kind: "deferred", role, source, isSigner, isWritable };
}

function instructionAccounts(
  config: DevnetConfig,
  owner: string
): PlannedAccount[] {
  const { statePda, corePda } = deriveIrmaPdas(config.program.programId);
  return [
    resolvedAccount("state", statePda.toBase58(), false, true),
    resolvedAccount("irmaAdmin", owner, true, true),
    resolvedAccount("core", corePda.toBase58(), false, true),
    resolvedAccount("systemProgram", SYSTEM_PROGRAM_ID, false, false),
  ];
}

export function planPlaceLimitOrder(
  config: DevnetConfig,
  input: PlaceLimitOrderInput
): PlannedStep {
  assertPersonaCan(input.actor, "placeLimitOrder");
  const pair = requireEnabledPair(config, input.pair);
  const side = input.side;
  if (side !== "ask" && side !== "bid") {
    throw new Error(`side must be ask or bid, got ${side}`);
  }

  const owner = requirePublicKey(input.owner).toBase58();
  const bin = input.bin ?? defaultBinTarget(side);
  const binId = resolveBinId(bin, input.activeBinId);
  const decimals =
    side === "ask" ? config.tokens.irma.decimals : pair.token.decimals;
  const amountRaw = amountToRaw(input.amount, decimals);
  const tokenMint = side === "ask" ? pair.pool.tokenX : pair.pool.tokenY;
  const tokenProgram =
    input.tokenProgram ??
    (side === "ask" ? config.tokens.irma.program : pair.token.program);
  const dlmmProgramId = loadDlmmProgramId();
  const limitOrder = input.limitOrder
    ? requirePublicKey(input.limitOrder).toBase58()
    : undefined;
  const userToken = getAssociatedTokenAddressSync(
    new PublicKey(tokenMint),
    new PublicKey(owner),
    false,
    new PublicKey(tokenProgram)
  ).toBase58();

  const remaining: PlannedAccount[] = [
    resolvedAccount("lbPair", pair.pool.address, false, true),
    input.reserve
      ? resolvedAccount("reserve", input.reserve, false, true)
      : deferredAccount(
          "reserve",
          side === "ask" ? "lbPair.reserveX" : "lbPair.reserveY",
          false,
          true
        ),
    resolvedAccount("tokenMint", tokenMint, false, false),
    limitOrder
      ? resolvedAccount("limitOrder", limitOrder, true, true)
      : deferredAccount("limitOrder", "generated-at-execution", true, true),
    resolvedAccount("owner", owner, true, true),
    resolvedAccount("userToken", userToken, false, true),
    resolvedAccount("tokenProgram", tokenProgram, false, false),
    resolvedAccount("systemProgram", SYSTEM_PROGRAM_ID, false, false),
    resolvedAccount(
      "eventAuthority",
      deriveEventAuthorityPda(dlmmProgramId).toBase58(),
      false,
      false
    ),
    resolvedAccount("dlmmProgram", dlmmProgramId, false, false),
  ];

  if (typeof binId === "number") {
    remaining.push(
      resolvedAccount(
        "binArray",
        deriveBinArrayPda(pair.pool.address, binId, dlmmProgramId).toBase58(),
        false,
        true
      )
    );
  } else {
    remaining.push(
      deferredAccount(
        "binArray",
        "binId relative to lbPair.activeId",
        false,
        true
      )
    );
  }

  const step: PlannedStep = {
    op: "place",
    actor: input.actor,
    pairKey: pair.key,
    tokenName: pair.token.name,
    irmaInstruction: "placeLimitOrder",
    opensPositionAccount: false,
    accounts: [...instructionAccounts(config, owner), ...remaining],
    args: {
      symbol: pair.token.name,
      isAskSide: side === "ask",
      amount: input.amount,
      amountRaw,
      bin,
      binId: binId ?? null,
      computeUnitLimit: 400_000,
    },
    notes: [
      "Account order matches tests/test_limit_order.ts place remaining_accounts.",
      "Live place still requires the IRMA admin signer after PR #147 is on master.",
    ],
  };
  assertAccountRoles(remaining, PLACE_ACCOUNT_ROLES, "place");
  assertSwapOnlyNeverOpensPositions(input.actor, step);
  assertNoEmbeddedSecrets(step, "place step");
  return step;
}

export function planCancelLimitOrder(
  config: DevnetConfig,
  input: CancelLimitOrderInput
): PlannedStep {
  assertPersonaCan(input.actor, "cancelLimitOrder");
  const pair = requireEnabledPair(config, input.pair);
  const owner = requirePublicKey(input.owner).toBase58();
  const binIds = input.binIds ?? [];
  if (binIds.some((id) => !Number.isInteger(id))) {
    throw new Error("cancel binIds must be integers");
  }

  const tokenXProgram = input.tokenXProgram ?? config.tokens.irma.program;
  const tokenYProgram = input.tokenYProgram ?? pair.token.program;
  const ownerTokenX = getAssociatedTokenAddressSync(
    new PublicKey(pair.pool.tokenX),
    new PublicKey(owner),
    false,
    new PublicKey(tokenXProgram)
  ).toBase58();
  const ownerTokenY = getAssociatedTokenAddressSync(
    new PublicKey(pair.pool.tokenY),
    new PublicKey(owner),
    false,
    new PublicKey(tokenYProgram)
  ).toBase58();
  const dlmmProgramId = loadDlmmProgramId();
  const limitOrder = requirePublicKey(input.limitOrder).toBase58();

  const remaining: PlannedAccount[] = [
    resolvedAccount("lbPair", pair.pool.address, false, true),
    input.reserveX
      ? resolvedAccount("reserveX", input.reserveX, false, true)
      : deferredAccount("reserveX", "lbPair.reserveX", false, true),
    input.reserveY
      ? resolvedAccount("reserveY", input.reserveY, false, true)
      : deferredAccount("reserveY", "lbPair.reserveY", false, true),
    resolvedAccount("tokenXMint", pair.pool.tokenX, false, false),
    resolvedAccount("tokenYMint", pair.pool.tokenY, false, false),
    resolvedAccount("limitOrder", limitOrder, true, true),
    resolvedAccount("ownerTokenX", ownerTokenX, false, true),
    resolvedAccount("ownerTokenY", ownerTokenY, false, true),
    resolvedAccount("owner", owner, true, true),
    resolvedAccount("tokenXProgram", tokenXProgram, false, false),
    resolvedAccount("tokenYProgram", tokenYProgram, false, false),
    resolvedAccount("memoProgram", MEMO_PROGRAM_ID, false, false),
    resolvedAccount(
      "eventAuthority",
      deriveEventAuthorityPda(dlmmProgramId).toBase58(),
      false,
      false
    ),
    resolvedAccount("dlmmProgram", dlmmProgramId, false, false),
    ...(binIds.length > 0
      ? binIds.map((binId, index) =>
          resolvedAccount(
            `binArray[${index}]`,
            deriveBinArrayPda(
              pair.pool.address,
              binId,
              dlmmProgramId
            ).toBase58(),
            false,
            true
          )
        )
      : [deferredAccount("binArray", "binIds from placed order", false, true)]),
  ];

  const step: PlannedStep = {
    op: "cancel",
    actor: input.actor,
    pairKey: pair.key,
    tokenName: pair.token.name,
    irmaInstruction: "cancelLimitOrder",
    opensPositionAccount: false,
    accounts: [...instructionAccounts(config, owner), ...remaining],
    args: {
      symbol: pair.token.name,
      limitOrder,
      binIds,
      computeUnitLimit: 400_000,
    },
    notes: [
      "Cancel also claims filled/unfilled proceeds (tests/test_limit_order.ts).",
    ],
  };
  assertAccountRoles(
    remaining.slice(0, CANCEL_ACCOUNT_ROLES.length),
    CANCEL_ACCOUNT_ROLES,
    "cancel"
  );
  assertSwapOnlyNeverOpensPositions(input.actor, step);
  assertNoEmbeddedSecrets(step, "cancel step");
  return step;
}

export function planCloseLimitOrder(
  config: DevnetConfig,
  input: CloseLimitOrderInput
): PlannedStep {
  assertPersonaCan(input.actor, "closeLimitOrder");
  const owner = requirePublicKey(input.owner).toBase58();
  const pair = input.pair ? requireEnabledPair(config, input.pair) : undefined;
  const limitOrder = requirePublicKey(input.limitOrder).toBase58();
  const dlmmProgramId = loadDlmmProgramId();

  const remaining: PlannedAccount[] = [
    resolvedAccount("limitOrder", limitOrder, true, true),
    resolvedAccount("owner", owner, true, true),
    resolvedAccount("rentReceiver", owner, false, true),
    resolvedAccount(
      "eventAuthority",
      deriveEventAuthorityPda(dlmmProgramId).toBase58(),
      false,
      false
    ),
    resolvedAccount("dlmmProgram", dlmmProgramId, false, false),
  ];

  const step: PlannedStep = {
    op: "close",
    actor: input.actor,
    pairKey: pair?.key ?? "n/a",
    tokenName: pair?.token.name ?? "n/a",
    irmaInstruction: "closeLimitOrderIfEmpty",
    opensPositionAccount: false,
    accounts: [...instructionAccounts(config, owner), ...remaining],
    args: {
      limitOrder,
      computeUnitLimit: 200_000,
    },
    notes: ["Closes an empty limit-order account to recover rent."],
  };
  assertSwapOnlyNeverOpensPositions(input.actor, step);
  assertNoEmbeddedSecrets(step, "close step");
  return step;
}

/**
 * Fill is not an IRMA instruction. A swap-only counterparty swap across the
 * resting bin fills the order after PR #147 is on master.
 */
export function planFillLimitOrder(
  config: DevnetConfig,
  input: FillLimitOrderInput
): PlannedStep {
  assertPersonaCan(input.actor, "fillLimitOrder");
  const pair = requireEnabledPair(config, input.pair);
  const swapper = requirePublicKey(input.swapper).toBase58();
  const bin = input.bin ?? defaultBinTarget(input.side);
  const decimals =
    input.side === "ask" ? pair.token.decimals : config.tokens.irma.decimals;
  const amountRaw = amountToRaw(input.amount, decimals);

  const step: PlannedStep = {
    op: "fill",
    actor: input.actor,
    pairKey: pair.key,
    tokenName: pair.token.name,
    irmaInstruction: null,
    opensPositionAccount: false,
    accounts: [
      resolvedAccount("lbPair", pair.pool.address, false, true),
      resolvedAccount("swapper", swapper, true, true),
    ],
    args: {
      method: "counterparty-swap",
      side: input.side,
      amount: input.amount,
      amountRaw,
      bin,
      limitOrder: input.limitOrder ?? null,
      poolAddress: pair.pool.address,
    },
    notes: [
      "No fill_limit_order instruction exists on IRMA; fill via DLMM swap.",
      "swap-only wallets must not create or sign a new position account.",
    ],
  };
  assertSwapOnlyNeverOpensPositions(input.actor, step);
  assertNoEmbeddedSecrets(step, "fill step");
  return step;
}

export function planLimitOrderLifecycle(
  config: DevnetConfig,
  input: LifecycleInput
): LimitOrderPlan {
  const lp = ownerKey(input.lp);
  const swapOnly = ownerKey(input.swapOnly);
  if (lp === swapOnly) {
    throw new Error("LP and swap-only wallets must be different");
  }

  const pair = requireEnabledPair(config, input.pair);
  const bin = input.bin ?? defaultBinTarget(input.side);
  const resolvedBinId = resolveBinId(bin, input.activeBinId);
  const placeholderLimitOrder = "11111111111111111111111111111111";
  const limitOrder = input.limitOrder ?? placeholderLimitOrder;

  const steps = [
    planPlaceLimitOrder(config, {
      pair: pair.key,
      side: input.side,
      amount: input.amount,
      actor: "lp",
      owner: lp,
      bin,
      limitOrder: input.limitOrder,
      activeBinId: input.activeBinId,
    }),
    planFillLimitOrder(config, {
      pair: pair.key,
      actor: "swap-only",
      swapper: swapOnly,
      side: input.side,
      amount: input.amount,
      bin,
      limitOrder: input.limitOrder,
    }),
    planCancelLimitOrder(config, {
      pair: pair.key,
      actor: "lp",
      owner: lp,
      limitOrder,
      binIds: typeof resolvedBinId === "number" ? [resolvedBinId] : undefined,
    }),
    planCloseLimitOrder(config, {
      actor: "lp",
      owner: lp,
      pair: pair.key,
      limitOrder,
    }),
  ];

  if (!input.limitOrder) {
    steps[2].notes.push(
      "cancel/close use a placeholder limit-order pubkey until place runs live."
    );
    steps[3].notes.push(
      "cancel/close use a placeholder limit-order pubkey until place runs live."
    );
  }

  const plan: LimitOrderPlan = {
    mode: "dry-run",
    pairKey: pair.key,
    poolAddress: pair.pool.address,
    steps,
    dependsOnPullRequest: LIVE_EXECUTION_PREREQUISITES.dependsOnPullRequest,
    requiresAdminWalletForLiveExecution: true,
    requiresIrmaIdlForLiveExecution: true,
  };
  assertNoEmbeddedSecrets(plan, "limit-order plan");
  return plan;
}

export function planEnabledPairLifecycles(
  config: DevnetConfig,
  input: Omit<LifecycleInput, "pair"> & { pairs?: string[] }
): LimitOrderPlan[] {
  const pairs: EnabledPair[] = input.pairs
    ? input.pairs.map((pair) => requireEnabledPair(config, pair))
    : listEnabledPairs(config);
  return pairs.map((pair: EnabledPair) =>
    planLimitOrderLifecycle(config, { ...input, pair: pair.key })
  );
}

export function refuseLiveExecution(op: LimitOrderOp | "scenario"): never {
  throw new Error(
    `Live ${op} execution is disabled in this harness revision. ` +
      `Wait until PR #${LIVE_EXECUTION_PREREQUISITES.dependsOnPullRequest} is on master, ` +
      `then run with funded wallets and ${LIVE_EXECUTION_PREREQUISITES.irmaIdlPath}.`
  );
}

function assertAccountRoles(
  accounts: PlannedAccount[],
  expected: readonly string[],
  label: string
): void {
  const roles = accounts.map((account) => account.role);
  for (let i = 0; i < expected.length; i++) {
    if (roles[i] !== expected[i]) {
      throw new Error(
        `${label} remaining account[${i}] expected ${expected[i]}, got ${roles[i]}`
      );
    }
  }
}
