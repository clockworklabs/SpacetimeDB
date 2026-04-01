use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub active: bool,
}

#[table(accessor = user_stats)]
pub struct UserStats {
    #[primary_key]
    pub key: String,
    pub count: u64,
}

#[reducer]
pub fn compute_user_counts(ctx: &ReducerContext) {
    let total = ctx.db.user().iter().count() as u64;
    let active = ctx.db.user().iter().filter(|u| u.active).count() as u64;

    ctx.db.user_stats().insert(UserStats {
        key: "total".to_string(),
        count: total,
    });
    ctx.db.user_stats().insert(UserStats {
        key: "active".to_string(),
        count: active,
    });
}
