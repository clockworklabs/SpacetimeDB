use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = legacy_item, public)]
pub struct LegacyItem {
    #[primary_key]
    pub id: u64,
    pub value: String,
    pub revision: u64,
    pub deleted: bool,
}

#[table(accessor = backfill_snapshot, public)]
pub struct BackfillSnapshot {
    #[primary_key]
    pub id: u64,
    pub value: String,
    pub revision: u64,
    pub deleted: bool,
}

#[table(accessor = item_v2, public)]
pub struct ItemV2 {
    #[primary_key]
    pub id: u64,
    pub value: String,
    pub revision: u64,
}

#[reducer]
pub fn write_item(ctx: &ReducerContext, id: u64, value: String) -> Result<(), String> {
    let revision = ctx.db.legacy_item().id().find(id).map_or(1, |r| r.revision + 1);
    let row = LegacyItem {
        id,
        value: value.clone(),
        revision,
        deleted: false,
    };
    if ctx.db.legacy_item().id().find(id).is_some() {
        ctx.db.legacy_item().id().update(row);
    } else {
        ctx.db.legacy_item().insert(row);
    }
    let next = ItemV2 { id, value, revision };
    if ctx.db.item_v2().id().find(id).is_some() {
        ctx.db.item_v2().id().update(next);
    } else {
        ctx.db.item_v2().insert(next);
    }
    Ok(())
}

#[reducer]
pub fn seed_legacy(ctx: &ReducerContext, id: u64, value: String) -> Result<(), String> {
    ctx.db.legacy_item().insert(LegacyItem {
        id,
        value,
        revision: 1,
        deleted: false,
    });
    Ok(())
}

#[reducer]
pub fn delete_item(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if let Some(mut row) = ctx.db.legacy_item().id().find(id) {
        if !row.deleted {
            row.revision += 1;
            row.deleted = true;
            ctx.db.legacy_item().id().update(row);
            ctx.db.item_v2().id().delete(id);
        }
    }
    Ok(())
}

#[reducer]
pub fn capture_batch(ctx: &ReducerContext, after_id: u64, limit: u64) -> Result<(), String> {
    if limit == 0 || limit > 10 {
        return Err("invalid limit".into());
    }
    let mut rows: Vec<_> = ctx.db.legacy_item().iter().filter(|r| r.id > after_id).collect();
    rows.sort_by_key(|r| r.id);
    for row in ctx.db.backfill_snapshot().iter() {
        ctx.db.backfill_snapshot().id().delete(row.id);
    }
    for r in rows.into_iter().take(limit as usize) {
        ctx.db.backfill_snapshot().insert(BackfillSnapshot {
            id: r.id,
            value: r.value,
            revision: r.revision,
            deleted: r.deleted,
        });
    }
    Ok(())
}

#[reducer]
pub fn apply_batch(ctx: &ReducerContext) -> Result<(), String> {
    for snapshot in ctx.db.backfill_snapshot().iter() {
        let r = ctx.db.legacy_item().id().find(snapshot.id).ok_or("missing source")?;
        if r.deleted {
            ctx.db.item_v2().id().delete(r.id);
        } else {
            let next = ItemV2 {
                id: r.id,
                value: r.value,
                revision: r.revision,
            };
            if ctx.db.item_v2().id().find(r.id).is_some() {
                ctx.db.item_v2().id().update(next);
            } else {
                ctx.db.item_v2().insert(next);
            }
        }
    }
    Ok(())
}
