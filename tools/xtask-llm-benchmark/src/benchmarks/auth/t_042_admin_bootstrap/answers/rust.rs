use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = config)]
pub struct Config {
    #[primary_key]
    pub id: u32,
    pub admin: Identity,
}

#[table(accessor = admin_log, public)]
pub struct AdminLog {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub action: String,
}

#[reducer]
pub fn bootstrap_admin(ctx: &ReducerContext) {
    if ctx.db.config().id().find(0).is_some() {
        panic!("already bootstrapped");
    }
    ctx.db.config().insert(Config {
        id: 0,
        admin: ctx.sender(),
    });
}

#[reducer]
pub fn admin_action(ctx: &ReducerContext, action: String) {
    let config = ctx.db.config().id().find(0).expect("not bootstrapped");
    if ctx.sender() != config.admin {
        panic!("not admin");
    }
    ctx.db.admin_log().insert(AdminLog { id: 0, action });
}
