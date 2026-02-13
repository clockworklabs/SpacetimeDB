use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer]
pub fn insert_user(ctx: &ReducerContext, id: i32, name: String, age: i32, active: bool) -> Result<(), String> {
    ctx.db.user().insert(User { id, name, age, active });
    Ok(())
}
