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

All tests run for 300 seconds with 64 concurrent connections, with a transfer workload (read-modify-write transaction between two accounts).

The SpacetimeDB rows were obtained using a single-node SpacetimeDB Standalone instance, so the published numbers are reproducible with the public, downloadable server.

Each row reports mean TPS and sample standard deviation of per-second throughput within a single 300-second run. `alpha=1.5` corresponds to ~80% contention. When standard deviation approaches or exceeds mean TPS, throughput is unstable across the run.

Data description: reported summary metrics are computed from steady-state windows after a 30-second warmup (`tSec >= 30`), using the recorded per-second `timeSeries` data.

### Alpha = 0

| System | clients | pipelining | max_pool | TPS | TPS Stddev | p50 lat ms | p99 lat ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SpacetimeDB | 64 | 40 | N/A | 279,024 | 4,763 | 8 | 12 |
| Node.js + SQLite | 64 | off | N/A | 3,121 | 80 | 19 | 40 |
| Node.js + Supabase | 64 | off | 64 | 7,362 | 1,179 | 6 | 18 |
| Bun + Postgres | 64 | off | 64 | 10,729 | 146 | 5 | 11 |
| Node.js + Postgres | 64 | off | 64 | 9,904 | 223 | 6 | 11 |
| Node.js + PlanetScale (SN) | 64 | off | 64 | 4,535 | 117 | 14 | 20 |
| Node.js + PlanetScale (HA) | 384 | off | 384 | 4,275 | 135 | 89 | 110 |
| Convex | 64 | off | N/A | 1,140 | 118 | 53 | 62 |
| Node.js + CockroachDB (5 node) | 320 | off | 320 | 4,253 | 561 | 71 | 120 |
| HAProxy - Node.js + CockroachDB (5 node) | 320 | off | 320 | 5,481 | 566 | 57 | 95 |

### Alpha = 1.5

| System | clients | pipelining | max_pool | TPS | TPS Stddev | p50 lat ms | p99 lat ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| SpacetimeDB | 64 | 40 | N/A | 303,919 | 4,712 | 7 | 11 |
| Node.js + SQLite | 64 | off | N/A | 3,188 | 73 | 18 | 39 |
| Node.js + Supabase | 64 | off | 64 | 2,534 | 57 | 2 | 197 |
| Bun + Postgres | 64 | off | 64 | 2,772 | 61 | 7 | 13 |
| Node.js + Postgres | 64 | off | 64 | 961 | 25 | 10 | 16 |
| Node.js + PlanetScale (SN) | 64 | off | 64 | 235 | 12 | 20 | 2,504 |
| Node.js + PlanetScale (HA) | 384 | off | 384 | 248 | 13 | 416 | 10,121 |
| Convex | 64 | off | N/A | 126 | 52 | 20 | 1,081 |
| Node.js + CockroachDB (5 node) | 320 | off | 320 | 0.03 | 0.18 | 698 | 9,695 |
| HAProxy - Node.js + CockroachDB (5 node) | 64 | off | 64 | 6.87 | 9.12 | 5,943 | 9,880 |

### Alpha = 0 (Pipelined)

| System | clients | pipelining | max_pool | TPS | TPS Stddev | p50 lat ms | p99 lat ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Node.js + SQLite | 64 | 40 | N/A | 2,977 | 84 | 722 | 747 |
| Node.js + Supabase | 64 | 40 | 64 | 8,874 | 308 | 284 | 303 |
| Bun + Postgres | 64 | 40 | 64 | 10,184 | 120 | 250.1 | 260.5 |
| Node.js + Postgres | 64 | 40 | 64 | 9,165 | 145 | 276 | 290 |
| Node.js + PlanetScale (SN) | 64 | 40 | 64 | 4,325 | 85 | 590 | 604 |
| Node.js + PlanetScale (HA) | 384 | 40 | 384 | 3,355 | 327 | 4,354 | 4,438 |
| Convex | 64 | 40 | N/A | 1,154 | 134 | 2,119 | 2,150 |
| Node.js + CockroachDB (5 node) | 320 | 40 | 320 | 4,250 | 766 | 3,030 | 3,161 |
| HAProxy - Node.js + CockroachDB (5 node) | 320 | 40 | 320 | 5,992 | 1,765 | 2,431 | 2,562 |

**Key Finding:** In these runs, SpacetimeDB is the only system sustaining hundreds of thousands of TPS in both alpha profiles. Non-SpacetimeDB systems remain in the low-thousands TPS range at best, and several show severe contention sensitivity at `alpha=1.5` with large tail-latency growth.

## Methodology

All systems were tested with **out-of-the-box default settings**, with one exception: the local Postgres instance (and Bun, which uses the same Postgres instance) is configured with `default_transaction_isolation = 'serializable'`. No other custom tuning or configuration optimization was applied.

The managed Postgres services (Supabase, PlanetScale) run at their default isolation level of `READ COMMITTED`.

Throughput is counted from successful operations that the benchmark client observes completing inside the configured test window for every system.

### Published Benchmark Defaults

The reported tables in this README use the following defaults unless a row explicitly shows a different value:

- `clients`: `64`
- `pipelining`: `off` for non-pipelined tables
- `MAX_POOL`: `64` for pg-based RPC servers (`postgres_rpc`, `cockroach_rpc`, `supabase_rpc`, `planetscale_pg_rpc`)
- Pipelined table runs use `BENCH_PIPELINED=1` and `MAX_INFLIGHT_PER_WORKER=40`
- `MAX_INFLIGHT_PER_WORKER` is required whenever `BENCH_PIPELINED=1`

For rows that scale client count above 64 (for example, some HA topologies), `max_pool` is scaled to match the row values shown in the table.

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

When aggregating these JSONs into summary tables, use a 30-second warmup cutoff (`--warmup-sec 30`) to match the published numbers.

Useful flags:

- `--alpha <csv>`: Zipf alpha. This benchmark reports `0` (uniform / ~0% contention) and `1.5` (Zipf / ~80% contention).
- `--connectors <csv>`: which connectors to run. Defaults to every test in `src/tests/test-1/`.
- `--seconds <num>`: duration of each run.
- `--concurrency <num>`: number of concurrent clients (default: `64`).
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

