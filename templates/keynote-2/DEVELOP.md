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
npm run demo
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
- `--skip-prep` - Skip database seeding
- `--no-animation` - Disable animated output

`demo` always runs the built-in `test-1` scenario. Use `bench` if you need to choose a test explicitly.
`demo` uses `--systems`; `bench` uses `--connectors`.

---

## Prerequisites

- **Node.js** ≥ 22.x
- **pnpm** installed globally
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
- `SPACETIME_METRICS_ENDPOINT` – `1` = read committed transfer counts from the derived SpacetimeDB metrics endpoint; otherwise only local counters are used

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
4. `npm run prep` to seed the databases.
5. `npm run bench` to run the test against all connectors.

## Commands & Examples

### 1. Run a test

```bash
npm run bench -- [test-name] [--seconds N] [--concurrency N] [--alpha A] [--connectors list]
```

Examples:

```bash
# Default test (test-1), default args
npm run bench

# Explicit test name
npm run bench -- test-1

# Short run, 100 concurrent workers
npm run bench -- test-1 --seconds 10 --concurrency 100

# Heavier skew on hot accounts
npm run bench -- test-1 --alpha 2.0

# Only run selected connectors
npm run bench -- test-1 --connectors spacetimedb,sqlite_rpc
```

### 2. Run the distributed TypeScript SpacetimeDB benchmark

Use this mode when you want to spread explicit TypeScript client connections across multiple machines. The existing `npm run bench` flow is still the single-process benchmark; the distributed flow is a separate coordinator + generator setup.

The commands below are written so they run unchanged on a single machine. For a true multi-machine run, replace `127.0.0.1` with the actual coordinator and server hostnames or IP addresses reachable from each generator machine.

#### Machine roles

- **Server machine**: runs SpacetimeDB and hosts the benchmarked module.
- **Coordinator machine**: runs `bench-dist-coordinator` and `bench-dist-control`. It may also run one or more generators if you want.
- **Generator machines**: run `bench-dist-generator`. You can run multiple generator processes on the same machine as long as each one has a unique `--id`.

#### Distributed setup

All coordinator and generator machines should use the same `templates/keynote-2` checkout and have dependencies installed:

```bash
cd templates/keynote-2
pnpm install
cp .env.example .env
```

Generate TypeScript bindings in that checkout on each machine that will run the coordinator or a generator:

```bash
spacetime generate --lang typescript --out-dir module_bindings --module-path ./spacetimedb
```

#### Step 1: Start the server

On the **server machine**:

```bash
spacetime start
```

#### Step 2: Publish the module and seed the accounts

On any machine with the repo checkout and CLI access to the server, publish the module and seed the database once before starting the distributed run:

```bash
cd templates/keynote-2

export STDB_URL=ws://127.0.0.1:3000
export STDB_MODULE=test-1
export STDB_MODULE_PATH=./spacetimedb

spacetime publish -c -y --server local --module-path "$STDB_MODULE_PATH" "$STDB_MODULE"
spacetime call --server local "$STDB_MODULE" seed 100000 10000000
```

If you are using a named server instead of `local`, replace `--server local` with the correct server name.

#### Step 3: Start the coordinator

On the **coordinator machine**:

```bash
cd templates/keynote-2

pnpm run bench-dist-coordinator -- \
  --test test-1 \
  --connector spacetimedb \
  --warmup-seconds 15 \
  --window-seconds 30 \
  --verify 1 \
  --stdb-url ws://127.0.0.1:3000 \
  --stdb-module test-1 \
  --bind 127.0.0.1 \
  --port 8080
```

Notes:

- `--warmup-seconds` is the unmeasured warmup period. Generators submit requests during warmup, but those transactions are excluded from TPS.
- `--window-seconds` is the measured interval.
- `--pipelined 1` enables request pipelining. Omit it or pass `--pipelined 0` to stay in closed-loop mode, one request at a time.
- `--max-inflight-per-connection` caps the number of in-flight requests each connection may have when pipelining is enabled. The default is `8`.
- `--verify 1` preserves the existing benchmark semantics by running one verification pass centrally after the epoch completes.
- The coordinator derives the HTTP metrics endpoint from `--stdb-url` by switching to `http://` or `https://` and appending `/v1/metrics`.
- For a real multi-machine run, change `--bind 127.0.0.1` to `--bind 0.0.0.0` so remote generators can reach the coordinator.
- For a real multi-machine run, set `--stdb-url` to the server machine's reachable address.

#### Step 4: Start generators on one or more client machines

On **generator machine 1**:

```bash
cd templates/keynote-2

pnpm run bench-dist-generator -- \
  --id gen-a \
  --coordinator-url http://127.0.0.1:8080 \
  --test test-1 \
  --connector spacetimedb \
  --concurrency 2500 \
  --accounts 100000 \
  --alpha 1.5 \
  --open-parallelism 128 \
  --control-retries 3 \
  --stdb-url ws://127.0.0.1:3000 \
  --stdb-module test-1
```

On **generator machine 2**:

```bash
cd templates/keynote-2

pnpm run bench-dist-generator -- \
  --id gen-b \
  --coordinator-url http://127.0.0.1:8080 \
  --test test-1 \
  --connector spacetimedb \
  --concurrency 2500 \
  --accounts 100000 \
  --alpha 1.5 \
  --open-parallelism 128 \
  --control-retries 3 \
  --stdb-url ws://127.0.0.1:3000 \
  --stdb-module test-1
```

Repeat that on as many generator machines as needed, adjusting `--id` and `--concurrency` for each process.
For a real multi-machine run, replace `127.0.0.1` with the coordinator host in `--coordinator-url` and the SpacetimeDB server host in `--stdb-url`.

`--open-parallelism` controls connection ramp-up only. It deliberately avoids a connection storm by opening connections in bounded parallel batches.
`--control-retries` sets the retry cap for `register`, `ready`, `/state`, and `/stopped`. The default is `3`.

#### Step 5: Confirm generators are ready

On the **coordinator machine**:

```bash
cd templates/keynote-2

pnpm run bench-dist-control -- status --coordinator-url http://127.0.0.1:8080
```

Wait until each generator shows `state=ready` and `opened=N/N`.

#### Step 6: Start an epoch

On the **coordinator machine**:

```bash
cd templates/keynote-2

pnpm run bench-dist-control -- start-epoch --coordinator-url http://127.0.0.1:8080 --label run-1
```

`start-epoch` waits for the epoch to finish, then prints the final result.

#### Step 7: Check results

Each completed epoch writes one JSON result file on the coordinator machine under:

```text
templates/keynote-2/runs/distributed/
```

The result contains:

- participating generator IDs
- total participating connections
- whether the epoch used closed-loop or pipelined load, and the per-connection in-flight cap
- committed transaction delta from the server metrics endpoint
- measured window duration
- computed TPS
- verification result

#### Operational notes

- Start the coordinator before the generators.
- Generators begin submitting requests when the coordinator enters `warmup`, not when the measured window begins.
- Throughput is measured only from the committed transaction counter delta recorded after warmup, so warmup transactions are excluded.
- Distributed TypeScript mode defaults to closed-loop, one request at a time per connection. Enable pipelining on the coordinator with `--pipelined 1`, and all generators will follow that setting for the epoch.
- Late generators are allowed to register and become ready while an epoch is already running, but they only participate in the next epoch.
- The coordinator does not use heartbeats. It includes generators that most recently reported `ready`.
- If a participating generator dies and never sends `/stopped`, the epoch result is written with an `error`, and that generator remains `running` in coordinator status until you restart it and let it register again.
- You can run multiple generator processes on the same machine if you want to test the harness locally. Just make sure each process uses a unique `--id`.

---

## CLI Arguments

From `src/cli.ts`:

- **`test-name`** (positional)
  - Name of the test folder under `src/tests/`
  - Default: `test-1`

- **`--seconds N`**
  - Duration of the benchmark in seconds
  - Default: `1`

- **`--concurrency N`**
  - Number of workers / in-flight operations
  - Default: `10`

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
docker compose run --rm bench \
  --seconds 5 \
  --concurrency 50 \
  --alpha 1 \
  --connectors convex
```

If using Docker, make sure to set `USE_DOCKER=1` in `.env`, verify docker-compose env variables, verify you've run supabase init, and run `npm run prep` before running bench.

## Output

Every run writes a JSON file into `./runs/`:

- Directory: `./runs/`
- Filename: `<test-name>-<timestamp>.json`
  - Example: `test-1-2025-11-17T16-45-12-345Z.json`

Point your visualizations / CSV exports at `./runs/` and you’re good.
