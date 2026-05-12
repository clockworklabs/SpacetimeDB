use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer]
pub fn insert_user(ctx: &ReducerContext, name: String, age: i32, active: bool) {
    ctx.db.user().insert(User {
        id: 0,
        name,
        age,
        active,
    });
}

#[reducer]
pub fn delete_user(ctx: &ReducerContext, id: u64) {
    ctx.db.user().id().delete(id);
}
