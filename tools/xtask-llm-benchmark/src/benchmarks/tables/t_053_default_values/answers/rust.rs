use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = widget, public)]
pub struct Widget {
    #[primary_key]
    id: u64,
    name: String,
    #[default(true)]
    enabled: bool,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.widget().insert(Widget { id: 1, name: "legacy".into(), enabled: true });
}

#[spacetimedb::reducer]
pub fn touch(_ctx: &ReducerContext) {}
