# Inflation-Resistant Medium of Account (IRMA)

This a set of Solana contracts started in solang (now continuing in Rust for easy integration with Anchor) that implement an inflation-resistant stablecoin using OpenBook. Switching over to Rust to make it easy to integrate with Solana. This is now patent pending.

IRMA Token: irmaN9bozBo9nnTqu1RBuoK7imbvBiFJTtdm6c7tC9f

# For dev contributors

If you are new to Solana, it helps to read a good intro: [Link](https://www.helius.dev/blog/the-solana-programming-model-an-introduction-to-developing-on-solana)

Programming directly on Solana involves lots of complicated details. Using the Anchor library helps simplify things like figuring out the instructions that form a transaction. Here is a good documentation on Anchor: [Link](https://book.anchor-lang.com/)


# How to install the tools in your Linux workstation
(Ref: https://solana.com/docs/intro/installation)

## 0. Run Ubuntu 24.04

The problem with previous versions of Ubuntu is that the glibc version that comes with those are not compatible with anchor version 0.31.1. You can save time by starting with the Ubuntu 24.04 distribution. If you are running Ubuntu undel WSL in Windows, you can terminate any previous running version and simply issue the following wsl command:

wsl --install Ubuntu-24.04

## 1. Installing Solana, Cargo, and Anchor

```shell
# Using Ubuntu WSL

@ Install solana cli
sh -c "$(curl -sSfL https://release.solana.com/v1.18.4/install)"

# Cargo installation
sudo apt install cargo

# Anchor installation
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force

# Below is optional if cargo install fails
sudo apt-get update && sudo apt-get upgrade && sudo apt-get install -y pkg-config build-essential libudev-dev libssl-dev

# Install the latest version of the CLI using avm and set to latest
avm install latest
avm use latest
```

Add the `/home/<username>/.cargo/bin` to the $PATH.

Verify installation by:

```shell
avm --version
anchor --version
```

Source: [here](https://www.anchor-lang.com/docs/installation)

Anchor is installed at this point

```shell
anchor init hello-world
```

this prepares files for Rust - not what we need - but good enough to start somewhere.

## 2. Rustc Installation
(Ref: https://solana.com/docs/intro/installation)

### To integrate with a pre-existing, external contract:

```shell
anchor idl fetch --out openbook.idl <address of OpenBook program>
cargo add openbook.idl
```

### To compile all TypeScript source files

The tsc config file tsconfig.json must be used, and the only way I found to do this (that also transpiles the .ts files) is to simply do the CLI command "tsc".

### Install Nodejs and yarn
(Ref: https://solana.com/docs/intro/installation)

### Install mocha (for running tests)
(Ref: https://installati.one/install-mocha-ubuntu-20-04/)

## 3. Possible Issues

If your contract increases in size as you develop it, "anchor deploy" can fail with "Error: Deploying program failed: RPC response error -32002: Transaction simulation failed: Error processing Instruction 0: account data too small for instruction [3 log messages]". Anchor deploy calculates the necessary data size, but if you modify the contract, memory size requirement of your contract can increase. This is the reason for this error. Another error that can come up during testing is "Error: failed to send transaction: Transaction simulation failed: Error processing Instruction 0: incorrect program id for instruction".

To fix:

Delete the target folder

Run "anchor build --solana-version 1.18.4" this will add a new keypair to target/deploy

Start localnet solana-test-validator in another WSL instance

Run "anchor deploy" in the first WSL instance

Run "anchor keys list", this will give you the new program ids

Delete the target folder again

Run anchor build again to get the correct id(s) in target/types folder

Stop the solana-test-validator, then run "anchor test"

Note: The program ids of all contracts should be in the following places:
in the sol source file (above "contract" keyword), in the target/idl json files (bottom), in the target/types files (bottom), and in Anchor.toml.

It can be confusing because "anchor deploy" outputs program ids which can be different from "anchor keys list". If this happens, use the ids output by "anchor deploy".

Also, the Solana address to which the program is deployed must have some minimum SOL! Not only the account that deployed the program, but also the program address itself must have some SOL (because of the rent rule for any address: an address does not exist if it has no SOL). This issue shows up when you try to "anchor run node":

```
failed, error:  SendTransactionError: failed to send transaction: Transaction simulation failed: Error processing Instruction 0: incorrect program id for instruction
  logs: [
    'Program 4b6V88qC7MXuvFhnTf1wSTQVJj7eswky6WkETaNqTJtm invoke [1]',
    'Program log: program_id should be 2VN914zCR1WkHwXeP8vRdimBWMYuNGL9MqJbbQ28SmMy',
    'Program 4b6V88qC7MXuvFhnTf1wSTQVJj7eswky6WkETaNqTJtm consumed 1672 of 200000 compute units',
    'Program 4b6V88qC7MXuvFhnTf1wSTQVJj7eswky6WkETaNqTJtm failed: incorrect program id for instruction'
  ],
  programErrorStack: ProgramErrorStack {
    stack: [
      [PublicKey [PublicKey(4b6V88qC7MXuvFhnTf1wSTQVJj7eswky6WkETaNqTJtm)]]
    ]
  }
}
```
To fix this issue, first find out what the address of the program is (program id), by issuing the following CLI:

solana-keygen pubkey target/deploy/openbookv_2_interface-keypair.json

and then

solana airdrop 1 \<program id obtained above\>

then you'd have to edit four files to put this same address as the program id.

However, the program CreateMarket could still fail to run, display the same error message as above. What finally worked for me was the following:
```
1. Go to the other WSL instance and stop solana-test-validator.
2. Issue the command "solana-test-validator --reset" in order to start clean.
3. In the first WSL instance, issue the following commands:
    anchor build --solana-version 1.18.4
    solana-keygen pubkey target/deploy/openbookv_2_interface-keypair.json
    anchor deploy -p openbookv_2_interface --program-keypair target/deploy/openbookv_2_interface-keypair.json
4. The command "anchor run node" should then output the following:

Your transaction signature 4ycYnpRui2x1hMHtNRWrPE3dRFt4KToZvgm2vmWSCwoCCmV7325sSGF7m1PBqd7ZCtUgH7HFBRshFnzwaX4Tb72h
state {
  side: { bid: {} },
  priceLots: <BN: 400>,
  maxBaseLots: <BN: 800>,
  maxQuoteLotsIncludingFees: <BN: 54bd2>,
  clientOrderId: <BN: bacf>,
  orderType: { market: {} },
  expiryTimestamp: <BN: 8a8310>,
  selfTradeBehavior: { abortTransaction: {} },
  limit: 80
}
size:  44
```

Finally, it is recommended not to use "anchor deploy". Instead, use "solana program deploy":

solana program deploy target/deploy/openbookv_2_interface.so  -k /home/ctapang/.config/solana/id.json

If you make a mistake, you can start all over with the localnet cluster by issuing the command: "solana-test-validator --reset" which would delete the blockchain and start from genesis.
