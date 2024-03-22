use spacetimedb::{query, spacetimedb, time_span::Span};

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "x", x))]
#[spacetimedb(index(btree, name = "chunk", chunk))]
#[spacetimedb(index(btree, name = "coordinates", x, z, dimension))]
#[derive(Debug, PartialEq, Eq)]
pub struct Location {
    #[primarykey]
    pub id: u64,
    pub chunk: u64,
    pub x: i32,
    pub z: i32,
    pub dimension: u32,
}

// 1000 chunks, 1200 rows per chunk = 1.2M rows
const NUM_CHUNKS: u64 = 1000;
const ROWS_PER_CHUNK: u64 = 1200;

#[spacetimedb(reducer)]
pub fn load_location_table() {
    for chunk in 0u64..NUM_CHUNKS {
        for i in 0u64..ROWS_PER_CHUNK {
            let id = chunk * 1200 + i;
            let x = 0i32;
            let z = chunk as i32;
            let dimension = id as u32;
            let _ = Location::insert(Location {
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

#[spacetimedb(reducer)]
/// Probing a single column index for a single row should be fast!
pub fn test_index_scan_on_id() {
    let span = Span::start("Index scan on {id}");
    let location = Location::filter_by_id(&ID).unwrap();
    span.end();
    assert_eq!(ID, location.id);
}

#[spacetimedb(reducer)]
/// Scanning a single column index for `ROWS_PER_CHUNK` rows should also be fast!
pub fn test_index_scan_on_chunk() {
    let span = Span::start("Index scan on {chunk}");
    let n = Location::filter_by_chunk(&CHUNK).count();
    span.end();
    assert_eq!(n as u64, ROWS_PER_CHUNK);
}

#[spacetimedb(reducer)]
/// Probing a multi-column index for a single row should be fast!
pub fn test_index_scan_on_x_z_dimension() {
    let z = CHUNK as i32;
    let dimension = ID as u32;
    let span = Span::start("Index scan on {x, z, dimension}");
    let n = query!(|r: Location| r.x == 0 && r.z == z && r.dimension == dimension).count();
    span.end();
    assert_eq!(n, 1);
}

#[spacetimedb(reducer)]
/// Probing a multi-column index for `ROWS_PER_CHUNK` rows should also be fast!
pub fn test_index_scan_on_x_z() {
    let z = CHUNK as i32;
    let span = Span::start("Index scan on {x, z}");
    let n = query!(|r: Location| r.x == 0 && r.z == z).count();
    span.end();
    assert_eq!(n as u64, ROWS_PER_CHUNK);
}
