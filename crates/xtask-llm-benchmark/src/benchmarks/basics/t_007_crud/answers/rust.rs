use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = users)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[reducer]
pub fn crud(ctx: &ReducerContext) {
    ctx.db.users().insert(User { id: 1, name: "Alice".into(), age: 30, active: true });
    ctx.db.users().insert(User { id: 2, name: "Bob".into(),   age: 22, active: false });
    ctx.db.users().id().update(User { id: 1, name: "Alice2".into(), age: 31, active: false });
    ctx.db.users().id().delete(2);
}
