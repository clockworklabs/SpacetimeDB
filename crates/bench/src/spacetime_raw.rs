use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, TableStyle},
    ResultBench,
};
use spacetimedb::db::datastore::traits::{IndexDef, TableDef};
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb_lib::AlgebraicValue;
use std::hint::black_box;
use tempdir::TempDir;

pub type DbResult = (RelationalDB, TempDir, u32);

pub struct SpacetimeRaw {
    db: RelationalDB,
    _temp_dir: TempDir,
}
impl BenchDatabase for SpacetimeRaw {
    fn name() -> &'static str {
        "spacetime_raw"
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

    fn create_table<T: BenchTable>(&mut self, table_style: TableStyle) -> ResultBench<Self::TableId> {
        let name = table_name::<T>(table_style);
        self.db.with_auto_commit(|tx| {
            let table_def = TableDef::from(T::product_type());
            let table_id = self.db.create_table(tx, table_def)?;
            self.db.rename_table(tx, table_id, &name)?;
            match table_style {
                TableStyle::Unique => {
                    self.db
                        .create_index(tx, IndexDef::new("id".to_string(), table_id, 0, true))?;
                }
                TableStyle::NonUnique => (),
                TableStyle::MultiIndex => {
                    for (i, column) in T::product_type().elements.iter().enumerate() {
                        self.db.create_index(
                            tx,
                            IndexDef::new(column.name.clone().unwrap(), table_id, i as u32, false),
                        )?;
                    }
                }
            }

            Ok(table_id)
        })
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

    type PreparedInsert<T> = PreparedQuery;
    #[inline(never)]
    fn prepare_insert<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInsert<T>> {
        Ok(PreparedQuery { table_id: *table_id })
    }

    fn insert<T: BenchTable>(&mut self, prepared: &Self::PreparedInsert<T>, row: T) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            self.db.insert(tx, prepared.table_id, row.into_product_value())?;
            Ok(())
        })
    }

    type PreparedInsertBulk<T> = PreparedQuery;
    #[inline(never)]
    fn prepare_insert_bulk<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
    ) -> ResultBench<Self::PreparedInsertBulk<T>> {
        Ok(PreparedQuery { table_id: *table_id })
    }

    fn insert_bulk<T: BenchTable>(&mut self, prepared: &Self::PreparedInsertBulk<T>, rows: Vec<T>) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in rows {
                self.db.insert(tx, prepared.table_id, row.into_product_value())?;
            }
            Ok(())
        })
    }

    type PreparedInterate = PreparedQuery;
    #[inline(never)]
    fn prepare_iterate<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInterate> {
        Ok(PreparedQuery { table_id: *table_id })
    }
    #[inline(never)]
    fn iterate(&mut self, prepared: &Self::PreparedInterate) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in self.db.iter(tx, prepared.table_id)? {
                black_box(row);
            }
            Ok(())
        })
    }

    type PreparedFilter = PreparedFind;
    #[inline(never)]
    fn prepare_filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        column_id: u32,
    ) -> ResultBench<Self::PreparedFilter> {
        Ok(PreparedFind {
            table_id: *table_id,
            column_id,
        })
    }
    #[inline(never)]
    fn filter(&mut self, prepared: &Self::PreparedFilter, value: AlgebraicValue) -> ResultBench<()> {
        self.db.with_auto_commit(|tx| {
            for row in self
                .db
                .iter_by_col_eq(tx, prepared.table_id, prepared.column_id, &value)?
            {
                black_box(row);
            }
            Ok(())
        })
    }
}

pub struct PreparedQuery {
    table_id: u32,
}

pub struct PreparedFind {
    table_id: u32,
    column_id: u32,
}
