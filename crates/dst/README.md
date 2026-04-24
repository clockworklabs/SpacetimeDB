# `spacetimedb-dst`

Deterministic simulation testing utilities for SpacetimeDB.

## DST In A Nutshell

Current DST is a CLI-driven simulator pipeline:

1. the CLI picks a `target`, `scenario`, seed, and run budget
2. the workload generator produces a deterministic stream or materialized case
3. the target installs schema and executes interactions against a real engine
4. properties are checked during execution and against the final outcome
5. on failure, the saved case can be replayed and shrunk from CLI

Today the main shared workload family is `workload/table_ops/`.
It is good for targets that behave like transactional tables:

- schema generation
- inserts / deletes
- transaction begin / commit / rollback
- range scans and visibility checks
- scenario-specific properties such as `banking`

The important split is:

- workload code decides what to try
- target code decides how to execute it on a concrete engine
- properties decide whether the observed behavior is valid

## What Is In This Crate

This crate contains reusable pieces for building deterministic simulations,
shared workload generators, and concrete DST targets.

- root helpers:
  `seed.rs`, `config.rs`
- root internal helpers:
  `bugbase.rs`, `shrink.rs`
- root shared target internals:
  `schema.rs`
- `workload/`:
  shared table-style workload split into scenarios, generation, model, and
  properties
- `targets/`:
  `datastore.rs`, `relational_db_commitlog.rs`
- binary:
  `src/main.rs`

## Reading Order

If you are new to the crate, this order keeps the mental model small:

1. `src/main.rs`
2. `config.rs`
3. `seed.rs`
4. `workload/table_ops/`
5. `targets/datastore.rs`
6. `targets/relational_db_commitlog.rs`

## Core Model

Most code in the crate revolves around the same shape:

- `Case`: generated input for one deterministic run.
- `Outcome`: final observable result.
- Properties/checks: assertions performed during execution or against the final outcome.

That separation is intentional:

- generation decides what to try,
- execution decides what happened,
- properties decide whether the run is acceptable,
- shrinking tries to keep the failure while deleting unnecessary steps.

## Shared Table Workload Map

The main reusable DST workload now lives in `workload/table_ops/`:

1. `types.rs`
   common scenario, interaction, outcome, and engine traits
2. `scenarios/`
   scenario-specific schema generation like `random_crud`, `indexed_ranges`,
   and `banking`
3. `model.rs`
   generator model and expected-state model
4. `generation.rs`
   `InteractionStream` and scenario-aware workload planning
5. `runner.rs`
   generic execute/run helpers shared by multiple targets

Concrete targets like `targets/datastore.rs` and `targets/relational_db_commitlog.rs`
reuse that workload and swap in target-specific engines and target-owned
properties.

## Property Ownership

Properties are now owned by targets, not by `workload/table_ops`.

- workload emits only operations (`BeginTx`, `CommitTx`, `Insert`, `Delete`, ...)
- target execution code decides which properties to evaluate and when
- failure messages are tagged by property family for easier triage

Current target-side property families include:

- `PQS::InsertSelect`
- `PQS::IndexRangeExcluded` (composite index range behavior)
- `NoREC::SelectSelectOptimizer`
- `TLP::WhereTrueFalseNull`
- `TLP::UNIONAllPreservesCardinality`
- `DeleteSelect`
- shadow-style table consistency checks (for banking-like mirrored tables)

## Failure Flow

For a failing target case:

1. `run_case_detailed` returns `DatastoreExecutionFailure`
2. internal `shrink.rs` truncates after failure and tries removing interactions
   while preserving the same failure reason

## CLI

Long DST runs are intended to be driven from CLI, not from `#[test]`.

Core commands:

```bash
cargo run -p spacetimedb-dst -- run --target datastore --scenario banking --duration 5m
cargo run -p spacetimedb-dst -- run --target datastore --scenario indexed-ranges --duration 5m
cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --seed 42 --max-interactions 2000
cargo run -p spacetimedb-dst -- replay --target datastore bug.json
cargo run -p spacetimedb-dst -- shrink --target datastore bug.json
```

DST workloads are run from CLI only. Use `random-crud` for broad coverage and
`indexed-ranges` when you want to bias toward secondary/composite index range
behavior without hardcoding a single historical bug.

## How To Add More Targets

There are two extension patterns.

### 1. Reuse `table_ops`

Use this when the new engine still looks like a transactional table store.
Examples:

- another datastore wrapper
- another relational layer
- a storage engine exposing the same table semantics through a different API

In that case:

1. add `targets/<new_target>.rs`
2. reuse `TableWorkloadCase` and `TableScenarioId`
3. implement the target-specific engine bootstrap and row operations
4. expose the same CLI-facing functions used by `main.rs`
   - `materialize_case`
   - `run_case_detailed`
   - `run_generated_with_config_and_scenario`
   - `save_case`
   - `load_case`
   - `shrink_failure`
5. add the target to the CLI `TargetKind`

This is the path `datastore` and `relational_db_commitlog` use today.

### 2. Add A New Workload Family

Use this when the thing being tested is not naturally “tables plus tx”.
Examples:

- commitlog replay
- crash / reopen / durability
- replication
- network partitions
- leader election

Do not force those into `table_ops`.

Instead, add a new workload family under `workload/`, for example:

- `workload/commitlog_ops/`
- `workload/replication_ops/`

That workload family should define its own:

- case type
- interaction enum
- outcome type
- properties / invariants
- generator / stream planner
- runner helpers

Then add a target that executes that workload against the real implementation.

## Adding Commitlog Replay

Commitlog replay should be a new workload family, not another `table_ops`
scenario.

Good interaction examples:

- `Append`
- `Flush`
- `Fsync`
- `Crash`
- `Reopen`
- `Replay`
- `CheckDurablePrefix`
- `CheckReplayedState`

Good properties:

- replay restores the same durable prefix
- non-durable suffix is not reported as committed after reopen
- replay is deterministic for the same saved case
- snapshot plus replay matches replay-only, if snapshots exist

Suggested layout:

- `workload/commitlog_ops/`
- `targets/commitlog.rs`

If replay is exercised through `RelationalDB`, then use:

- `workload/commitlog_ops/`
- `targets/relational_db_lifecycle.rs`

But keep the workload family separate from `table_ops`.

## Adding Replication

Replication also should be its own workload family.

Good interaction examples:

- `ClientWrite`
- `Replicate`
- `DropMessage`
- `Partition`
- `HealPartition`
- `CrashReplica`
- `RestartReplica`
- `ElectLeader`
- `CheckReplicaState`

Good properties:

- committed prefix agreement
- no committed entry lost after restart
- followers do not apply invalid orderings
- replicas converge after heal
- read guarantees match the configured consistency level

Suggested layout:

- `workload/replication_ops/`
- `targets/replication.rs`

This target will likely need a composed cluster fixture rather than the
single-engine shape used by current table targets.

## Rule Of Thumb

- If the test subject is “a DB that executes table operations”, reuse
  `table_ops`.
- If the test subject is “a system with lifecycle, log, or network events”,
  make a new workload family.

## Current Scope

This crate provides shared table workload generation, concrete targets
(`datastore` and `relational_db_commitlog`), and a small CLI for seeded or
duration-bounded runs.
