/// Load test to see large variations on performance
///
/// NOTE: It should be running with `--release` or `--profile bench` when looking with perf tools...
use criterion::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempdir::TempDir;

use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::error::DBError;
use spacetimedb::execution_context::ExecutionContext;
use spacetimedb::sql::execute::run;
use spacetimedb::subscription::subscription::create_table;
use spacetimedb_lib::error::ResultTest;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{product, AlgebraicType};

fn make_test_db() -> Result<(RelationalDB, TempDir), DBError> {
    let tmp_dir = TempDir::new("stdb_test")?;
    let stdb = open_db(&tmp_dir, true, false)?.with_row_count(Arc::new(|_, _| i64::MAX));
    Ok((stdb, tmp_dir))
}

#[derive(Clone, Copy)]
pub struct GameData {
    location_state: u64,
    footprint_tile_state: u64,
}

impl GameData {
    /// Will generate on `release`:
    /// Chunks * 100
    /// FootprintTileState * 1'000
    /// LocationState * 1'000.000 rows
    pub fn new(base_rows: u64) -> Self {
        Self {
            footprint_tile_state: base_rows,
            location_state: base_rows * 10,
        }
    }
}

#[cfg(not(debug_assertions))]
pub const BASE_ROWS: usize = 1_000;
#[cfg(debug_assertions)]
pub const BASE_ROWS: usize = 10;
const EXPECT_ROWS: usize = BASE_ROWS * 1_000;

pub struct Tables {
    db: RelationalDB,
    _tmp: TempDir,
    location_state: TableId,
    footprint_tile_state: TableId,
}

fn make_tables() -> Result<Tables, DBError> {
    let (db, tmp) = make_test_db()?;
    let mut tx = db.begin_mut_tx();

    // LocationState
    let schema = &[
        ("entity_id", AlgebraicType::U64),
        ("chunk_index", AlgebraicType::U64),
        ("x", AlgebraicType::I32),
        ("z", AlgebraicType::I32),
        ("dimension", AlgebraicType::U32),
    ];

    let indexes = &[
        (0.into(), "entity_id_index"),
        (1.into(), "chunk_index"),
        (2.into(), "x_index"),
    ];
    let location_state = create_table(&db, &mut tx, "LocationState", schema, indexes)?;

    // FootprintTileState
    let schema = &[
        ("entity_id", AlgebraicType::U64),
        ("type", AlgebraicType::I32),
        ("owner_entity_id", AlgebraicType::U64),
    ];

    let indexes = &[(0.into(), "entity_id_index")];
    let footprint_tile_state = create_table(&db, &mut tx, "FootprintTileState", schema, indexes)?;

    db.commit_tx(&ExecutionContext::default(), tx)?;

    Ok(Tables {
        db,
        _tmp: tmp,
        location_state,
        footprint_tile_state,
    })
}

fn fill_tables(tables: &Tables, data: GameData) -> Result<(), DBError> {
    let db = &tables.db;
    let mut tx = db.begin_mut_tx();

    for i in 0..data.location_state {
        for chunk_id in 0u64..100 {
            db.insert(
                &mut tx,
                tables.location_state,
                product![i, chunk_id, (i + 10) as i32, (i + 20) as i32, (i + 30) as u32],
            )?;
        }
    }

    for i in 0..data.footprint_tile_state {
        db.insert(
            &mut tx,
            tables.footprint_tile_state,
            product![i, (i + 10) as i32, i + 30],
        )?;
    }

    db.commit_tx(&ExecutionContext::default(), tx)?;

    Ok(())
}

pub fn query(tables: &Tables, data: GameData) -> Result<usize, DBError> {
    let db = &tables.db;
    let tx = db.begin_tx();
    let mut found = 0;
    for chunk_index in 0..data.footprint_tile_state {
        found += run(
            db,
            &format!(
                "SELECT FootprintTileState.*
FROM FootprintTileState
JOIN LocationState ON entity_id = LocationState.entity_id
WHERE LocationState.chunk_index = {chunk_index}"
            ),
            AuthCtx::for_testing(),
        )?[0]
            .data
            .len();
    }

    db.release_tx(&ExecutionContext::default(), tx);

    Ok(found)
}

pub fn game_insert(base_rows: usize) -> ResultTest<Duration> {
    let tables = make_tables()?;

    let data = GameData::new(base_rows as u64);

    let start = Instant::now();
    black_box(fill_tables(&tables, data)?);

    let result = black_box(run(&tables.db, "SELECT * FROM LocationState", AuthCtx::for_testing())?);

    assert_eq!(result[0].data.len(), EXPECT_ROWS);

    Ok(start.elapsed())
}

pub fn game_query(base_rows: usize) -> ResultTest<Duration> {
    let tables = make_tables()?;

    let data = GameData::new(base_rows as u64);
    fill_tables(&tables, data)?;

    let start = Instant::now();

    let result = query(&tables, data)?;

    #[cfg(debug_assertions)]
    assert_eq!(result, EXPECT_ROWS / 100);

    #[cfg(not(debug_assertions))]
    assert_eq!(result, EXPECT_ROWS / 10);

    Ok(start.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_insert() -> ResultTest<()> {
        game_insert(BASE_ROWS)?;

        Ok(())
    }

    #[test]
    fn test_game_query() -> ResultTest<()> {
        println!("{:?}", game_query(BASE_ROWS)?);
        Ok(())
    }
}
