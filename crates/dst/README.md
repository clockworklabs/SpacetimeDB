# `spacetimedb-dst`

Deterministic simulation testing for SpacetimeDB targets.

## How DST Works

DST is CLI-first and interaction-stream based:

1. CLI picks `target`, `scenario`, `seed`, and run budget.
2. A workload generator emits `next_interaction()` deterministically.
3. The target engine executes each interaction on a real implementation.
4. Target properties validate behavior during the run and at finish.
5. Run stops on first failure or budget expiry (`--duration` / `--max-interactions`).

There is no case materialization/replay path in the current crate. All runs are
generated and executed as a deterministic stream.

## Current Targets

- `datastore`
- `relational-db-commitlog`

Both targets reuse shared workload families and share the same streaming runner.

## Workload Families

- `workload/table_ops`: transactional table operations (create schema, insert,
  delete, begin/commit/rollback patterns).
- `workload/commitlog_ops`: composes `table_ops` and injects lifecycle/chaos
  operations (sync/close-reopen/dynamic-table ops) for commitlog durability
  testing.

## Properties

Properties are target-owned and reusable across targets via
`targets/properties.rs`. A target chooses which property kinds to enable and
applies them through a shared `PropertyRuntime`.

Examples:

- `PQS::InsertSelect`
- `DeleteSelect`
- `NoREC::SelectSelectOptimizer`
- `TLP::WhereTrueFalseNull`
- `IndexRangeExcluded`
- `BankingTablesMatch`

## CLI

```bash
cargo run -p spacetimedb-dst -- run --target datastore --scenario banking --duration 5m
cargo run -p spacetimedb-dst -- run --target datastore --scenario indexed-ranges --duration 5m
cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --seed 42 --max-interactions 2000
```

Trace every interaction:

```bash
RUST_LOG=trace cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --duration 5m
```

## Adding A New Target

1. Add a target engine in `src/targets/<name>.rs`.
2. Reuse an existing workload family or add `src/workload/<new_family>/`.
3. Plug target-specific properties through `PropertyRuntime`.
4. Add a `TargetDescriptor` in `src/targets/descriptor.rs`.
5. Register in CLI `TargetKind`.

Use `table_ops` when semantics are table-transaction oriented. Add a new
workload family when you need lifecycle/network/replication semantics.
