use spacetimedb::{ReducerContext, Table};

#[spacetimedb::env]
pub struct Env {
    pub SMOKE_REQUIRED: String,
    #[env(values("ready", "other"))]
    pub SMOKE_MODE: String,
    pub SMOKE_OPTIONAL: Option<String>,
    pub SMOKE_EMPTY: Option<String>,
    pub SMOKE_NUMBER: Option<String>,
    pub SMOKE_FLAG: Option<String>,
}

#[spacetimedb::table(accessor = initial_environment, public)]
pub struct InitialEnvironment {
    required: String,
    mode: String,
    optional: Option<String>,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.initial_environment().insert(InitialEnvironment {
        required: ctx.env.SMOKE_REQUIRED(),
        mode: ctx.env.SMOKE_MODE(),
        optional: ctx.env.SMOKE_OPTIONAL(),
    });
}

#[spacetimedb::reducer]
pub fn check_environment(
    ctx: &ReducerContext,
    required: String,
    mode: String,
    optional: Option<String>,
    empty: Option<String>,
    number: Option<String>,
    flag: Option<String>,
) {
    assert_eq!(ctx.env.SMOKE_REQUIRED(), required);
    assert_eq!(ctx.env.SMOKE_MODE(), mode);
    assert_eq!(ctx.env.SMOKE_OPTIONAL(), optional);
    assert_eq!(ctx.env.SMOKE_EMPTY(), empty);
    assert_eq!(ctx.env.SMOKE_NUMBER(), number);
    assert_eq!(ctx.env.SMOKE_FLAG(), flag);
}
