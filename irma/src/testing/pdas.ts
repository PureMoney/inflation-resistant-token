import * as fs from "fs";
import * as path from "path";
import { PublicKey } from "@solana/web3.js";

export const BINS_PER_ARRAY = 70;
export const STATE_SEED = "state_v5";
export const CORE_SEED = "core_v5";
export const BIN_ARRAY_SEED = "bin_array";
export const EVENT_AUTHORITY_SEED = "__event_authority";
export const MEMO_PROGRAM_ID = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
export const SYSTEM_PROGRAM_ID = "11111111111111111111111111111111";

const DEFAULT_DLMM_PROGRAM_ID = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";

export function findDlmmIdlPath(startDir = process.cwd()): string {
  const fromEnv = process.env.IRMA_DLMM_IDL;
  if (fromEnv) {
    return path.resolve(fromEnv);
  }

  let dir = path.resolve(startDir);
  for (let i = 0; i < 8; i++) {
    const candidate = path.join(dir, "idls", "dlmm.json");
    const nested = path.join(dir, "irma", "idls", "dlmm.json");
    if (fs.existsSync(candidate)) {
      return candidate;
    }
    if (fs.existsSync(nested)) {
      return nested;
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      break;
    }
    dir = parent;
  }
  throw new Error("idls/dlmm.json not found. Set IRMA_DLMM_IDL.");
}

export function loadDlmmProgramId(idlPath?: string): string {
  try {
    const resolved = idlPath ?? findDlmmIdlPath();
    const parsed = JSON.parse(fs.readFileSync(resolved, "utf-8")) as {
      address?: string;
    };
    if (!parsed.address) {
      throw new Error(`${resolved} is missing address`);
    }
    return parsed.address;
  } catch (error) {
    if (idlPath) {
      throw error;
    }
    return DEFAULT_DLMM_PROGRAM_ID;
  }
}

export function binIdToBinArrayIndex(binId: number): number {
  return Math.floor(binId / BINS_PER_ARRAY);
}

export function deriveBinArrayPda(
  lbPair: PublicKey | string,
  binId: number,
  dlmmProgramId: PublicKey | string = loadDlmmProgramId()
): PublicKey {
  const binArrayIndex = binIdToBinArrayIndex(binId);
  const binArrayIdxBuffer = Buffer.alloc(8);
  binArrayIdxBuffer.writeBigInt64LE(BigInt(binArrayIndex));
  const [pda] = PublicKey.findProgramAddressSync(
    [
      Buffer.from(BIN_ARRAY_SEED),
      new PublicKey(lbPair).toBuffer(),
      binArrayIdxBuffer,
    ],
    new PublicKey(dlmmProgramId)
  );
  return pda;
}

export function deriveEventAuthorityPda(
  dlmmProgramId: PublicKey | string = loadDlmmProgramId()
): PublicKey {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from(EVENT_AUTHORITY_SEED)],
    new PublicKey(dlmmProgramId)
  );
  return pda;
}

export function deriveIrmaPdas(programId: PublicKey | string): {
  statePda: PublicKey;
  corePda: PublicKey;
} {
  const program = new PublicKey(programId);
  const [statePda] = PublicKey.findProgramAddressSync(
    [Buffer.from(STATE_SEED)],
    program
  );
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from(CORE_SEED)],
    program
  );
  return { statePda, corePda };
}

export function amountToRaw(amount: number, decimals: number): string {
  if (!Number.isFinite(amount) || amount <= 0) {
    throw new Error(`Amount must be a positive number, got ${amount}`);
  }
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 18) {
    throw new Error(`Invalid decimals: ${decimals}`);
  }
  const raw = Math.round(amount * Math.pow(10, decimals));
  if (raw <= 0) {
    throw new Error(`Amount ${amount} rounds to zero at ${decimals} decimals`);
  }
  return String(raw);
}
