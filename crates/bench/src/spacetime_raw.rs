use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, IndexStrategy},
    ResultBench,
};
use spacetimedb::db::datastore::traits::{ColId, IndexDef, TableDef, TableSchema};
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::error::DBError;
use spacetimedb::sql::execute::run;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::sats::{string, AlgebraicValue, SatsString};
use std::hint::black_box;
use tempdir::TempDir;

pub type DbResult = (RelationalDB, TempDir, u32);

pub struct SpacetimeRaw {
    db: RelationalDB,
    _temp_dir: TempDir,
}
impl BenchDatabase for SpacetimeRaw {
    fn name() -> &'static str {
        "stdb_raw"
    }
    type TableId = u32;

    fn build(in_memory: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let temp_dir = TempDir::new("stdb_test")?;
        let db = open_db(temp_dir.path(), in_memory, fsync)?;

        Ok(SpacetimeRaw {
            db,
            _temp_dir: temp_dir,
        })
    }

    fn create_table<T: BenchTable>(&mut self, index_strategy: IndexStrategy) -> ResultBench<Self::TableId> {
        let name = SatsString::from_string(table_name::<T>(index_strategy));
        self.db.with_auto_commit(|tx| {
            let table_def = TableDef::from(T::product_type());
            let table_id = self.db.create_table(tx, table_def)?;
            self.db.rename_table(tx, table_id, name)?;
            match index_strategy {
                IndexStrategy::Unique => {
                    self.db
                        .create_index(tx, IndexDef::new(string("id"), table_id, ColId(0), true))?;
                }
                IndexStrategy::NonUnique => (),
                IndexStrategy::MultiIndex => {
                    for (i, column) in T::product_type().elements.iter().enumerate() {
                        self.db.create_index(
                            tx,
                            IndexDef::new(column.name.clone().unwrap(), table_id, ColId(i as u32), false),
                        )?;
                    }
                }
            }

            Ok(table_id)
        })
    }

    fn get_table<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<TableSchema> {
        let schema = self.db.with_auto_commit(|tx| {
            //TODO: For some reason this not retrieve the table name, wait for the PR that fix the bootstraping issues
            let mut t = self.db.schema_for_table(tx, *table_id)?;
            t.table_name = self.db.table_name_from_id(tx, *table_id)?.unwrap();
            Ok::<TableSchema, DBError>(t)
        })?;
        Ok(schema)
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            self.db.clear_table(tx, *table_id)?;
            Ok(())
        })
    }

    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        self.db
            .with_auto_commit(|tx| Ok(self.db.iter(tx, *table_id)?.map(|_| 1u32).sum()))
    }

    fn empty_transaction(&mut self) -> ResultBench<()> {
        self.db.with_auto_commit(|_tx| Ok(()))
    }

    fn insert<T: BenchTable>(&mut self, table_id: &Self::TableId, row: T) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            self.db.insert(tx, *table_id, row.into_product_value())?;
            Ok(())
        })
    }

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in rows {
                self.db.insert(tx, *table_id, row.into_product_value())?;
            }
            Ok(())
        })
    }

    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in self.db.iter(tx, *table_id)? {
                black_box(row);
            }
            Ok(())
        })
    }

    fn filter<T: BenchTable>(
        &mut self,
        table: &TableSchema,
        column_index: u32,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in self.db.iter_by_col_eq(tx, table.table_id, column_index, value)? {
                black_box(row);
            }
            Ok(())
        })
    }

    fn sql_select(&mut self, table: &TableSchema) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            let sql_query = format!("SELECT * FROM {}", table.table_name);

            run(&self.db, tx, &sql_query, AuthCtx::for_testing())?;

            Ok(())
        })
    }

    fn sql_where<T: BenchTable>(
        &mut self,
        table: &TableSchema,
        column_index: u32,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            let column = &table.columns[column_index as usize].col_name;

            let table_name = &table.table_name;

            let value = match value {
                AlgebraicValue::U32(x) => x.to_string(),
                AlgebraicValue::U64(x) => x.to_string(),
                AlgebraicValue::String(x) => format!("'{}'", x),
                _ => {
                    unreachable!()
                }
            };

            let sql_query = format!("SELECT * FROM {table_name} WHERE {column} = {value}");

            run(&self.db, tx, &sql_query, AuthCtx::for_testing())?;

            Ok(())
        })
    }
}
