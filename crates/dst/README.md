# `spacetimedb-dst`

Deterministic simulation testing utilities for SpacetimeDB.

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
  `datastore.rs`, `relational_db.rs`
- binary:
  `src/main.rs`

## Reading Order

If you are new to the crate, this order keeps the mental model small:

1. `src/main.rs`
2. `config.rs`
3. `seed.rs`
4. `workload/table_ops/`
5. `targets/datastore.rs`
6. `targets/relational_db.rs`

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
   common scenario, interaction, event, outcome, and engine traits
2. `properties.rs`
   first-class properties such as visibility, row-count, and banking table
   matching
3. `scenarios/`
   scenario-specific schema generation like `random_crud` and `banking`
4. `model.rs`
   generator model and expected-state model
5. `generation.rs`
   `InteractionStream` and scenario-aware workload planning
6. `runner.rs`
   generic execute/run helpers shared by multiple targets

Concrete targets like `targets/datastore.rs` and `targets/relational_db.rs`
reuse that workload and swap in target-specific engines.

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
cargo run -p spacetimedb-dst -- run --target relational-db --seed 42 --max-interactions 2000
cargo run -p spacetimedb-dst -- replay --target datastore bug.json
cargo run -p spacetimedb-dst -- shrink --target datastore bug.json
```

DST workloads are run from CLI only. Use `random-crud` for broad coverage and
`indexed-ranges` when you want to bias toward secondary/composite index range
behavior without hardcoding a single historical bug.

## Current Scope

This crate provides shared table workload generation, two concrete targets
(`datastore` and `relational_db`), and a small CLI for seeded or
duration-bounded runs.
