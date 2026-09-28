use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
pub fn noop(_ctx: &ReducerContext) {}
