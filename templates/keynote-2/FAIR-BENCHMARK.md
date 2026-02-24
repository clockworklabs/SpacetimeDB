# Fair Benchmark: SpacetimeDB vs Competitors

This is an alternative benchmark configuration that levels the playing field between SpacetimeDB and traditional database stacks. The original benchmark (`demo.ts`) has several asymmetries that compound to inflate SpacetimeDB's advantage far beyond what the architecture alone provides.

## What This Changes

| Factor | Original Benchmark | Fair Benchmark |
|--------|-------------------|----------------|
| **SpacetimeDB client** | Custom Rust client with 16,384 in-flight ops | Same TypeScript client as everyone else |
| **TPS counting** | Server-side Prometheus metrics (fire-and-forget) | Client-side round-trip counting for ALL systems |
| **Durability** | `confirmedReads=false` (no durability guarantee) | `confirmedReads=true` (durable commits, like Postgres fsync) |
| **Pipeline depth** | 16,384 for SpacetimeDB vs 8 for competitors | Same for all (configurable, default 8) |
| **Postgres isolation** | `serializable` (non-default, worst-case for contention) | `read_committed` (Postgres actual default) |
| **Postgres sync commit** | `synchronous_commit=off` | `synchronous_commit=on` (matches SpacetimeDB confirmed reads) |
| **Postgres transfer** | 4 ORM round-trips via Drizzle | Also tested with stored procedure (single DB call) |
| **Warmup** | 5s warmup for Rust client only | No warmup for any system (equal cold start) |

## Why These Changes Matter

### 1. Same Client Language (TypeScript for All)

The original benchmark uses a hand-tuned **Rust client** for SpacetimeDB that sends 16,384 concurrent operations per connection via binary WebSocket, while all competitors use a TypeScript client with HTTP/JSON and 8 in-flight operations. This alone is a ~2000x difference in pipeline depth.

The README justifies this by saying "we were bottlenecked on our test TypeScript client" — but then no competitor gets the same optimization. A fair comparison uses the same client for all.

### 2. Confirmed Reads (Durable Commits)

The original benchmark defaults `STDB_CONFIRMED_READS` to `false`, meaning SpacetimeDB doesn't wait for durable commits before reporting success. Meanwhile Postgres runs with `fsync=on`. This is comparing "maybe durable" vs "definitely durable" — not a fair durability comparison.

### 3. Client-Side TPS Counting

The original `demo.ts` sets `USE_SPACETIME_METRICS_ENDPOINT=1`, which counts committed transactions **on the server** via Prometheus. Combined with the fire-and-forget Rust client, this counts transactions that completed server-side but whose acknowledgments may not have reached the client. All other systems count only after the full round-trip completes.

### 4. Postgres Isolation Level

The original forces `default_transaction_isolation=serializable` on Postgres, which is **not** the default (`read_committed`). Under the Zipf contention workload, serializable causes massive transaction aborts and retries, dramatically hurting Postgres performance. The benchmark already uses `SELECT ... FOR UPDATE` for row-level locking, making serializable unnecessary.

### 5. Stored Procedure vs ORM

SpacetimeDB's reducer executes as a single atomic operation inside the database. The original Postgres benchmark uses Drizzle ORM which requires:
- `BEGIN`
- `SELECT ... FOR UPDATE` (fetch both accounts)
- `UPDATE` (debit)
- `UPDATE` (credit)
- `COMMIT`

That's 5 round-trips between the Node.js process and Postgres. A stored procedure (`do_transfer()`) does the same work in a single call — which is the fair equivalent of SpacetimeDB's reducer model.

## Running the Fair Benchmark

### Prerequisites

```bash
# Install dependencies
pnpm install

# Start services
docker compose -f docker-compose-fair.yml up -d

# Start SpacetimeDB
spacetime start

# Publish the SpacetimeDB module
spacetime publish --server local test-1 --module-path ./spacetimedb
```

### Run

```bash
# Default: SpacetimeDB vs Postgres (ORM) vs Postgres (stored proc)
npm run fair-bench

# With options
npm run fair-bench -- --seconds 10 --concurrency 50 --alpha 0.5

# High contention
npm run fair-bench -- --alpha 1.5

# Include more systems
npm run fair-bench -- --systems spacetimedb,postgres_rpc,postgres_storedproc_rpc,sqlite_rpc

# Custom pipeline depth
npm run fair-bench -- --pipeline-depth 16

# Skip seeding (if already seeded)
npm run fair-bench -- --skip-prep
```

### Start the Stored Procedure RPC Server (non-Docker)

```bash
# Set PG_URL in .env or environment
PG_STOREDPROC_RPC_PORT=4105 npx tsx src/rpc-servers/postgres-storedproc-rpc-server.ts
```

## Expected Results

With a leveled playing field, SpacetimeDB's genuine architectural advantage (colocated compute+storage, no network hop for data access) should still show a meaningful speedup — likely **2-5x** rather than the claimed **14x**. The remaining advantage is real and architectural:

- Zero-copy in-process data access vs TCP round-trips
- Rust execution vs Node.js JavaScript
- Binary BSATN protocol vs JSON serialization

The factors that are **not** architectural and were removed:
- Custom Rust client vs shared TypeScript client
- 16,384 vs 8 pipeline depth
- Server-side vs client-side TPS counting
- Unequal durability guarantees
- Non-default Postgres isolation level penalizing competitors
- ORM overhead (5 round-trips) vs single reducer call

## Detailed Asymmetry Analysis

For a comprehensive breakdown of every asymmetry in the original benchmark, see the table in the PR description.
