/// Load test to see large variations on performance
///
/// NOTE: It should be running with `--release` or `--profile bench` when looking with perf tools...
use criterion::black_box;
use itertools::Itertools;
use std::time::{Duration, Instant};
use tempdir::TempDir;

use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::error::DBError;
use spacetimedb::execution_context::ExecutionContext;
use spacetimedb::sql::execute::run;
use spacetimedb::subscription::subscription::create_table;
use spacetimedb_lib::error::ResultTest;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::operator::OpCmp;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::relation::FieldName;
use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue};
use spacetimedb_vm::dsl::mem_table;
use spacetimedb_vm::eval::run_ast;
use spacetimedb_vm::expr::{Code, ColumnOp, QueryExpr};
use spacetimedb_vm::program::Program;

fn make_test_db() -> Result<(RelationalDB, TempDir), DBError> {
    let tmp_dir = TempDir::new("stdb_test")?;
    let stdb = open_db(&tmp_dir, true, false)?;
    Ok((stdb, tmp_dir))
}

#[cfg(not(debug_assertions))]
pub const EXPECT_ROWS: usize = 1_000_000;
// To avoid very slow execution on `debug`
#[cfg(debug_assertions)]
pub const EXPECT_ROWS: usize = 10_000;
const EXPECT_QUERY_ROWS: usize = EXPECT_ROWS / 100;

#[derive(Clone, Copy)]
pub struct GameData {
    num_rows: u64,
}

impl GameData {
    /// Will generate on `release`:
    /// FootprintTileState | LocationState
    ///    EXPECT_ROWS (1'000.000 rows)
    /// Chunks * 12_000 | 1_200
    pub fn new(num_rows: u64) -> Self {
        Self { num_rows }
    }
}

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

/// Generate `chunk_ids` cycling 1_200 * 10 -> 12_000 * 1
fn chunks_cycle() -> impl Iterator<Item = usize> {
    vec![1_200; 10]
        .into_iter()
        .chain(std::iter::once(12_000))
        .cycle()
        .enumerate()
        .flat_map(|(id, chunk_size)| (0..chunk_size).map(move |_| id))
}
fn fill_tables(tables: &Tables, data: GameData) -> Result<(), DBError> {
    let db = &tables.db;
    let mut tx = db.begin_mut_tx();

    // Generate locations
    for (i, chunk_id) in chunks_cycle().take(data.num_rows as usize).enumerate() {
        db.insert(
            &mut tx,
            tables.location_state,
            product![
                i as u64,
                chunk_id as u64,
                (i + 10) as i32,
                (i + 20) as i32,
                (i + 30) as u32
            ],
        )?;
    }

    for (i, owner_entity_id) in chunks_cycle().take(data.num_rows as usize).enumerate() {
        db.insert(
            &mut tx,
            tables.footprint_tile_state,
            product![i as u64, (i + 10) as i32, owner_entity_id as u64],
        )?;
    }

    db.commit_tx(&ExecutionContext::default(), tx)?;

    Ok(())
}

pub fn query(tables: &Tables) -> Result<usize, DBError> {
    let db = &tables.db;
    let tx = db.begin_tx();
    let mut found = 0;
    for chunk_index in chunks_cycle()
        .group_by(|x| *x)
        .into_iter()
        .map(|(chunk_id, _)| chunk_id)
        .take(EXPECT_QUERY_ROWS)
    {
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

pub fn game_insert_select(base_rows: usize) -> ResultTest<Duration> {
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
    black_box(fill_tables(&tables, data)?);

    let start = Instant::now();

    let result = black_box(query(&tables)?);

    assert_eq!(result, EXPECT_ROWS);

    Ok(start.elapsed())
}

const TOTAL_HEADERS: usize = 500;
/// Generate `TOTAL` selections to check the overhead of clone headers
pub fn query_header() -> Result<Duration, DBError> {
    let columns: Vec<_> = (0..TOTAL_HEADERS)
        .map(|i| ProductTypeElement::new(AlgebraicType::U64, Some(i.to_string())))
        .collect();

    let p = &mut Program::new(AuthCtx::for_testing());

    let schema = ProductType::new(columns);

    let row = ProductValue::new(
        &std::iter::repeat(AlgebraicValue::U64(0))
            .take(TOTAL_HEADERS)
            .collect::<Vec<_>>(),
    );

    let input = mem_table(schema, vec![row]);
    let table_name = input.head.table_name.clone();
    let inv = input.clone();

    let mut q = QueryExpr::new(input.clone());
    for i in 0..TOTAL_HEADERS {
        q = q.with_select(ColumnOp::cmp(
            FieldName::Pos {
                table: table_name.clone(),
                field: i,
            },
            OpCmp::Eq,
            AlgebraicValue::U64(0),
        ));
    }

    let start = Instant::now();

    let result = black_box(run_ast(p, q.into()));

    assert_eq!(result, Code::Table(inv.clone()), "Query And");

    Ok(start.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_insert() -> ResultTest<()> {
        game_insert_select(EXPECT_ROWS)?;

        Ok(())
    }

    #[test]
    fn test_game_query() -> ResultTest<()> {
        println!("{:?}", game_query(EXPECT_ROWS)?);
        Ok(())
    }

    #[test]
    fn test_query_header() -> ResultTest<()> {
        println!("{:?}", query_header()?);
        Ok(())
    }
}
