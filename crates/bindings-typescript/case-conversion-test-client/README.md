# Case Conversion Test Client (TypeScript)

TypeScript SDK test client for the Rust module `sdk-test-case-conversion`.
Tests that the TS SDK correctly handles case-converted names with digit boundaries.

## What it tests

- Table accessors: `player1`, `person2`, `person_at_level_2`
- Field names with digit boundaries: `player1Id`, `currentLevel2`, `status3Field`
- Nested structs: `personInfo.ageValue1`, `personInfo.scoreTotal`
- Enum variants: `Player2Status` (`Active1`, `BannedUntil`)
- Explicit reducer names: `banPlayer1`
- Query builder: filters on digit-boundary columns, right semijoins

## Prerequisites

1. Build the SpacetimeDB CLI and standalone server:

   ```bash
   cargo build -p spacetimedb-cli -p spacetimedb-standalone
   ```

2. Build the TS SDK (from repo root):
   ```bash
   cd crates/bindings-typescript && pnpm install && pnpm build
   ```

## Regenerate bindings

If the Rust module (`modules/sdk-test-case-conversion`) changes:

```bash
pnpm run generate
```

This runs `spacetime generate` against the module and formats the output.

## Run via the Rust test harness (recommended)

From the repo root:

```bash
cargo test -p spacetimedb-sdk --test test case_conversion_rust_ts_client
```

This compiles the Rust module, starts a local server, publishes the module, generates
bindings, builds the TS client, and runs each subcommand (`insert-player`, `insert-person`,
`ban-player`, `query-builder-filter`, `query-builder-join`).

## Run manually

1. Start the standalone server:

   ```bash
   spacetime start
   ```

2. Publish the module:

   ```bash
   spacetime publish sdk-test-case-conversion \
     --module-path modules/sdk-test-case-conversion --server local
   ```

3. Generate bindings (if not already done):

   ```bash
   cd crates/bindings-typescript/case-conversion-test-client
   pnpm run generate
   ```

4. Build and run a test:
   ```bash
   pnpm install
   pnpm run build
   SPACETIME_SDK_TEST_DB_NAME=sdk-test-case-conversion node dist/index.js insert-player
   ```

Available subcommands: `insert-player`, `insert-person`, `ban-player`,
`query-builder-filter`, `query-builder-join`.
