use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, TableStyle},
    ResultBench,
};
use rusqlite::Connection;
use spacetimedb_lib::{
    sats::{self},
    AlgebraicType, AlgebraicValue, ProductType,
};
use std::{fmt::Write, hint::black_box};
use tempdir::TempDir;

pub struct SQLite {
    db: Connection,
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

    fn create_table<T: BenchTable>(&mut self, table_style: crate::schemas::TableStyle) -> ResultBench<Self::TableId> {
        let mut statement = String::new();
        let name = table_name::<T>(table_style);
        write!(&mut statement, "CREATE TABLE {name} (")?;
        for (i, column) in T::product_type().elements.iter().enumerate() {
            let name = column.name.clone().unwrap();
            let type_ = match column.algebraic_type {
                AlgebraicType::Builtin(sats::BuiltinType::U32) => "INTEGER",
                AlgebraicType::Builtin(sats::BuiltinType::U64) => "INTEGER",
                AlgebraicType::Builtin(sats::BuiltinType::String) => "TEXT",
                _ => unimplemented!(),
            };
            let extra = if table_style == TableStyle::Unique && i == 0 {
                " PRIMARY KEY"
            } else {
                ""
            };
            let comma = if i == 0 { "" } else { ", " };
            write!(&mut statement, "{comma}{name} {type_}{extra}")?;
        }
        write!(&mut statement, ");")?;
        log::info!("SQLITE: `{statement}`");
        self.db.execute_batch(&statement)?;
        // TODO(jgilles): add indexes
        Ok(name)
    }
    #[inline(never)]
    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.execute_batch(&format!("DELETE FROM {table_id};"))?;
        Ok(())
    }
    #[inline(never)]
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

    type PreparedInsert<T> = PreparedStatement;
    #[inline(never)]
    fn prepare_insert<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInsert<T>> {
        let statement = insert_template(table_id, T::product_type());
        let _ = self.db.prepare_cached(&statement);
        Ok(PreparedStatement { statement })
    }

    fn insert<T: BenchTable>(&mut self, prepared: &Self::PreparedInsert<T>, row: T) -> ResultBench<()> {
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&prepared.statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        stmt.execute(row.into_sqlite_params())?;
        commit.execute(())?;
        Ok(())
    }

    type PreparedInsertBulk<T> = PreparedStatement;
    #[inline(never)]
    fn prepare_insert_bulk<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
    ) -> ResultBench<Self::PreparedInsertBulk<T>> {
        let statement = insert_template(table_id, T::product_type());
        let _ = self.db.prepare_cached(&statement);
        Ok(PreparedStatement { statement })
    }

    fn insert_bulk<T: BenchTable>(&mut self, prepared: &Self::PreparedInsertBulk<T>, rows: Vec<T>) -> ResultBench<()> {
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&prepared.statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        for row in rows {
            stmt.execute(row.into_sqlite_params())?;
        }
        commit.execute(())?;

        Ok(())
    }

    type PreparedInterate = PreparedStatement;
    #[inline(never)]
    fn prepare_iterate<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInterate> {
        let statement = format!("SELECT * FROM {table_id}");
        let _ = self.db.prepare_cached(&statement);
        Ok(PreparedStatement { statement })
    }
    #[inline(never)]
    fn iterate(&mut self, prepared: &Self::PreparedInterate) -> ResultBench<()> {
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&prepared.statement)?;
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

    type PreparedFilter = PreparedStatement;
    #[inline(never)]
    fn prepare_filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        column_id: u32,
    ) -> ResultBench<Self::PreparedFilter> {
        let column = T::product_type().elements[column_id as usize].name.clone().unwrap();
        Ok(PreparedStatement {
            statement: format!("SELECT * FROM {table_id} WHERE {column} = ?"),
        })
    }
    #[inline(never)]
    fn filter(&mut self, prepared: &Self::PreparedFilter, value: AlgebraicValue) -> ResultBench<()> {
        let mut begin = self.db.prepare_cached(BEGIN_TRANSACTION)?;
        let mut stmt = self.db.prepare_cached(&prepared.statement)?;
        let mut commit = self.db.prepare_cached(COMMIT_TRANSACTION)?;

        begin.execute(())?;
        match value {
            AlgebraicValue::Builtin(sats::BuiltinValue::String(value)) => {
                for _ in stmt.query_map((value,), |row| {
                    black_box(row);
                    Ok(())
                })? {}
            }
            AlgebraicValue::Builtin(sats::BuiltinValue::U32(value)) => {
                for _ in stmt.query_map((value,), |row| {
                    black_box(row);
                    Ok(())
                })? {}
            }
            AlgebraicValue::Builtin(sats::BuiltinValue::U64(value)) => {
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

/// We don't need to actually store any handles to the DB, rusqlite has an internal fast
/// string cache that will look up the query. This is necessary to use prepared statements
/// in rusqlite transactions.
pub struct PreparedStatement {
    statement: String,
}

const BEGIN_TRANSACTION: &str = "BEGIN DEFERRED";
const COMMIT_TRANSACTION: &str = "COMMIT";

#[inline(never)]
fn insert_template(table_id: &str, product_type: ProductType) -> String {
    let mut columns = String::new();
    let mut params = String::new();

    for (i, elt) in product_type.elements.iter().enumerate() {
        let comma = if i == 0 { "" } else { ", " };

        let name = elt.name().unwrap();
        write!(&mut columns, "{comma}{name}").unwrap();

        let sqlite_arg_id = i + 1;
        write!(&mut params, "{comma}?{sqlite_arg_id}").unwrap();
    }

    format!("INSERT INTO {table_id}({columns}) VALUES ({params})")
}
