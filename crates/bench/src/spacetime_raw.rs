use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, IndexStrategy},
    ResultBench,
};
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::execution_context::ExecutionContext;
use spacetimedb_lib::sats::AlgebraicValue;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::db::def::{IndexDef, TableDef};
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
    type TableId = TableId;

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
        let name = table_name::<T>(index_strategy);
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            let table_def = TableDef::from_product(&name, T::product_type());
            let table_id = self.db.create_table(tx, table_def)?;
            self.db.rename_table(tx, table_id, &name)?;
            match index_strategy {
                IndexStrategy::Unique => {
                    self.db
                        .create_index(tx, table_id, IndexDef::btree("id".into(), ColId(0), true))?;
                }
                IndexStrategy::NonUnique => (),
                IndexStrategy::MultiIndex => {
                    for (i, column) in T::product_type().elements.iter().enumerate() {
                        self.db.create_index(
                            tx,
                            table_id,
                            IndexDef::btree(column.name.clone().unwrap(), ColId(i as u32), false),
                        )?;
                    }
                }
            }

            Ok(table_id)
        })
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            self.db.clear_table(tx, *table_id)?;
            Ok(())
        })
    }

    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        let ctx = ExecutionContext::default();
        self.db.with_auto_commit(&ctx, |tx| {
            Ok(self.db.iter_mut(&ctx, tx, *table_id)?.map(|_| 1u32).sum())
        })
    }

    fn empty_transaction(&mut self) -> ResultBench<()> {
        self.db.with_auto_commit(&ExecutionContext::default(), |_tx| Ok(()))
    }

    fn insert<T: BenchTable>(&mut self, table_id: &Self::TableId, row: T) -> ResultBench<()> {
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            self.db.insert(tx, *table_id, row.into_product_value())?;
            Ok(())
        })
    }

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            for row in rows {
                self.db.insert(tx, *table_id, row.into_product_value())?;
            }
            Ok(())
        })
    }

    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        let ctx = ExecutionContext::default();
        self.db.with_auto_commit(&ctx, |tx| {
            for row in self.db.iter_mut(&ctx, tx, *table_id)? {
                black_box(row);
            }
            Ok(())
        })
    }

    fn filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        column_index: u32,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        let col: ColId = column_index.into();
        let ctx = ExecutionContext::default();
        self.db.with_auto_commit(&ctx, |tx| {
            for row in self.db.iter_by_col_eq_mut(&ctx, tx, *table_id, col, value)? {
                black_box(row);
            }
            Ok(())
        })
    }
}
