# SpacetimeDB Benchmark Suite

A benchmark suite comparing SpacetimeDB against traditional web application stacks for transactional workloads.

## Quick Demo

See SpacetimeDB's performance advantage with one command:

```bash
pnpm install
pnpm run demo
```

The demo compares SpacetimeDB and Convex by default, since both are easy for anyone to set up and run locally without additional infrastructure. Other systems (Postgres, CockroachDB, SQLite, etc.) are also supported but require more setup. The demo checks that required services are running (prompts you to start them if not), seeds databases, and displays animated results.

**Options:** `--systems a,b,c` | `--seconds N` | `--concurrency N` | `--alpha N` | `--skip-prep` | `--no-animation`

**Note:** `demo` always runs the built-in `test-1` scenario. Use `bench` if you need to specify a test name directly.
**Note:** `demo` selects targets with `--systems`; `bench` filters test connectors with `--connectors`.

## Results Summary

All tests run for 300 seconds with 50 concurrent connections, with a transfer workload (read-modify-write transaction between two accounts).

The SpacetimeDB rows were obtained using a single-node SpacetimeDB Standalone instance, so the published numbers are reproducible with the public, downloadable server.

Each cell shows **mean TPS ± sample standard deviation** of the per-second throughput within a single 300-second run, with the sample variance in parentheses. Cells where the standard deviation approaches or exceeds the mean (e.g. CockroachDB and Convex at ~80% contention) indicate that the system's throughput is unstable across the run.

| System                                  | Mean TPS ± σ (Var) (~0% Contention) | Mean TPS ± σ (Var) (~80% Contention) |
| --------------------------------------- | ----------------------------------- | ------------------------------------ |
| SpacetimeDB (TypeScript Module)         | 294,827 ± 5,266 (27,728,435)        | 304,865 ± 4,751 (22,569,090)         |
| SpacetimeDB (Rust Module)               | 266,139 ± 4,662 (21,730,912)        | 278,070 ± 4,279 (18,312,134)         |
| SQLite + Node HTTP + Drizzle            | 3,109 ± 86 (7,326)                  | 3,228 ± 80 (6,396)                   |
| Bun + Drizzle + Postgres                | 10,662 ± 215 (46,418)               | 2,773 ± 83 (6,930)                   |
| Supabase + Node HTTP + Drizzle          | 6,853 ± 1,017 (1,034,915)           | 2,896 ± 111 (12,414)                 |
| Postgres + Node HTTP + Drizzle          | 9,933 ± 184 (33,704)                | 2,169 ± 56 (3,161)                   |
| CockroachDB + Node HTTP + Drizzle       | 3,353 ± 25 (630)                    | 79 ± 127 (16,059)                    |
| Convex (self-hosted local)              | 1,120 ± 161 (25,856)                | 118 ± 97 (9,335)                     |
| PlanetScale PS-2560 (single-node, EBS)  | 1,513 ± 26 (678)                    | 289 ± 15 (238)                       |
| PlanetScale M-15360 (Metal NVMe, HA)    | 1,351 ± 25 (637)                    | 279 ± 16 (257)                       |

**Key Finding:** SpacetimeDB reaches hundreds of thousands of TPS for the transfer workload, while the best non-SpacetimeDB result shown here is SQLite at 3,228 TPS. Traditional databases also suffer significant degradation under high contention (CockroachDB drops 98%).

## Methodology

All systems were tested with **out-of-the-box default settings**, with one exception: the local Postgres instance (and Bun, which uses the same Postgres instance) is configured with `default_transaction_isolation = 'serializable'`. No other custom tuning or configuration optimization was applied.

The managed Postgres services (Supabase, PlanetScale) run at their default isolation level of `READ COMMITTED`.

Throughput is counted from successful operations that the benchmark client observes completing inside the configured test window for every system.

### Test Architecture

All benchmarks follow an **apples-to-apples** comparison using the same architecture pattern:

```
Client → Web Server (HTTP) → ORM (Drizzle) → Database
```

Or for integrated platforms (SpacetimeDB, Convex):

```
Client → Integrated Platform (compute + storage colocated)
```

This ensures we're measuring real-world application performance, not raw database throughput.

### Machine Topology

The reported numbers use a single benchmark host wherever possible. This means client, server, and database were all run on the same machine.

We did this mainly because it was the most favorable benchmarking setup for the competitor platforms, because it minimizes server to database latency, but also because it allows others to easily reproduce the results.

For completeness, we also tested separated-machine topologies, where the benchmark client, server, and database processes were not colocated on one machine. However, in each case we found that doing so either did not change or reduced the throughput of other systems due to the additional network hop. We published the most favorable numbers for our competitors.

The platforms that cannot use this exact topology are PlanetScale and CockroachDB. PlanetScale operates a managed cloud database and does not have a self-hosted variant of the service, so the benchmark client and RPC server are colocated on a benchmark host in the same region and availability zone as the database host. CockroachDB is a distributed database running across multiple nodes, so the benchmark client and RPC server cannot be colocated with the database on a single node.

### The Transaction

Each transaction performs a **fund transfer** between two accounts:

1. Read both source and destination account balances
2. Verify sufficient funds in source account
3. Debit source account
4. Credit destination account
5. Commit transaction with row-level locking

This is a classic read-modify-write workload that tests transactional integrity under concurrent access.

### Test Command

The numbers in the table above were collected with `pnpm run bench`:

```bash
pnpm install
pnpm run prep                                                              # seed all backing databases once
pnpm run bench --alpha 0,1.5 --connectors <connectors> --seconds 300       # one JSON per (connector, alpha)
```

`--alpha` and `--connectors` both accept comma-separated values. The bench writes one JSON per (connector, alpha, run) tuple into `runs/`.

Useful flags:

- `--alpha <csv>`: Zipf alpha. This benchmark reports `0` (uniform / ~0% contention) and `1.5` (Zipf / ~80% contention).
- `--connectors <csv>`: which connectors to run. Defaults to every test in `src/tests/test-1/`.
- `--seconds <num>`: duration of each run.
- `--concurrency <num>`: number of concurrent clients (default: `50`).
- `--runs <num>`: repeat each (connector, alpha) combination this many times (default: `1`). Each repeat writes its own JSON.
- `--prep-between-alphas`: run `pnpm run prep` before each (connector, alpha) combination to reset DB state.
- `--stdb-compression <none|gzip>`: SpacetimeDB client compression mode (default: `none`).

### Hardware Configuration

**Server Machine (all systems except PlanetScale):**

- PhoenixNAP s3.c3.medium bare metal instance - Intel i9-14900k 24 cores (32 threads), 128GB DDR5 Memory, OS: Ubuntu 24.04

**Bench client for PlanetScale:**

- AWS `m7i.8xlarge` in `us-east-2`, colocated with the PlanetScale cluster. Clusters tested: PS-2560 single-node EBS, M-15360 Metal HA (1 primary + 2 replicas). Both Postgres 18.3.

### Account Seeding

- 100,000 accounts seeded before each benchmark
- Initial balance: 1,000,000,000 per account
- Zipf distribution controls which accounts are selected for transfers

## Technical Notes

### Why SpacetimeDB Outperforms Traditional Stacks

The primary bottleneck in traditional web application architectures is the **round-trip latency between the application server and database**:

```
Traditional: Client → Server → Database → Server → Client
                        ↑___________↑
                     Network round-trip per query
```

SpacetimeDB eliminates this by **colocating compute and storage**:

```
SpacetimeDB: Client → SpacetimeDB (compute + storage) → Client
```

This architectural difference means SpacetimeDB can execute transactions in microseconds rather than milliseconds, resulting in order-of-magnitude performance improvements.

### Client Pipelining

The benchmark supports **pipelining** for all clients - sending multiple requests without waiting for responses. This maximizes throughput by keeping connections saturated.

### Confirmed Reads (`withConfirmedReads`)

SpacetimeDB supports `withConfirmedReads` mode which ensures transactions are durably committed before acknowledging to the client. The benchmark results shown use `withConfirmedReads = ON` for fair comparison with databases that provide similar durability guarantees.

### Cloud vs Local Results

PlanetScale results (~280 TPS under high contention, regardless of cluster tier) demonstrate the **significant impact of cloud database latency**. When the database is accessed over the network (even within the same cloud region), round-trip latency dominates performance. This is why SpacetimeDB's colocated architecture provides such dramatic improvements.

## Systems Tested

| System                            | Architecture                                            |
| --------------------------------- | ------------------------------------------------------- |
| SpacetimeDB Standalone            | Integrated platform; single-node downloadable server.   |
| SQLite + Node HTTP + Drizzle      | Node.js HTTP server → Drizzle ORM → SQLite              |
| Bun + Drizzle + Postgres          | Bun HTTP server → Drizzle ORM → PostgreSQL              |
| Postgres + Node HTTP + Drizzle    | Node.js HTTP server → Drizzle ORM → PostgreSQL          |
| Supabase + Node HTTP + Drizzle    | Node.js HTTP server → Drizzle ORM → Supabase (Postgres) |
| CockroachDB + Node HTTP + Drizzle | Node.js HTTP server → Drizzle ORM → CockroachDB         |
| PlanetScale + Node HTTP + Drizzle | Node.js HTTP server → Drizzle ORM → PlanetScale (Cloud) |
| Convex                            | Integrated platform                                     |

## Running the Benchmarks

See [DEVELOP.md](./DEVELOP.md) for prerequisites, configuration, and full CLI reference.

## Output

Benchmark results are written to `./runs/` as JSON files with TPS and latency statistics.

## License

See repository root for license information.
