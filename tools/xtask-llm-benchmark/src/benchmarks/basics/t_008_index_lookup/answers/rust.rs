use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = users)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(name = results)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub name: String,
}

#[reducer]
pub fn lookup_user_name(ctx: &ReducerContext, id: i32) {
    if let Some(u) = ctx.db.users().id().find(id) {
        ctx.db.results().insert(ResultRow { id: u.id, name: u.name });
    }
}
