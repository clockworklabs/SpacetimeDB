use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub name: String,
}

#[reducer]
pub fn lookup_user_name(ctx: &ReducerContext, id: i32) {
    if let Some(u) = ctx.db.user().id().find(id) {
        ctx.db.result().insert(ResultRow { id: u.id, name: u.name });
    }
}
