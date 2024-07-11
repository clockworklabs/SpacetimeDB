use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, IndexStrategy},
    ResultBench,
};
use spacetimedb::db::relational_db::{tests_utils::TestDB, RelationalDB};
use spacetimedb::execution_context::ExecutionContext;
use spacetimedb_lib::db::raw_def::{RawIndexDefV0, RawTableDefV0};
use spacetimedb_lib::sats::AlgebraicValue;
use spacetimedb_primitives::{ColId, TableId};
use std::hint::black_box;
use tempdir::TempDir;

pub type DbResult = (RelationalDB, TempDir, u32);

pub struct SpacetimeRaw {
    pub db: TestDB,
}

impl BenchDatabase for SpacetimeRaw {
    fn name() -> &'static str {
        "stdb_raw"
    }
    type TableId = TableId;

    fn build(in_memory: bool, _fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let db = if in_memory {
            TestDB::in_memory()
        } else {
            TestDB::durable()
        }?;
        Ok(Self { db })
    }

    fn create_table<T: BenchTable>(&mut self, index_strategy: IndexStrategy) -> ResultBench<Self::TableId> {
        let name = table_name::<T>(index_strategy);
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            let table_def = RawTableDefV0::from_product(&name, T::product_type());
            let table_id = self.db.create_table(tx, table_def)?;
            self.db.rename_table(tx, table_id, &name)?;
            match index_strategy {
                IndexStrategy::Unique0 => {
                    self.db
                        .create_index(tx, table_id, RawIndexDefV0::btree("id".into(), ColId(0), true))?;
                }
                IndexStrategy::NoIndex => (),
                IndexStrategy::BTreeEachColumn => {
                    for (i, column) in T::product_type().elements.iter().enumerate() {
                        self.db.create_index(
                            tx,
                            table_id,
                            RawIndexDefV0::btree(column.name.clone().unwrap(), ColId(i as u32), false),
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

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        self.db.with_auto_commit(&ExecutionContext::default(), |tx| {
            for row in rows {
                self.db.insert(tx, *table_id, row.into_product_value())?;
            }
            Ok(())
        })
    }

    fn update_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, row_count: u32) -> ResultBench<()> {
        let ctx = ExecutionContext::default();
        self.db.with_auto_commit(&ctx, |tx| {
            let rows = self
                .db
                .iter_mut(&ctx, tx, *table_id)?
                .take(row_count as usize)
                .map(|row| row.to_product_value())
                .collect::<Vec<_>>();

            assert_eq!(rows.len(), row_count as usize, "not enough rows found for update_bulk!");
            for mut row in rows {
                // It would likely be faster to collect a vector of IDs and delete + insert them all at once,
                // but this implementation is closer to how `update` works in modules.
                // (update_by_{field} -> spacetimedb::query::update_by_field -> (delete_by_col_eq; insert))
                let id = self
                    .db
                    .iter_by_col_eq_mut(&ctx, tx, *table_id, 0, &row.elements[0])?
                    .next()
                    .expect("failed to find row during update!")
                    .pointer();

                assert_eq!(
                    self.db.delete(tx, *table_id, [id]),
                    1,
                    "failed to delete row during update!"
                );

                // relies on column 1 being a u64, which is guaranteed by BenchTable
                if let AlgebraicValue::U64(i) = row.elements[1] {
                    row.elements[1] = AlgebraicValue::U64(i + 1);
                } else {
                    panic!("column 1 is not a u64!");
                }

                self.db.insert(tx, *table_id, row)?;
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
        let ctx = ExecutionContext::default();
        self.db.with_auto_commit(&ctx, |tx| {
            for row in self.db.iter_by_col_eq_mut(&ctx, tx, *table_id, column_index, &value)? {
                black_box(row);
            }
            Ok(())
        })
    }
}
