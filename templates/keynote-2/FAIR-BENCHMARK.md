# Fair Benchmark: SpacetimeDB vs Competitors

This is an alternative benchmark configuration that levels the playing field
between SpacetimeDB and traditional database stacks. It runs alongside the
standard `demo`/`bench` commands without modifying their behaviour.

## What's Already Fair in `master`

When this fork was first opened (Feb 2026), the standard benchmark had
several asymmetries between SpacetimeDB and competitors. Most have since been
addressed upstream:

| Asymmetry | Status on master |
|-----------|------------------|
| SpacetimeDB used a hand-tuned Rust client; competitors used TypeScript | **Fixed (#4753):** Rust client removed; everyone uses the TypeScript client. |
| `STDB_CONFIRMED_READS` defaulted to `false` | **Fixed (#4682):** confirmed reads is now the default. |
| 5s warmup applied only to the SpacetimeDB Rust client | **Fixed (#4757):** warmup removed everywhere. |
| TypeScript client was missing some optimizations the Rust client had | **Fixed (#4494):** TS client brought to parity. |
| Compression unspecified | **Fixed (#4743):** compression mode is now an explicit knob. |

What this PR additionally enforces:

| Factor | Standard `bench` | Fair Benchmark |
|--------|------------------|----------------|
| **Pipelining** | SpacetimeDB pipelines up to `maxInflightPerWorker` (128) per connection; HTTP RPC connectors do not opt-in to pipelining | **Sequential** for all systems (`BENCH_PIPELINED=0`) so per-connection concurrency is identical |
| **Postgres isolation** | `serializable` (forced via the standard `docker-compose.yml`) | `read_committed` (Postgres' actual default) — `SELECT … FOR UPDATE` already provides row locking |
| **Postgres `synchronous_commit`** | `off` | `on` (matches SpacetimeDB confirmed-reads durability) |
| **Postgres single-call transfer** | 5 ORM round-trips via Drizzle | Adds `postgres_storedproc_rpc`: a single `SELECT do_transfer(...)` PL/pgSQL call |
| **Confirmed reads** | Default (true) | Explicitly forced to `true` (belt-and-suspenders) |

These remaining differences matter because the **architectural** advantage
of SpacetimeDB (colocated compute + storage, no network hop for data access)
is what we want to isolate. Sequential mode and a single-call stored
procedure remove the most obvious confounds between "platform architecture"
and "client/protocol/ORM choices."

## Why These Changes Matter

### 1. Sequential Operations (No Pipelining)

SpacetimeDB's TypeScript connector advertises `maxInflightPerWorker = 128`,
which the runner picks up to pipeline 128 in-flight reducer calls per
connection. RPC connectors (Postgres, CockroachDB, SQLite) leave this unset,
so their per-connection concurrency is effectively 1. Forcing
`BENCH_PIPELINED=0` makes both sides sequential per connection, isolating
the per-call latency comparison.

### 2. Postgres Isolation Level

The standard `docker-compose.yml` sets
`default_transaction_isolation=serializable` for Postgres. That is **not**
Postgres' default (`read_committed`). Under the Zipf contention workload,
serializable causes large transaction-abort/retry storms that
disproportionately hurt Postgres. The benchmark already uses
`SELECT … FOR UPDATE` for row-level locking, so serializable is unnecessary
to get correct results.

### 3. Stored Procedure (`postgres_storedproc_rpc`)

A SpacetimeDB reducer is a **single atomic call** that runs inside the
database. The standard Postgres comparison uses Drizzle ORM, which sends
roughly:

- `BEGIN`
- `SELECT … FOR UPDATE` (fetch both accounts)
- `UPDATE` (debit)
- `UPDATE` (credit)
- `COMMIT`

i.e. ~5 round-trips between Node and Postgres per transfer. A PL/pgSQL
stored procedure (`do_transfer`) does the same work in a single round-trip
— architecturally the same shape as a reducer. Comparing
`postgres_storedproc_rpc` against `spacetimedb` cleanly isolates the
"platform architecture" gap from the "ORM round-trip overhead" gap.

### 4. `synchronous_commit=on`

SpacetimeDB with confirmed reads waits for durable acknowledgement before
returning. Postgres should match: `synchronous_commit=on` (the default;
the standard compose file overrides it to `off` for raw throughput).

## Running the Fair Benchmark

### Prerequisites

```bash
# Install dependencies
pnpm install

# Start services with fair config (Postgres tuned to defaults; stored
# procedure RPC server included)
docker compose -f docker-compose-fair.yml up -d

# If running SpacetimeDB locally instead of in Docker
spacetime start
spacetime publish --server local test-1 --module-path ./spacetimedb
```

### Run

```bash
# Default: SpacetimeDB vs Postgres (ORM) vs Postgres (stored proc)
pnpm run fair-bench

# With options (these mirror the standard bench CLI; --skip-prep is fair-bench-only)
pnpm run fair-bench -- --seconds 10 --concurrency 50 --alpha 0.5

# High contention
pnpm run fair-bench -- --alpha 1.5

# Include more systems
pnpm run fair-bench -- --systems spacetimedb,postgres_rpc,postgres_storedproc_rpc,sqlite_rpc

# Skip seeding (if already seeded)
pnpm run fair-bench -- --skip-prep
```

### Start the Stored Procedure RPC Server (non-Docker)

```bash
# Set PG_URL in .env or environment
PG_STOREDPROC_RPC_PORT=4105 npx tsx src/rpc-servers/postgres-storedproc-rpc-server.ts
```

## Postgres Rust Client (Apples-to-Apples Binary Protocol)

Now that `master` has removed the SpacetimeDB Rust client, the original
"Rust binary protocol vs Node.js HTTP+JSON" gap is no longer present in the
standard benchmark — both sides use TypeScript. The
`postgres-rust-client/` directory in this PR is therefore a **standalone
reference**: it lets you measure how much of any remaining gap is due to
Node.js client overhead vs. the database itself, by driving Postgres with a
Rust binary client.

```bash
cd postgres-rust-client
cargo run --release -- --seconds 10 --concurrency 50 --alpha 0.5
```

## What the Numbers Should Show

With the playing field leveled, SpacetimeDB's genuine architectural
advantage (colocated compute+storage, no network hop for data access)
should still show a meaningful speedup. The remaining gap reflects:

- Zero-copy in-process data access vs TCP round-trips
- Rust execution vs Node.js JavaScript on the server side
- Binary BSATN protocol vs JSON serialization

Confounds that are **not** architectural and are normalized away here:

- Per-connection pipelining differences
- Postgres being run at non-default isolation
- ORM overhead (5 round-trips) vs single-call reducer
- `synchronous_commit=off` skewing Postgres' durability story
