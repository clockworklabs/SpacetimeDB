use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}
