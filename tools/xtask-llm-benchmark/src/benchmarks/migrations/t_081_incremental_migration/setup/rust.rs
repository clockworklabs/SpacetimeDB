use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = legacy_item, public)]
pub struct LegacyItem {
    #[primary_key]
    id: u64,
    value: String,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.legacy_item().insert(LegacyItem {
        id: 1,
        value: "old".into(),
    });
}
