use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = test_event, public, event)]
pub struct TestEvent {
    pub name: String,
    pub value: u64,
}

#[spacetimedb::reducer]
pub fn emit_test_event(ctx: &ReducerContext, name: String, value: u64) {
    ctx.db.test_event().insert(TestEvent { name, value });
}

#[spacetimedb::reducer]
pub fn emit_multiple_test_events(ctx: &ReducerContext) {
    ctx.db.test_event().insert(TestEvent {
        name: "a".to_string(),
        value: 1,
    });
    ctx.db.test_event().insert(TestEvent {
        name: "b".to_string(),
        value: 2,
    });
    ctx.db.test_event().insert(TestEvent {
        name: "c".to_string(),
        value: 3,
    });
}

/// A no-op reducer that lets us observe a subsequent transaction.
#[spacetimedb::reducer]
pub fn noop(_ctx: &ReducerContext) {}
