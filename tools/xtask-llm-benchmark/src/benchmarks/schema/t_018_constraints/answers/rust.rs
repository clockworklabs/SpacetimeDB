use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(
    name = account,
    index(name = by_name, btree(columns = [name]))
)]
pub struct Account {
    #[primary_key]
    pub id: i32,
    #[unique]
    pub email: String,
    pub name: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.account().insert(Account { id: 1, email: "a@example.com".into(), name: "Alice".into() });
    ctx.db.account().insert(Account { id: 2, email: "b@example.com".into(), name: "Bob".into() });
}
