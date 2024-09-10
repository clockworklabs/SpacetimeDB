use spacetimedb::{log_stopwatch::LogStopwatch, ReducerContext, Table};

#[spacetimedb::table(name = location, index(name = coordinates, btree(columns = [x, z, dimension])))]
#[derive(Debug, PartialEq, Eq)]
pub struct Location {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub chunk: u64,
    #[index(btree)]
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

// 1000 chunks, 1200 rows per chunk = 1.2M rows
const NUM_CHUNKS: u64 = 1000;
const ROWS_PER_CHUNK: u64 = 1200;

#[spacetimedb::reducer]
pub fn load_location_table(ctx: &ReducerContext) {
    for chunk in 0u64..NUM_CHUNKS {
        for i in 0u64..ROWS_PER_CHUNK {
            let id = chunk * 1200 + i;
            let x = 0i32;
            let z = chunk as i32;
            let dimension = id as u32;
            ctx.db.location().insert(Location {
                id,
                chunk,
                x,
                z,
                dimension,
            });
        }
    }
}

const ID: u64 = 989_987;
const CHUNK: u64 = ID / ROWS_PER_CHUNK;

#[spacetimedb::reducer]
/// Probing a single column index for a single row should be fast!
pub fn test_index_scan_on_id(ctx: &ReducerContext) {
    let span = LogStopwatch::new("Index scan on {id}");
    let location = ctx.db.location().id().find(ID).unwrap();
    span.end();
    assert_eq!(ID, location.id);
}

#[spacetimedb::reducer]
/// Scanning a single column index for `ROWS_PER_CHUNK` rows should also be fast!
pub fn test_index_scan_on_chunk(ctx: &ReducerContext) {
    let span = LogStopwatch::new("Index scan on {chunk}");
    let n = ctx.db.location().chunk().filter(&CHUNK).count();
    span.end();
    assert_eq!(n as u64, ROWS_PER_CHUNK);
}

#[spacetimedb::reducer]
/// Probing a multi-column index for a single row should be fast!
pub fn test_index_scan_on_x_z_dimension(ctx: &ReducerContext) {
    let z = CHUNK as i32;
    let dimension = ID as u32;
    let span = LogStopwatch::new("Index scan on {x, z, dimension}");
    let n = ctx
        .db
        .location()
        .iter()
        .filter(|r| r.x == 0 && r.z == z && r.dimension == dimension)
        .count();
    span.end();
    assert_eq!(n, 1);
}

#[spacetimedb::reducer]
/// Probing a multi-column index for `ROWS_PER_CHUNK` rows should also be fast!
pub fn test_index_scan_on_x_z(ctx: &ReducerContext) {
    let z = CHUNK as i32;
    let span = LogStopwatch::new("Index scan on {x, z}");
    let n = ctx.db.location().iter().filter(|r| r.x == 0 && r.z == z).count();
    span.end();
    assert_eq!(n as u64, ROWS_PER_CHUNK);
}
