import * as fs from "fs";
import { Keypair, PublicKey } from "@solana/web3.js";

/**
 * Wallet inputs for B-series testing. Planning only needs a public key.
 * Keypair material is referenced by path/env name and never copied into plans.
 */
export type WalletSource =
  | { type: "publicKey"; publicKey: string }
  | { type: "envPublicKey"; envVar: string }
  | { type: "keypairFile"; path: string }
  | { type: "envKeypairPath"; envVar: string };

export type ResolvedWallet = {
  publicKey: string;
  source: WalletSource["type"];
  /** Path that later live execution may read. Never a secret. */
  keypairPath?: string;
  hasKeypairMaterial: boolean;
};

const SECRET_KEYS = [
  "secretKey",
  "secret_key",
  "privateKey",
  "private_key",
  "SOLANA_PRIVATE_KEY",
];

export function resolveWallet(
  source: WalletSource,
  env: NodeJS.Dict<string> = process.env
): ResolvedWallet {
  switch (source.type) {
    case "publicKey":
      return {
        publicKey: requirePublicKey(source.publicKey).toBase58(),
        source: source.type,
        hasKeypairMaterial: false,
      };
    case "envPublicKey": {
      const value = env[source.envVar];
      if (!value) {
        throw new Error(`Missing public-key env var ${source.envVar}`);
      }
      return {
        publicKey: requirePublicKey(value).toBase58(),
        source: source.type,
        hasKeypairMaterial: false,
      };
    }
    case "envKeypairPath": {
      const value = env[source.envVar];
      if (!value) {
        throw new Error(`Missing keypair-path env var ${source.envVar}`);
      }
      return readKeypairPublicKey(value, source.type);
    }
    case "keypairFile":
      return readKeypairPublicKey(source.path, source.type);
    default:
      throw new Error(
        `Unsupported wallet source: ${(source as WalletSource).type}`
      );
  }
}

export function readKeypairPublicKey(
  keypairPath: string,
  source: ResolvedWallet["source"] = "keypairFile"
): ResolvedWallet {
  if (!fs.existsSync(keypairPath)) {
    throw new Error(`Keypair file not found: ${keypairPath}`);
  }
  const parsed = JSON.parse(fs.readFileSync(keypairPath, "utf-8"));
  const keypair = keypairFromJson(parsed);
  return {
    publicKey: keypair.publicKey.toBase58(),
    source,
    keypairPath,
    hasKeypairMaterial: true,
  };
}

export function keypairFromJson(parsed: unknown): Keypair {
  if (!Array.isArray(parsed) || parsed.some((n) => typeof n !== "number")) {
    throw new Error("Keypair file must be a JSON array of numbers");
  }
  return Keypair.fromSecretKey(Uint8Array.from(parsed));
}

export function requirePublicKey(value: string): PublicKey {
  try {
    return new PublicKey(value.trim());
  } catch {
    throw new Error(`Invalid public key: ${value}`);
  }
}

export function assertNoEmbeddedSecrets(value: unknown, label = "plan"): void {
  walkForSecretArrays(value, label);
}

function walkForSecretArrays(value: unknown, label: string): void {
  if (Array.isArray(value)) {
    if (
      value.length === 64 &&
      value.every((n) => typeof n === "number" && n >= 0 && n <= 255)
    ) {
      throw new Error(`${label} looks like a raw secret-key array`);
    }
    value.forEach((entry, index) =>
      walkForSecretArrays(entry, `${label}[${index}]`)
    );
    return;
  }
  if (value && typeof value === "object") {
    for (const [key, nested] of Object.entries(
      value as Record<string, unknown>
    )) {
      if (SECRET_KEYS.includes(key)) {
        throw new Error(`${label} embeds secret field ${key}`);
      }
      walkForSecretArrays(nested, `${label}.${key}`);
    }
  }
}

export const LP_WALLET_ENV = {
  publicKey: "IRMA_LP_PUBKEY",
  keypairPath: "IRMA_LP_WALLET",
} as const;

export const SWAP_ONLY_WALLET_ENV = {
  publicKey: "IRMA_SWAP_ONLY_PUBKEY",
  keypairPath: "IRMA_SWAP_ONLY_WALLET",
} as const;

export function walletSourceFromEnv(
  env: NodeJS.Dict<string>,
  vars: { publicKey: string; keypairPath: string },
  fallbackPublicKey?: string
): WalletSource {
  if (env[vars.publicKey]) {
    return { type: "envPublicKey", envVar: vars.publicKey };
  }
  if (env[vars.keypairPath]) {
    return { type: "envKeypairPath", envVar: vars.keypairPath };
  }
  if (fallbackPublicKey) {
    return { type: "publicKey", publicKey: fallbackPublicKey };
  }
  throw new Error(
    `Provide ${vars.publicKey} or ${vars.keypairPath} (path only; do not paste secret keys)`
  );
}
