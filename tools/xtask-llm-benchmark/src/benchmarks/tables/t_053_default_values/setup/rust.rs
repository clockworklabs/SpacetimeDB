use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = widget, public)]
pub struct Widget {
    #[primary_key]
    id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.widget().insert(Widget {
        id: 1,
        name: "legacy".into(),
    });
}

#[spacetimedb::reducer]
pub fn touch(_ctx: &ReducerContext) {}
