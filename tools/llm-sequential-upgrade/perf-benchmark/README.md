# Perf Benchmark — PG vs STDB Chat Apps

Runtime performance harness for the Level 11 chat apps the LLM built in the
sequential upgrade benchmark. Measures throughput, latency, and concurrent
connections so we have showcase numbers for the marketing one-pager.

This is **not** a synthetic benchmark of PostgreSQL vs SpacetimeDB. It's a
benchmark of *the apps the LLM built on each stack*, run as-is.

## What it tests

| Scenario | What it measures |
|---|---|
| `stress` | N writers flooding `send_message` for D seconds. Sustained msgs/sec + p99 latency. |
| `realistic` | M users at human cadence (5–15s jitter) for D seconds. Latency under realistic load. |
| `soak` | Ramp connections at +R/sec until errors or cap. Max concurrent connections. |

## Setup

```bash
npm install

# Generate SpacetimeDB bindings against the target Level 11 app's backend.
# Re-run this if you change which app you're benchmarking.
spacetime generate --lang typescript --out-dir src/module_bindings \
  --module-path ../sequential-upgrade/sequential-upgrade-20260406/spacetime/results/chat-app-20260406-153727/backend/spacetimedb
```

## Prerequisites for running

The target apps must already be running:

- **Postgres**: `cd <pg-app>/server && npm run dev` (Express on `:6001`),
  plus the `exhaust-test-postgres-1` Docker container (port 6432).
- **SpacetimeDB**: local `spacetime start` running, and the target module
  must be published (the apps publish themselves automatically when generated).

## Run

```bash
# PG stress, 30s, 20 writers
npm run run -- --backend pg --scenario stress --writers 20 --duration 30

# STDB stress, 30s, 50 writers
npm run run -- --backend stdb --scenario stress --writers 50 --duration 30 \
  --module chat-app-20260406-153727

# All scenarios for one backend
npm run run -- --backend pg --scenario all
npm run run -- --backend stdb --scenario all --module chat-app-20260406-153727
```

Results land in `results/<timestamp>/<backend>-<scenario>.json`.

## Caveats

- The PG app's `send_message` handler enforces a **500ms-per-user rate limit**
  in application code. Each PG writer can therefore issue at most ~2 msgs/sec.
  Throughput scales with writers, not with cadence. The harness paces writers
  at ~510ms to avoid drops. SpacetimeDB has no equivalent limit, so its
  per-writer ceiling is much higher.
- Numbers reflect what shipped from the LLM, on a single dev machine, against
  a local DB. They are not the theoretical ceiling of either backend.
- Each connection in the harness uses the same Node process clock, so fan-out
  latency is meaningful (no clock skew across machines).
