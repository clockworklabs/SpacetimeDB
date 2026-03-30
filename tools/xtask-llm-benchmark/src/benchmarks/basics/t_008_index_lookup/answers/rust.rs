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

#[table(accessor = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

#[reducer]
pub fn insert_user(ctx: &ReducerContext, name: String, age: i32, active: bool) {
    ctx.db.user().insert(User { id: 0, name, age, active });
}

#[reducer]
pub fn lookup_user_name(ctx: &ReducerContext, id: u64) {
    if let Some(u) = ctx.db.user().id().find(id) {
        ctx.db.result().insert(ResultRow { id: u.id, name: u.name });
    }
}
