use spacetimedb::{reducer, table, ReducerContext};

#[table(name = users)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer]
pub fn update_user(ctx: &ReducerContext, id: i32, name: String, age: i32, active: bool) {
    ctx.db.users().id().update(User { id, name, age, active });
}