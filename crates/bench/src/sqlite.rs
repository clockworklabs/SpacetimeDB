use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, IndexStrategy},
    ResultBench,
};
use ahash::AHashMap;
use lazy_static::lazy_static;
use rusqlite::Connection;
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicValue, ProductType};
use std::{
    fmt::Write,
    hint::black_box,
    sync::{Arc, RwLock},
};
use tempdir::TempDir;

/// SQLite benchmark harness.
pub struct SQLite {
    db: Connection,
    /// We keep this alive to prevent the temp dir from being deleted.
    _temp_dir: TempDir,
}

impl BenchDatabase for SQLite {
    fn name() -> &'static str {
        "sqlite"
    }

    fn build(in_memory: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let temp_dir = TempDir::new("sqlite_test")?;
        let db = if in_memory {
            Connection::open_in_memory()?
        } else {
            Connection::open(temp_dir.path().join("test.db"))?
        };
        // For sqlite benchmarks we should set synchronous to either full or off which more
        // closely aligns with wal_fsync=true and wal_fsync=false respectively in stdb.
        db.execute_batch(if fsync {
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = full;"
        } else {
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = off;"
        })?;

        Ok(SQLite {
            db,
            _temp_dir: temp_dir,
        })
    }

    type TableId = String;

    /// We derive the SQLite schema from the AlgebraicType of the table.
    fn create_table<T: BenchTable>(
        &mut self,
        index_strategy: crate::schemas::IndexStrategy,
    ) -> ResultBench<Self::TableId> {
        let mut statement = String::new();
        let table_name = table_name::<T>(index_strategy);
        write!(&mut statement, "CREATE TABLE {table_name} (")?;
        for (i, column) in T::product_type().elements.iter().enumerate() {
            let column_name = column.name.clone().unwrap();
            let type_ = match column.algebraic_type {
                AlgebraicType::U32 | AlgebraicType::U64 => "INTEGER",
                AlgebraicType::String => "TEXT",
                _ => unimplemented!(),
            };
            let extra = if index_strategy == IndexStrategy::Unique && i == 0 {
                " PRIMARY KEY"
            } else {
                ""
            };
            let comma = if i == 0 { "" } else { ", " };
            write!(&mut statement, "{comma}{column_name} {type_}{extra}")?;
        }
        writeln!(&mut statement, ");")?;

        if index_strategy == IndexStrategy::MultiIndex {
            for column in T::product_type().elements.iter() {
                let column_name = column.name.clone().unwrap();

                writeln!(
                    &mut statement,
                    "CREATE INDEX index_{table_name}_{column_name} ON {table_name}({column_name});"
                )?;
            }
        }

        log::info!("SQLITE: `{statement}`");
        self.db.execute_batch(&statement)?;

        Ok(table_name)
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.execute_batch(&format!("DELETE FROM {table_id};"))?;
        Ok(())
    }

    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        let rows = self
            .db
            .query_row(&format!("SELECT COUNT(*) FROM {table_id}"), (), |row| row.get(0))?;
        Ok(rows)
    }

    fn empty_transaction(&mut self) -> ResultBench<()> {
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        commit.execute(())?;
        Ok(())
    }

    fn insert<T: BenchTable>(&mut self, table_id: &Self::TableId, row: T) -> ResultBench<()> {
        let statement = memo_query(BenchName::Insert, table_id, || {
            insert_template(table_id, T::product_type())
        });

        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        stmt.execute(row.into_sqlite_params())?;
        commit.execute(())?;
        Ok(())
    }

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        let statement = memo_query(BenchName::InsertBulk, table_id, || {
            insert_template(table_id, T::product_type())
        });

        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        for row in rows {
            stmt.execute(row.into_sqlite_params())?;
        }
        commit.execute(())?;

        Ok(())
    }

    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        let statement = format!("SELECT * FROM {table_id}");
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;
        begin.execute(())?;
        let iter = stmt.query_map((), |row| {
            black_box(row);
            Ok(())
        })?;
        for _ in iter {}

        commit.execute(())?;

        Ok(())
    }

    fn filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        column_index: u32,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        let statement = memo_query(BenchName::Filter, table_id, || {
            let column = T::product_type()
                .elements
                .swap_remove(column_index as usize)
                .name
                .unwrap();
            format!("SELECT * FROM {table_id} WHERE {column} = ?")
        });

        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        match value {
            AlgebraicValue::String(value) => {
                for _ in stmt.query_map((&*value,), |row| {
                    black_box(row);
                    Ok(())
                })? {}
            }
            AlgebraicValue::U32(value) => {
                for _ in stmt.query_map((value,), |row| {
                    black_box(row);
                    Ok(())
                })? {}
            }
            AlgebraicValue::U64(value) => {
                for _ in stmt.query_map((value,), |row| {
                    black_box(row);
                    Ok(())
                })? {}
            }
            _ => unimplemented!(),
        }

        commit.execute(())?;
        Ok(())
    }
}

/// Note: The rusqlite transaction API just invokes these statements,
/// but it doesn't cache them, which significantly penalizes performance.
/// We use prepare_cache to let sqlite go as fast as possible.
const BEGIN_TRANSACTION: &str = "BEGIN DEFERRED";
const COMMIT_TRANSACTION: &str = "COMMIT";

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum BenchName {
    Insert,
    InsertBulk,
    Filter,
}

#[inline(never)]
/// Reduce latency of query formatting, for queries that are complicated to build.
fn memo_query<F: FnOnce() -> String>(bench_name: BenchName, table_id: &str, generate_query: F) -> Arc<str> {
    // fast path
    let queries = QUERIES.read().unwrap();

    if let Some(bench_queries) = queries.get(&bench_name) {
        if let Some(query) = bench_queries.get(table_id) {
            return query.clone();
        }
    }

    // slow path
    drop(queries);
    let mut queries = QUERIES.write().unwrap();

    let bench_queries = if let Some(bench_queries) = queries.get_mut(&bench_name) {
        bench_queries
    } else {
        queries.insert(bench_name, AHashMap::default());
        queries.get_mut(&bench_name).unwrap()
    };

    if let Some(query) = bench_queries.get(table_id) {
        query.clone()
    } else {
        let query = generate_query();
        bench_queries.insert(table_id.to_string(), (&query[..]).into());
        bench_queries[table_id].clone()
    }
}

lazy_static! {
    // bench_name -> table_id -> query.
    // Double hashmap is necessary because of tuple dereferencing problems.
    static ref QUERIES: RwLock<ahash::AHashMap<BenchName, ahash::AHashMap<String, Arc<str>>>> =
        RwLock::new(ahash::AHashMap::default());
}

#[inline(never)]
fn insert_template(table_id: &str, product_type: ProductType) -> String {
    let mut columns = String::new();
    let mut args = String::new();

    for (i, elt) in product_type.elements.iter().enumerate() {
        let comma = if i == 0 { "" } else { ", " };

        let name = elt.name().unwrap();
        write!(&mut columns, "{comma}{name}").unwrap();

        let sqlite_arg_id = i + 1;
        write!(&mut args, "{comma}?{sqlite_arg_id}").unwrap();
    }

    format!("INSERT INTO {table_id}({columns}) VALUES ({args})")
}
