use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::table(accessor = some_event, public, event)]
pub struct SomeEvent {
    account_id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn emit_event(ctx: &ReducerContext) {
    ctx.db.some_event().insert(SomeEvent {
        account_id: 7,
        name: "alpha".to_string(),
    });
}
