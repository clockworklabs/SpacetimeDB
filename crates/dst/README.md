# `spacetimedb-dst`

Deterministic simulation testing for SpacetimeDB components.

DST is not a generic random fuzzer. It is a seed-replayable framework for
generating meaningful SpacetimeDB histories, executing them against real
implementation paths, and checking semantic properties while the run is still
in progress.

## First Principles

- A failing run must be reproducible from target, scenario, seed, run budget,
  and fault profile. Use `--max-interactions` for exact replay; `--duration` is
  a wall-clock soak budget and may stop at a different step count on another
  machine or runtime.
- Workloads describe legal but stressful user behavior. They should not depend
  on target internals.
- Targets execute interactions against real SpacetimeDB code.
- Properties check externally observable behavior, preferably against a simple
  model or a replayed durable history.
- Generation, execution, and property checking stay separate so failures are
  diagnosable as workload bugs, target bugs, or weak assertions.
- Runs stream interactions instead of materializing a full plan by default.
- Fault injection is explicit, configurable, and summarized in the outcome.
- Shared probability and weighting logic belongs in `workload::strategy`, not
  ad hoc scenario code.

## Current Architecture

The CLI selects a target, scenario, seed, budget, and fault profile. The shared
runner pulls one interaction at a time from a source, sends it to the target,
and asks the property runtime to observe the result.

```text
CLI -> TargetDescriptor -> WorkloadSource -> TargetEngine -> Observation
                                      \-> StreamingProperties -> Outcome
```

The core contracts are:

- `WorkloadSource`: deterministic pull-based interaction stream.
- `TargetEngine`: target-specific execution and outcome collection.
- `StreamingProperties`: reusable property checks over observations and target
  accessors.

## Client Model

DST workloads use shared logical client IDs rather than target-owned ad hoc
connection numbers. A `ClientId` is a stable actor in the generated history; a
`SessionId` is one live connection/session for that actor. A single client can
own multiple active sessions, which matters for reconnect, multi-tab, and future
replication traffic. Targets translate those IDs into their own handles:

- `relational-db-commitlog` maps `SessionId` to direct write/read transaction
  slots.
- future replication targets can map `SessionId` plus endpoint/node IDs to a
  client connection routed through the simulated network.

Concrete handles stay target-owned. Shared workloads should carry logical
identity and lifecycle intent, not `RelTx`, websocket handles, or target-specific
connection objects.

## Workload Composition

DST workloads use three building blocks:

- **Source:** emits a deterministic stream of interactions.
- **Profile:** configures weights, schema shape, and generation policy.
- **Layer:** wraps a source and adds lifecycle, fault, or cross-cutting
  interactions.

`table_ops` is the base table-transaction workload. `commitlog_ops` composes it
and injects durability lifecycle operations such as sync, close/reopen, dynamic
table create/migrate/drop, and replay checks.

Use this rule of thumb:

- Add a new profile when the interaction language is unchanged and only weights
  or schema shape differ.
- Add a new layer when you are adding lifecycle behavior around an existing
  source.
- Add a new workload family only when the interaction vocabulary is genuinely
  different.

## Table Operation Semantics

The table workload keeps the executable operation language small. Similar
cases converge into physical operations such as `InsertRows`, `DeleteRows`, and
`BeginTx`; the generated interaction also carries a case label for coverage and
debug output.

Correctness does not come from that label. The property runtime asks its model
what the physical operation should do:

- inserting fresh rows should mutate the table
- inserting an exact visible row should be an idempotent no-op
- inserting an existing primary id with a different payload should report a
  unique-key error
- deleting visible rows should mutate the table
- deleting absent rows should report a missing-row error
- beginning or writing behind another writer should report a write conflict
- query operations (`PointLookup`, `PredicateCount`, `RangeScan`, `FullScan`)
  should match the model-visible state

The case label still matters for summaries. It lets a run report that it hit
`ExactDuplicateInsert` or `UniqueKeyConflictInsert`, without teaching the target
or properties to trust generator-provided expectations.

## Current Targets

- `relational-db-commitlog`: runs table and commitlog lifecycle interactions
  against `RelationalDB`, local durability, dynamic schema operations,
  close/reopen, and replay-from-history checks.

## Properties

Properties live in `src/properties.rs` and are selected by target.
Table-oriented properties use `TargetPropertyAccess` so the property runtime can
ask a target for rows, counts, lookups, and range scans without knowing target
storage internals.

Current property families include:

- insert/select and delete/select checks
- observed error vs model-predicted error matching
- model-predicted no-op checks
- point lookup, predicate count, range scan, and full scan vs the table oracle
- NoREC-style optimizer-vs-direct checks
- TLP-style true/false/null partition checks
- index range exclusion checks
- banking mirror-table invariants
- dynamic migration auto-increment checks
- durable replay state vs the oracle committed model

## Fault Injection

`relational-db-commitlog` can wrap the in-memory commitlog repo in
`BuggifiedRepo`. Fault decisions are deterministic from the run seed and
summarized in the final outcome.

Profiles:

- `off`: no injected disk behavior.
- `light`: latency and occasional short I/O.
- `default`: stronger latency and short I/O pressure.
- `aggressive`: higher latency and short I/O rates. I/O error hooks exist but
  are currently disabled in profile-driven runs because local durability does
  not yet classify those errors as recoverable target outcomes.

## Running

Fast local run:

```bash
cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --seed 42 --max-interactions 200
```

Scenario examples:

```bash
cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --scenario banking --duration 5m
cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --scenario indexed-ranges --duration 5m
```

Run with commitlog faults:

```bash
cargo run -p spacetimedb-dst -- run \
  --target relational-db-commitlog \
  --seed 42 \
  --max-interactions 400 \
  --commitlog-fault-profile default
```

Trace every interaction:

```bash
RUST_LOG=trace cargo run -p spacetimedb-dst -- run --target relational-db-commitlog --duration 5m
```

## Run Budgets

Prefer `--max-interactions` when reporting or replaying a failure. It is the
deterministic interaction budget, so target, scenario, seed, interaction count,
and fault profile are enough to rerun the same generated stream.

Use `--duration` for local soaks. It is intentionally wall-clock based, so it
can stop after a different number of interactions if host speed, logging, or
runtime behavior changes.

## Reading The Code

Start here:

- `src/core/mod.rs`: source, engine, property, and runner traits.
- `src/workload/table_ops`: table interaction language, generation model, and
  scenarios.
- `src/workload/commitlog_ops`: lifecycle layer over table workloads.
- `src/sim/`: local executor and deterministic-decision shim.
- `src/properties.rs`: property catalog and oracle/model checks.
- `src/targets/relational_db_commitlog.rs`: target adapter for RelationalDB,
  commitlog durability, fault injection, close/reopen, and replay.
- `src/targets/buggified_repo.rs`: deterministic disk-like fault layer.

## Adding A New Target

1. Add a target engine in `src/targets/<name>.rs`.
2. Reuse an existing workload family or add `src/workload/<new_family>/`.
3. Return observations that are rich enough for properties to validate behavior.
4. Plug target-specific properties through `PropertyRuntime`.
5. Add a `TargetDescriptor` in `src/targets/descriptor.rs`.
6. Register the target in CLI `TargetKind`.

## Current Gaps

- No structured trace/replay format yet.
- No shrinker yet; seed replay is the current reproduction mechanism.
- Sometimes-property reporting is still outcome-counter based, not a stable
  property-event catalog.
- The local `sim` shim is not a real simulator yet. It owns executor setup and
  deterministic fault decisions so future simulator work has one boundary.
- The current `RelationalDB` target drives open read snapshots to release before
  starting writes, because beginning a write behind an open read snapshot can
  block in this target shape. Interleaved read/write snapshot histories should
  come back once the target models that lock behavior explicitly.
- Runtime-boundary work for scheduler, time, network, filesystem, and lower
  randomness sources is still future work.
