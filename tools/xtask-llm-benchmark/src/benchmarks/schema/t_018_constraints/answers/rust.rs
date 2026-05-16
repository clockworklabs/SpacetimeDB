use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(
    accessor = account,
    index(accessor = by_name, btree(columns = [name]))
)]
pub struct Account {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub email: String,
    pub name: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.account().insert(Account {
        id: 0,
        email: "a@example.com".into(),
        name: "Alice".into(),
    });
    ctx.db.account().insert(Account {
        id: 0,
        email: "b@example.com".into(),
        name: "Bob".into(),
    });
}
