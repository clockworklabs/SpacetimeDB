# Event Tables Implementation Status

Tracking progress against the [event tables proposal](../SpacetimeDBPrivate/proposals/00XX-event-tables.md).

**Branches:**
- `tyler/impl-event-tables` -- full implementation (rebased on `phoebe/rust-sdk-ws-v2`)
- `tyler/event-tables-datastore` -- datastore-only subset (off `master`, PR [#4251](https://github.com/clockworklabs/SpacetimeDB/pull/4251))

## Implemented

### Server: Module-side (`#[table(..., event)]`)

- [x] `event` attribute on `#[table]` macro (`crates/bindings-macro/src/table.rs`)
- [x] `is_event` field on `TableSchema` propagated through schema validation (`crates/schema/`)
- [x] `RawModuleDefV10` includes `is_event` in table definitions (`crates/lib/src/db/raw_def/v10.rs`). Note: V9 does not support event tables.
- [x] Event table rows are recorded as inserts in `tx_data` at commit time but NOT merged into committed state (`crates/datastore/src/locking_tx_datastore/committed_state.rs`)
- [x] Commitlog replay treats event table inserts as noops -- `replay_insert()` returns early when `schema.is_event` (`committed_state.rs`)
- [x] Event tables function as normal tables during reducer execution (insert/delete/update within a transaction)

### Server: Datastore Unit Tests (PR #4251)

- [x] Insert + delete in same tx cancels out -- no TxData entry, no committed state (`test_event_table_insert_delete_noop`)
- [x] Update (delete + re-insert) leaves only the final row in TxData, nothing in committed state (`test_event_table_update_only_final_row`)
- [x] Bare insert records in TxData but not committed state (`test_event_table_insert_records_txdata_not_committed_state`)
- [x] `replay_insert()` is a no-op for event tables (`test_event_table_replay_ignores_inserts`)

### Server: Migration Validation (PR #4251)

- [x] `ChangeTableEventFlag` error variant in `auto_migrate.rs` prevents changing `is_event` between module versions
- [x] Tests: non-event -> event and event -> non-event both rejected; same flag accepted (`test_change_event_flag_rejected`, `test_same_event_flag_accepted`)

### Server: Subscriptions & Query Engine

- [x] `SELECT * FROM *` excludes event tables (`crates/core/src/subscription/subscription.rs`)
- [x] `CanBeLookupTable` trait: event tables do NOT implement it, preventing use as the right/lookup table in view semijoins (`crates/bindings-macro/src/table.rs`, `crates/query-builder/src/table.rs`)
- [x] `reads_from_event_table()` check in `SubscriptionPlan::compile()` rejects event tables as lookup tables in subscription joins (`crates/subscription/src/lib.rs`). The proposal notes that joining on event tables is well-defined (noops), but it is restricted for now for ease of implementation.
- [x] View definitions cannot select from event tables at runtime (`crates/core/src/host/wasm_common/module_host_actor.rs`). The proposal says to disallow event table access in the query builder entirely for now.

### Server: V1 Protocol Compatibility

- [x] V1 WebSocket subscriptions to event tables are rejected with a clear error message directing developers to upgrade (`crates/core/src/subscription/module_subscription_actor.rs`)
  - Enforced in all three V1 paths: `SubscribeSingle`, `SubscribeMulti`, and legacy `Subscribe`
- [x] `returns_event_table()` methods on `SubscriptionPlan` and `Plan` for detecting event table subscriptions

### Server: V2 Subscription Path

- [x] Event tables work correctly through the v2 subscription evaluation path -- verified end-to-end with v2 client subscribing, calling reducer, receiving `on_insert` callback
- [x] Merged `jsdt/ws-v2` (server-side v2 protocol) and `phoebe/rust-sdk-ws-v2` (client-side v2 SDK) into `tyler/impl-event-tables`

### Client: Rust SDK

- [x] `EventTable` trait and `TableHandle` implementation that skips client cache storage (`sdks/rust/src/table.rs`)
- [x] `is_event` flag on `TableMetadata` in client cache (`sdks/rust/src/client_cache.rs`)
- [x] Codegen generates `EventTable` impl (insert-only, no delete callbacks) for event tables (`crates/codegen/src/rust.rs`). The proposal says to generate both `on_insert` and `on_delete` (see Deferred section below).
- [x] `on_insert` callbacks fire for event table rows; `count()` and `iter()` always return empty
- [x] `spacetime_module.rs` exposes `is_event` in generated `SpacetimeModule` trait
- [x] SDK uses v2 WebSocket protocol by default (`ws::v2::BIN_PROTOCOL` in `sdks/rust/src/websocket.rs`)

### Reducer Callback Deprecation (2.0 -- primary goal of proposal)

The proposal's primary motivation is deprecating reducer event callbacks and replacing them with event tables. This is implemented in the v2 SDK:

- [x] V2 server does not publish `ReducerEvent` messages to clients (commit `fd3ef210f`)
- [x] V2 codegen does not generate `ctx.reducers.on_<reducer>()` callbacks; replaced with `_then()` pattern for per-call result callbacks
- [x] `CallReducerFlags` / `NoSuccessNotify` removed from v2 SDK (proposal recommends deprecation)

### Compile-time Checks (trybuild)

- [x] Event tables cannot be the lookup (right) table in `left_semijoin` (`crates/bindings/tests/ui/views.rs`)
- [x] Event tables cannot be the lookup (right) table in `right_semijoin`
- [x] Event tables CAN be the left/outer table in a semijoin (positive compile test -- would still be blocked at runtime by the view validation check)

### Integration Tests

All event table integration tests are active and passing on the v2 SDK:

- [x] Basic event table test: `on_insert` fires, row values match, cache stays empty (`event-table`)
- [x] Multiple events in one reducer test: 3 inserts in one reducer all arrive as callbacks (`multiple-events`)
- [x] Events don't persist across transactions test: event count doesn't grow after a subsequent noop reducer (`events-dont-persist`)

The V1 rejection test (`exec_v1_rejects_event_table`) exists in the test client source but is not wired up in `test.rs`, since the SDK now exclusively uses v2. The V1 rejection server logic remains in place for any V1 clients that may connect.

Test module: `modules/sdk-test-event-table/src/lib.rs`
Test client: `sdks/rust/tests/event-table-client/src/main.rs`
Test harness: `sdks/rust/tests/test.rs` (`event_table_tests` module)

### Proposal Document

- [x] WebSocket Protocol Compatibility section added to proposal (`SpacetimeDBPrivate/proposals/00XX-event-tables.md`)

## Deferred

Items that are well-defined and implementable but deliberately deferred per the proposal.

### `on_delete` Codegen

The proposal says to generate both `on_insert` and `on_delete` for event tables. Since the server only sends inserts (the optimization described in the proposal), the client would need to synthesize `on_delete` from `on_insert` since every event table row is a noop. This is deferred for now and can be added later without breaking changes. See proposal section "Client-side API".

### Event Tables in Subscription Joins

The proposal notes that using event tables as the right/inner/lookup table in subscription joins is well-defined (noops make it so), and that event-table-ness is "infectious" -- joined results behave as event tables too. Currently blocked by the `reads_from_event_table()` check in `SubscriptionPlan::compile()` and the `CanBeLookupTable` compile-time trait. This restriction exists for ease of implementation and can be relaxed later.

### Event Tables in Views

The proposal says event tables MAY be accessible in view functions but cannot return rows. Currently disallowed both at compile time (via `CanBeLookupTable`) and runtime (view validation check). The proposal suggests this will be allowed in the future, potentially with implicit "infectious" event-table-ness for views that join on event tables.

## Not Yet Implemented

### Server: Untested Behaviors

These are described in the proposal as expected to work but have no dedicated tests:

- [ ] RLS (Row-Level Security) on event tables -- proposal says RLS should apply with same semantics as non-event tables
- [ ] Primary key, unique constraints, indexes on event tables -- proposal says these should work within a single transaction
- [ ] Sequences and `auto_inc` on event tables -- proposal says these should work

### Server: Module-side for Other Languages

- [ ] TypeScript module bindings: `event` attribute on table declarations
- [ ] C# module bindings: `event` attribute on table declarations

### Client: Other SDKs

- [ ] TypeScript SDK: `EventTable` support and client codegen
- [ ] C# SDK: `EventTable` support and client codegen
- [ ] C++ SDK: `EventTable` support and client codegen

### Documentation

- [ ] Migration guide from reducer callbacks to event tables (proposal includes examples in "Reducer Callback Compatibility" section but no standalone guide exists)

### Proposal: Future Work Items (not blocking 2.0)

- [ ] `#[table]` attribute on reducer functions (auto-generate event table from reducer args) -- proposal "Reducer Event Table" section
- [ ] `ctx.on.<event_name>()` convenience syntax on client -- proposal ".on Syntax" section
- [ ] Event tables in views / infectious event-table-ness for joined views -- proposal "Views" section
- [ ] TTL tables as generalization of event tables (`ttl = Duration::ZERO`) -- proposal "TTLs, Temporal Filters..." section
- [ ] Temporal filters -- proposal "TTLs, Temporal Filters..." section
- [ ] Light mode deprecation (2.0) -- proposal "Light mode deprecation" section

## Known Issues

### V2 SDK Test Stability

The broader (non-event-table) SDK test suite has intermittent failures when running on the v2 protocol branches. These manifest as:
- `subscribe_all_select_star` and `fail_reducer` intermittent failures in Rust SDK tests
- Test timeouts under parallel execution

These are pre-existing issues in the v2 WebSocket implementation, not caused by event tables. The event table tests themselves pass reliably.

### `request_id` Bug (Fixed)

`sdks/rust/src/db_connection.rs:383` had `request_id: 0` hardcoded instead of using the generated `request_id` variable. This caused "Reducer result for unknown request_id 0" errors for all reducer calls. Fixed in commit `43d84a277` on this branch and `f2f385a28` on `phoebe/rust-sdk-ws-v2`.

## Key Files

| Area | File |
|------|------|
| Table macro | `crates/bindings-macro/src/table.rs` |
| Schema | `crates/schema/src/schema.rs` (`is_event` field) |
| Migration validation | `crates/schema/src/auto_migrate.rs` (`ChangeTableEventFlag`) |
| Committed state | `crates/datastore/src/locking_tx_datastore/committed_state.rs` |
| Datastore unit tests | `crates/datastore/src/locking_tx_datastore/datastore.rs` |
| Subscription filtering | `crates/core/src/subscription/subscription.rs` |
| V1 rejection | `crates/core/src/subscription/module_subscription_actor.rs` |
| Plan helpers | `crates/core/src/subscription/module_subscription_manager.rs`, `crates/subscription/src/lib.rs` |
| Query builder | `crates/query-builder/src/table.rs` (`CanBeLookupTable`) |
| Physical plan | `crates/physical-plan/src/plan.rs` (`reads_from_event_table`) |
| View validation | `crates/core/src/host/wasm_common/module_host_actor.rs` |
| Rust codegen | `crates/codegen/src/rust.rs` |
| Rust SDK | `sdks/rust/src/table.rs`, `sdks/rust/src/client_cache.rs` |
| Test module | `modules/sdk-test-event-table/src/lib.rs` |
| Test client | `sdks/rust/tests/event-table-client/src/main.rs` |
| Test harness | `sdks/rust/tests/test.rs` (`event_table_tests` module) |
| Compile tests | `crates/bindings/tests/ui/views.rs` |
| Proposal | `../SpacetimeDBPrivate/proposals/00XX-event-tables.md` |
