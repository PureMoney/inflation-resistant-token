import type {
  HarnessAction,
  Persona,
  PersonaKind,
  PlannedAccount,
  PlannedStep,
} from "./types";
import { POSITION_ACCOUNT_ROLES } from "./types";
import type { ResolvedWallet } from "./wallets";

const LP_ACTIONS: HarnessAction[] = [
  "swap",
  "fillLimitOrder",
  "placeLimitOrder",
  "cancelLimitOrder",
  "closeLimitOrder",
  "openPosition",
  "closePosition",
  "addLiquidity",
  "removeLiquidity",
];

const SWAP_ONLY_ACTIONS: HarnessAction[] = ["swap", "fillLimitOrder"];

export const PERSONA_ACTIONS: Record<PersonaKind, ReadonlySet<HarnessAction>> = {
  lp: new Set(LP_ACTIONS),
  "swap-only": new Set(SWAP_ONLY_ACTIONS),
};

export function createPersona(
  kind: PersonaKind,
  wallet: ResolvedWallet,
  id = kind
): Persona {
  return {
    id,
    kind,
    publicKey: wallet.publicKey,
  };
}

export function personaCan(kind: PersonaKind, action: HarnessAction): boolean {
  return PERSONA_ACTIONS[kind].has(action);
}

export function assertPersonaCan(kind: PersonaKind, action: HarnessAction): void {
  if (!personaCan(kind, action)) {
    throw new Error(`Persona ${kind} cannot perform ${action}`);
  }
}

export function isPositionAccountRole(role: string): boolean {
  const normalized = role.replace(/[^a-zA-Z]/g, "").toLowerCase();
  return POSITION_ACCOUNT_ROLES.some(
    (name) => name.toLowerCase() === normalized || normalized.includes("position")
  );
}

export function assertSwapOnlyNeverOpensPositions(
  kind: PersonaKind,
  step: Pick<PlannedStep, "op" | "opensPositionAccount" | "accounts" | "actor">
): void {
  if (kind !== "swap-only") {
    return;
  }
  if (step.opensPositionAccount) {
    throw new Error(
      `swap-only persona cannot open a position account during ${step.op}`
    );
  }
  const positionAccounts = step.accounts.filter((account) =>
    isPositionAccountRole(account.role)
  );
  if (positionAccounts.length > 0) {
    throw new Error(
      `swap-only persona cannot include position accounts: ${positionAccounts
        .map((account) => account.role)
        .join(", ")}`
    );
  }
}

export function assertNoUserPositionAccounts(
  accounts: PlannedAccount[],
  userPublicKey: string
): void {
  for (const account of accounts) {
    if (account.kind !== "resolved") {
      continue;
    }
    if (isPositionAccountRole(account.role) && account.pubkey === userPublicKey) {
      throw new Error(
        `User ${userPublicKey} must not own position account role ${account.role}`
      );
    }
  }
}

export function dualPersonas(lp: ResolvedWallet, swapOnly: ResolvedWallet): {
  lp: Persona;
  swapOnly: Persona;
} {
  if (lp.publicKey === swapOnly.publicKey) {
    throw new Error("LP and swap-only personas must use separate wallets");
  }
  return {
    lp: createPersona("lp", lp, "lp"),
    swapOnly: createPersona("swap-only", swapOnly, "swap-only"),
  };
}
