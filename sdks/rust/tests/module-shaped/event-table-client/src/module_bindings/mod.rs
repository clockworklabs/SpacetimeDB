// Module-shaped client bindings for `modules/sdk-test-event-table` (proposal 0040 spike).
// Hand-written as a single file: the module's declarations without function bodies, plus `client_module!`.

use spacetimedb_sdk as spacetimedb;

#[spacetimedb::table(accessor = test_event, public, event)]
pub struct TestEvent {
    pub name: String,
    pub value: u64,
}

#[spacetimedb::reducer]
pub fn emit_multiple_test_events(ctx: &spacetimedb::ReducerContext);

#[spacetimedb::reducer]
pub fn emit_test_event(ctx: &spacetimedb::ReducerContext, name: String, value: u64);

#[spacetimedb::reducer]
pub fn noop(ctx: &spacetimedb::ReducerContext);

spacetimedb::client_module! {
    tables: [test_event],
    reducers: [emit_multiple_test_events, emit_test_event(name, value), noop],
}
