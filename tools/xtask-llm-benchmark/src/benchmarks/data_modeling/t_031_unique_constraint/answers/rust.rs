use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = account)]
pub struct Account {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub email: String,
    pub display_name: String,
}

#[reducer]
pub fn create_account(ctx: &ReducerContext, email: String, display_name: String) {
    ctx.db.account().insert(Account {
        id: 0,
        email,
        display_name,
    });
}
