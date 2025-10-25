use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = users)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.users().insert(User { id: 1, name: "Alice".into(), age: 30, active: true });
    ctx.db.users().insert(User { id: 2, name: "Bob".into(),   age: 22, active: false });
}
