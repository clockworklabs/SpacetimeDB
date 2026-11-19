use spacetimedb::{ReducerContext, Table};

// Only this table may spill all the way to the remote tier. Large, distinct
// blobs let a small number of reducer calls exceed both backing-store budgets.
#[spacetimedb::table(accessor = object_row, public, cache_policy = no_requirement)]
pub struct ObjectRow {
    #[primary_key]
    id: u32,
    payload: String,
}

// Keep a small ordinary table alongside the evictable data. Its rows must
// remain usable while other tables are faulting objects back into memory.
#[spacetimedb::table(accessor = marker, public)]
pub struct Marker {
    #[primary_key]
    id: u32,
}

fn payload(id: u32) -> String {
    // The prefix prevents content-addressed deduplication across different rows.
    format!("{id}:{}", "x".repeat(1024 * 1024))
}

#[spacetimedb::reducer]
pub fn insert(ctx: &ReducerContext, id: u32) {
    ctx.db.object_row().insert(ObjectRow {
        id,
        payload: payload(id),
    });
}

#[spacetimedb::reducer]
pub fn check(ctx: &ReducerContext, id: u32) {
    // Verify the whole blob inside the module, instead of round-tripping through SQL.
    let row = ctx.db.object_row().id().find(id).expect("missing object row");
    assert_eq!(row.payload, payload(id), "incorrect payload for row {id}");
}

#[spacetimedb::reducer]
pub fn insert_marker(ctx: &ReducerContext, id: u32) {
    ctx.db.marker().insert(Marker { id });
}

// Insert into a second table first so the smoke test can observe rollback of
// an earlier write when the later primary-key constraint fails.
#[spacetimedb::reducer]
pub fn insert_conflicting(ctx: &ReducerContext, id: u32) {
    ctx.db.marker().insert(Marker { id });
    ctx.db.object_row().insert(ObjectRow {
        id,
        payload: "conflicting payload".to_owned(),
    });
}
