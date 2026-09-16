import { expect } from "chai";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { Keypair } from "@solana/web3.js";
import {
  assertNoEmbeddedSecrets,
  resolveWallet,
  walletSourceFromEnv,
  LP_WALLET_ENV,
} from "./wallets";
import {
  assertPersonaCan,
  assertSwapOnlyNeverOpensPositions,
  dualPersonas,
  personaCan,
} from "./personas";

describe("wallet inputs and dual personas", () => {
  const lp = Keypair.generate();
  const swapOnly = Keypair.generate();

  it("resolves public keys without embedding secrets", () => {
    const resolved = resolveWallet({
      type: "publicKey",
      publicKey: lp.publicKey.toBase58(),
    });
    expect(resolved.publicKey).to.equal(lp.publicKey.toBase58());
    expect(resolved.hasKeypairMaterial).to.equal(false);
    assertNoEmbeddedSecrets(resolved, "resolved wallet");
  });

  it("reads only the public key from a keypair file", () => {
    const file = path.join(os.tmpdir(), `irma-harness-${Date.now()}.json`);
    fs.writeFileSync(file, JSON.stringify(Array.from(lp.secretKey)));
    try {
      const resolved = resolveWallet({ type: "keypairFile", path: file });
      expect(resolved.publicKey).to.equal(lp.publicKey.toBase58());
      expect(resolved.hasKeypairMaterial).to.equal(true);
      expect(resolved.keypairPath).to.equal(file);
      expect(JSON.stringify(resolved)).to.not.include("secretKey");
      expect(JSON.stringify(resolved)).to.not.match(/\[(?:\d+,){10,}/);
    } finally {
      fs.unlinkSync(file);
    }
  });

  it("loads wallet sources from env var names rather than pasted secrets", () => {
    const env = { IRMA_LP_PUBKEY: lp.publicKey.toBase58() };
    const source = walletSourceFromEnv(env, LP_WALLET_ENV);
    expect(source).to.deep.equal({
      type: "envPublicKey",
      envVar: "IRMA_LP_PUBKEY",
    });
  });

  it("rejects the same wallet for both personas", () => {
    const wallet = resolveWallet({
      type: "publicKey",
      publicKey: lp.publicKey.toBase58(),
    });
    expect(() => dualPersonas(wallet, wallet)).to.throw(/separate wallets/);
  });

  it("prevents swap-only users from opening position accounts", () => {
    expect(personaCan("swap-only", "swap")).to.equal(true);
    expect(personaCan("swap-only", "fillLimitOrder")).to.equal(true);
    expect(personaCan("swap-only", "openPosition")).to.equal(false);
    expect(personaCan("swap-only", "placeLimitOrder")).to.equal(false);
    expect(() => assertPersonaCan("swap-only", "openPosition")).to.throw(
      /cannot perform openPosition/
    );
    expect(() =>
      assertSwapOnlyNeverOpensPositions("swap-only", {
        op: "fill",
        actor: "swap-only",
        opensPositionAccount: true,
        accounts: [],
      })
    ).to.throw(/cannot open a position account/);
    expect(() =>
      assertSwapOnlyNeverOpensPositions("swap-only", {
        op: "fill",
        actor: "swap-only",
        opensPositionAccount: false,
        accounts: [
          {
            kind: "resolved",
            role: "position",
            pubkey: swapOnly.publicKey.toBase58(),
            isSigner: true,
            isWritable: true,
          },
        ],
      })
    ).to.throw(/cannot include position accounts/);
  });

  it("allows LP personas to place limit orders and open positions", () => {
    expect(personaCan("lp", "placeLimitOrder")).to.equal(true);
    expect(personaCan("lp", "openPosition")).to.equal(true);
    expect(() => assertPersonaCan("lp", "openPosition")).to.not.throw();
  });
});
