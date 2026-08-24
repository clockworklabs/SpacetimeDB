use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = legacy_item, public)]
pub struct LegacyItem {
    #[primary_key]
    id: u64,
    value: String,
}

#[spacetimedb::table(accessor = item_v2, public)]
pub struct ItemV2 {
    #[primary_key]
    id: u64,
    value: String,
    version: u32,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.legacy_item().insert(LegacyItem {
        id: 1,
        value: "old".into(),
    });
}

#[spacetimedb::reducer]
pub fn migrate(ctx: &ReducerContext) {
    for row in ctx.db.legacy_item().iter() {
        if ctx.db.item_v2().id().find(row.id).is_none() {
            ctx.db.item_v2().insert(ItemV2 {
                id: row.id,
                value: row.value,
                version: 2,
            });
        }
    }
}

#[spacetimedb::reducer]
pub fn dual_write(ctx: &ReducerContext, id: u64, value: String) {
    ctx.db.legacy_item().insert(LegacyItem {
        id,
        value: value.clone(),
    });
    ctx.db.item_v2().insert(ItemV2 { id, value, version: 2 });
}
