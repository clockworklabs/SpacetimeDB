use spacetimedb::{reducer, table, ReducerContext};

#[table(name = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer]
pub fn delete_user(ctx: &ReducerContext, id: i32) {
    ctx.db.user().id().delete(id);
}
