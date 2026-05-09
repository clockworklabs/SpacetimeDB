## Overview

This app runs repeatable load tests against multiple connectors (Bun+Postgres, CockroachDB, SQLite, Supabase, Convex, SpacetimeDB, etc.).

Each run:

- Loads a scenario from `src/tests/<test-name>/`
- Runs it against one or more connectors
- Writes a JSON report into `./runs/` with TPS and latency stats

---

## Demo Mode

Run a quick performance comparison:

```bash
pnpm run demo
```

The script will:

- Check that required services are running (prompts you to start them if not)
- Seed databases with test data
- Run benchmarks at high contention
- Display animated results comparing your chosen systems (SpacetimeDB and Convex by default, as both are simple to run locally)

**Options:**

- `--seconds N` - Benchmark duration (default: 10)
- `--concurrency N` - Concurrent connections (default: 50)
- `--alpha N` - Contention level (default: 1.5)
- `--systems a,b,c` - Systems to compare (default: convex,spacetimedb)
- `--stdb-compression none|gzip` - SpacetimeDB client compression mode (default: none)
- `--skip-prep` - Skip database seeding
- `--no-animation` - Disable animated output

`demo` always runs the built-in `test-1` scenario. Use `bench` if you need to choose a test explicitly.
`demo` uses `--systems`; `bench` uses `--connectors`.

---

## Prerequisites

- **Node.js** ≥ 22.x
- **pnpm** installed globally
- **Rust** (required for SpacetimeDB benchmarks) -- [install](https://rust-lang.org/tools/install/)
- **Docker** for local Postgres / Cockroach / Supabase
- Local/Cloud Convex

From a fresh clone:

```bash
pnpm install
```

---

## Configuration (`.env`)

Copy `.env.example` to `.env` and adjust.

**Seeding / verification:**

- `SEED_ACCOUNTS` – number of accounts to seed (if unset, code defaults to `100_000`)
- `SEED_INITIAL_BALANCE` – starting balance per account
- `VERIFY` – enable extra verification passes when non-zero
- `ENABLE_RPC_SERVERS` – flag used by scripts that start the RPC benchmark servers

**Runtime toggles:**

- `USE_DOCKER` – `1` = run Docker Compose for Postgres / CockroachDB; `0` = skip
- `SKIP_PG` – `1` = don't init Postgres in prep
- `SKIP_CRDB` – `1` = don't init CockroachDB in prep
- `SKIP_SQLITE` – `1` = don't init SQLite in prep
- `SKIP_SUPABASE` – `1` = don't init Supabase in prep
- `SKIP_CONVEX` – `1` = don't init Convex in prep

Throughput is counted from successful operations that the benchmark client observes completing inside the configured test window for every connector, including SpacetimeDB.

**PostgreSQL / CockroachDB:**

- `PG_URL` – Postgres connection string
- `CRDB_URL` – CockroachDB connection string

**SQLite:**

- `SQLITE_FILE` – path to the SQLite file
- `SQLITE_MODE` – tuning preset for the SQLite connector

**SpacetimeDB:**

- `STDB_URL` – WebSocket URL for SpacetimeDB
- `STDB_MODULE` – module name to load (e.g. `test-1`)
- `STDB_MODULE_PATH` – filesystem path to the module source (for local dev)
- `STDB_COMPRESSION` – SpacetimeDB benchmark client compression (`none` or `gzip`)
- `STDB_CONFIRMED_READS` – `1` = force confirmed reads on, `0` = force them off

**Supabase:**

- `SUPABASE_URL` – Supabase project URL
- `SUPABASE_ANON_KEY` – Supabase anon/public key
- `SUPABASE_DB_URL` – Postgres connection string for the Supabase database

**Convex:**

- `CONVEX_URL` – Convex deployment URL
- `CONVEX_SITE_URL` – Convex site URL
- `CLEAR_CONVEX_ON_PREP` – Convex prep flag (clears data when enabled)
- `CONVEX_USE_SHARDED_COUNTER` – flag for using the sharded-counter implementation

**PlanetScale:**

- `PLANETSCALE_PG_URL` – Postgres connection string
- `PLANETSCALE_RPC_URL` – PlanetScale RPC server URL (default: `http://127.0.0.1:4104`)
- `SKIP_PLANETSCALE_PG` – `1` = skip PlanetScale in prep

**Bun / RPC helpers:**

- `BUN_URL` – Bun HTTP benchmark server URL
- `BUN_PG_URL` – Postgres connection string for the Bun benchmark service

**RPC benchmark servers:**

- `PG_RPC_URL` – HTTP URL for the Postgres RPC server
- `CRDB_RPC_URL` – HTTP URL for the CockroachDB RPC server
- `SQLITE_RPC_URL` – HTTP URL for the SQLite RPC server

---

## PlanetScale configuration

Create a Postgres database on PlanetScale and set `PLANETSCALE_PG_URL` (and `PLANETSCALE_RPC_URL` if the RPC server runs elsewhere) in `.env`. Reported results used PS-2560 (32 vCPUs, 256 GB RAM).

---

## Setup

### Generate bindings (first time after clone)

**SpacetimeDB module bindings:**

```bash
cd spacetimedb
cargo run -p spacetimedb-cli -- generate --lang typescript --out-dir ../module_bindings --module-path . -y
cd ..
```

(Or use `spacetime generate ...` if the CLI is installed.)

**Convex generated files:**

```bash
cd convex-app
npx convex dev
# Wait for it to generate files, then Ctrl+C
cd ..
```

### Start services

1. Start SpacetimeDB (`cargo run -p spacetimedb-cli -- start` or `spacetime start`)
2. Start Convex (inside convex-app run `npx convex dev`)
3. Init Supabase (run `supabase init`) inside project root.
4. `pnpm run prep` to seed the databases.
5. `pnpm run bench` to run the test against all connectors.

## Commands & Examples

### Run a test

```bash
pnpm run bench [test-name] [--seconds N] [--concurrency N] [--alpha A] [--connectors list] [--stdb-compression none|gzip]
```

Examples:

```bash
# Default test (test-1), default args
pnpm run bench

# Explicit test name
pnpm run bench test-1

# Short run, 100 concurrent workers
pnpm run bench test-1 --seconds 10 --concurrency 100

# Heavier skew on hot accounts
pnpm run bench test-1 --alpha 2.0

# Enable gzip for the SpacetimeDB benchmark client
pnpm run bench test-1 --connectors spacetimedb --stdb-compression gzip

# Only run selected connectors
pnpm run bench test-1 --connectors spacetimedb,sqlite_rpc
```

## CLI Arguments

From `src/cli.ts`:

- **`test-name`** (positional)
  - Name of the test folder under `src/tests/`
  - Default: `test-1`

- **`--seconds N`**
  - Duration of the benchmark in seconds
  - Default: `10`

- **`--concurrency N`**
  - Number of workers / in-flight operations
  - Default: `50`

- **`--alpha A`**
  - Zipf α parameter for account selection (hot vs cold distribution)
  - Default: `1.5`

- **`--connectors list`**
  - Optional, comma-separated list of connector `system` names
  - Example:

    ```bash
    --connectors spacetimedb,sqlite_rpc,postgres_rpc
    ```

  - If omitted, all connectors for that test are run
  - The valid names come from `tc.system` in the test modules and the keys in `CONNECTORS`
  - Valid names: `convex`, `spacetimedb`, `bun`, `postgres_rpc`, `cockroach_rpc`, `sqlite_rpc`, `supabase_rpc`, `planetscale_pg_rpc`

- **`--contention-tests startAlpha endAlpha step concurrency`**
  - Runs a sweep over Zipf α values for a single connector
  - Uses `startAlpha`, `endAlpha`, and `step` to choose the α values
  - Uses the provided `concurrency` for all runs

- **`--concurrency-tests startConc endConc step alpha`**
  - Runs a sweep over concurrency levels for a single connector
  - Uses `startConc`, `endConc`, and `step` to choose the concurrency values
  - Uses the provided `alpha` for all runs

---

### Running in Docker

You can also run the benchmark via Docker instead of Node directly:

```bash
docker compose run --rm bench -- --seconds 5 --concurrency 50 --alpha 1 --connectors convex
```

If using Docker, make sure to set `USE_DOCKER=1` in `.env`, verify docker-compose env variables, verify you've run supabase init, and run `pnpm run prep` before running bench.

## Output

Every run writes a JSON file into `./runs/`:

- Directory: `./runs/`
- Filename: `<test-name>-<timestamp>.json`
  - Example: `test-1-2025-11-17T16-45-12-345Z.json`

Point your visualizations / CSV exports at `./runs/` and you’re good.
