# Event Tables Implementation Status

Tracking progress against the [event tables proposal](../../SpacetimeDBPrivate/proposals/00XX-event-tables.md).

## Implemented

### Server: Module-side (`#[table(..., event)]`)

- [x] `event` attribute on `#[table]` macro (`crates/bindings-macro/src/table.rs`)
- [x] `is_event` field on `TableSchema` propagated through schema validation (`crates/schema/`)
- [x] `RawModuleDefV10` includes `is_event` in table definitions (`crates/lib/src/db/raw_def/v10.rs`)
- [x] Event table rows are recorded as inserts in `tx_data` at commit time but NOT merged into committed state (`crates/datastore/src/locking_tx_datastore/committed_state.rs`)
- [x] Event tables function as normal tables during reducer execution (insert/delete/update within a transaction)

### Server: Subscriptions & Query Engine

- [x] `SELECT * FROM *` excludes event tables (`crates/core/src/subscription/subscription.rs`)
- [x] `CanBeLookupTable` trait: event tables do NOT implement it, preventing use as the right/lookup table in semijoins (`crates/bindings-macro/src/table.rs`, `crates/query-builder/src/table.rs`)
- [x] `reads_from_event_table()` check in `SubscriptionPlan::compile()` rejects event tables as lookup tables in subscription joins (`crates/subscription/src/lib.rs`)
- [x] View definitions cannot select from event tables at runtime (`crates/core/src/host/wasm_common/module_host_actor.rs`)

### Server: V1 Protocol Compatibility

- [x] V1 WebSocket subscriptions to event tables are rejected with a clear error message directing developers to upgrade (`crates/core/src/subscription/module_subscription_actor.rs`)
  - Enforced in all three V1 paths: `SubscribeSingle`, `SubscribeMulti`, and legacy `Subscribe`
- [x] `returns_event_table()` methods on `SubscriptionPlan` and `Plan` for detecting event table subscriptions

### Client: Rust SDK (v1 only)

- [x] `EventTable` trait and `TableHandle` implementation that skips client cache storage (`sdks/rust/src/table.rs`)
- [x] `is_event` flag on `TableMetadata` in client cache (`sdks/rust/src/client_cache.rs`)
- [x] Codegen generates `EventTable` impl for event tables (`crates/codegen/src/rust.rs`)
- [x] `on_insert` callbacks fire for event table rows; `count()` and `iter()` always return empty
- [x] `spacetime_module.rs` exposes `is_event` in generated `SpacetimeModule` trait

### Compile-time Checks (trybuild)

- [x] Event tables cannot be the lookup (right) table in `left_semijoin` (`crates/bindings/tests/ui/views.rs`)
- [x] Event tables cannot be the lookup (right) table in `right_semijoin`
- [x] Event tables CAN be the left/outer table in a semijoin (positive test)

### Integration Tests

- [x] V1 rejection test: verifies v1 clients get a subscription error with upgrade message
- [x] Basic event table test: `on_insert` fires, row values match, cache stays empty (written but `#[ignore]` — needs v2 SDK)
- [x] Multiple events in one reducer test (written but `#[ignore]` — needs v2 SDK)
- [x] Events don't persist across transactions test (written but `#[ignore]` — needs v2 SDK)

### Proposal Document

- [x] WebSocket Protocol Compatibility section added to proposal (`SpacetimeDBPrivate/proposals/00XX-event-tables.md`)

## Not Yet Implemented

### Client: Rust SDK v2 WebSocket Support

- [ ] Add `WsVersion` field to `WsParams` and `DbConnectionBuilder`
- [ ] Add `with_ws_version()` builder method
- [ ] Update `request_insert_protocol_header()` to use `ws_v2::BIN_PROTOCOL` when v2 is selected
- [ ] Handle v2 server messages (subscription responses, transaction updates)
- [ ] Re-enable `#[ignore]`'d event table integration tests once v2 is working

### Client: Other SDKs

- [ ] TypeScript SDK: `EventTable` support and codegen
- [ ] C# SDK: `EventTable` support and codegen
- [ ] C++ SDK: `EventTable` support and codegen

### Server: Codegen for Other Languages

- [ ] TypeScript codegen for event tables
- [ ] C# codegen for event tables

### Server: V2 Subscription Path for Event Tables

- [ ] Verify event tables work correctly through the v2 subscription evaluation path (`eval_updates_sequential_inner_v2`)
- [ ] End-to-end test with a v2 client subscribing to event tables

### Proposal: Future Work Items (not blocking 2.0)

- [ ] `#[table]` attribute on reducer functions (auto-generate event table from reducer args)
- [ ] `ctx.on.<event_name>()` convenience syntax on client
- [ ] Event tables in views / infectious event-table-ness for joined views
- [ ] TTL tables as generalization of event tables (`ttl = Duration::ZERO`)
- [ ] Temporal filters
- [ ] Light mode deprecation (2.0)
- [ ] `CallReducerFlags` / `NoSuccessNotify` deprecation (2.0)

## Key Files

| Area | File |
|------|------|
| Table macro | `crates/bindings-macro/src/table.rs` |
| Schema | `crates/schema/src/schema.rs` (`is_event` field) |
| Committed state | `crates/datastore/src/locking_tx_datastore/committed_state.rs` |
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
| Test harness | `sdks/rust/tests/test.rs` (event_table_tests module) |
| Compile tests | `crates/bindings/tests/ui/views.rs` |
| Proposal | `SpacetimeDBPrivate/proposals/00XX-event-tables.md` |
