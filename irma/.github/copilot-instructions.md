# Copilot Instructions for the Inflation-Resistant Stablecoin Project

## Overview
This project implements an inflation-resistant stablecoin using the Solana blockchain. It leverages the Anchor framework for program development and integrates with Meteora DLMM for market operations. The project is organized into multiple components, with a focus on modularity and scalability.

### Key Components
- **`programs/irma/src/lib.rs`**: The main entry point for the IRMA program. Contains program instructions such as `initialize`, `add_reserve`, `remove_reserve`, and `crank`.
- **`programs/irma/src/pricing.rs`**: Handles pricing logic, including initializing pricing, adding reserves, and disabling reserves.
- **`programs/irma/src/crank_market.rs`**: Manages market operations, including processing events and updating the IRMA state.
- **`tests/`**: Contains integration tests for the IRMA program, written in TypeScript using the Anchor testing framework.

### External Dependencies
- **Anchor Framework**: Used for Solana program development.
- **Meteora DLMM**: Integrated for market operations.
- **Borsh**: Used for serialization and deserialization of program data.

## Developer Workflows

### Building the Program
To build the program, use the following command:
```bash
anchor build
```
This compiles the Rust code and generates the necessary IDL files.

### Testing the Program
Tests are located in the `tests/` directory and are written in TypeScript. To run the tests:
```bash
anchor test
```
This will deploy the program to a local Solana cluster and execute the tests.

### Debugging
- Use `msg!` macros in Rust to log messages during program execution.
- Inspect transaction logs using the Anchor testing framework.
- Use `cargo expand` to debug macro-generated code.

## Project-Specific Conventions

### Program Structure
- Each instruction is defined as a public function in `programs/irma/src/lib.rs`.
- Contexts for instructions are defined in `pricing.rs` and `crank_market.rs`.

### Naming Conventions
- Functions are named using snake_case (e.g., `add_reserve`).
- Modules are named after their functionality (e.g., `pricing`, `crank_market`).

### Error Handling
- Use `Result<()>` for functions that may fail.
- Log errors using `msg!` before returning them.

## Examples

### Adding a Stablecoin Reserve
The `add_reserve` function in `lib.rs` is used to add a new stablecoin to the reserves:
```rust
pub fn add_reserve(ctx: Context<Maint>, symbol: String, mint_address: Pubkey, decimals: u8) -> Result<()> {
    msg!("Add stablecoin entry, size of StateMap: {}", size_of::<StateMap>());
    crate::pricing::add_reserve(ctx, &symbol, mint_address, decimals)
}
```

### Cranking the Market
The `crank` function processes market events and updates the IRMA state:
```rust
pub fn crank<'c: 'info, 'info>(ctx: Context<'_, '_, 'c, 'info, ConsumeEvents>) -> Result<()> {
    msg!("Crank..., ");
    let slot = Clock::get()?.slot;
    msg!("Current slot: {}", slot);
    crank_market(ctx, slot)
}
```

## Key Files and Directories
- **`programs/irma/src/lib.rs`**: Main program logic.
- **`programs/irma/src/pricing.rs`**: Pricing-related logic.
- **`programs/irma/src/crank_market.rs`**: Market operations.
- **`tests/`**: Integration tests.

## Notes for AI Agents
- Always ensure that lifetimes in Rust functions are simplified unless explicitly required.
- Use `cargo expand` to debug macro-generated code when encountering issues with `#[program]` or `#[account]` macros.
- Follow the project-specific naming conventions and module structure for consistency.
