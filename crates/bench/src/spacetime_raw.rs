use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable, IndexStrategy},
    ResultBench,
};
use spacetimedb::db::relational_db::{tests_utils::TestDB, RelationalDB};
use spacetimedb::execution_context::Workload;
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::{bsatn, AlgebraicValue};
use spacetimedb_schema::{
    def::{BTreeAlgorithm, IndexAlgorithm},
    schema::{IndexSchema, TableSchema},
};
use std::hint::black_box;
use tempdir::TempDir;

pub type DbResult = (RelationalDB, TempDir, u32);

pub struct SpacetimeRaw {
    pub db: TestDB,
}

impl BenchDatabase for SpacetimeRaw {
    fn name() -> String {
        "stdb_raw".to_owned()
    }
    type TableId = TableId;

    fn build(in_memory: bool) -> ResultBench<Self>
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
        self.db.with_auto_commit(Workload::Internal, |tx| {
            let mut table_schema = TableSchema::from_product_type(T::product_type());
            table_schema.table_name = name.clone().into();
            let table_id = self.db.create_table(tx, table_schema)?;
            self.db.rename_table(tx, table_id, &name)?;
            match index_strategy {
                IndexStrategy::Unique0 => {
                    self.db.create_index(
                        tx,
                        IndexSchema {
                            index_id: IndexId::SENTINEL,
                            table_id,
                            index_name: "id".into(),
                            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                                columns: ColId(0).into(),
                            }),
                        },
                        true,
                    )?;
                }
                IndexStrategy::NoIndex => (),
                IndexStrategy::BTreeEachColumn => {
                    for (i, column) in T::product_type().elements.iter().enumerate() {
                        self.db.create_index(
                            tx,
                            IndexSchema {
                                index_id: IndexId::SENTINEL,
                                table_id,
                                index_name: column.name.clone().unwrap(),
                                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                                    columns: ColId(i as _).into(),
                                }),
                            },
                            false,
                        )?;
                    }
                }
            }

            Ok(table_id)
        })
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            self.db.clear_table(tx, *table_id)?;
            Ok(())
        })
    }

    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            Ok(self.db.iter_mut(tx, *table_id)?.map(|_| 1u32).sum())
        })
    }

    fn empty_transaction(&mut self) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |_tx| Ok(()))
    }

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            let mut scratch = Vec::new();
            for row in rows {
                scratch.clear();
                bsatn::to_writer(&mut scratch, &row.into_product_value()).unwrap();
                self.db.insert(tx, *table_id, &scratch)?;
            }
            Ok(())
        })
    }

    fn update_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, row_count: u32) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            let rows = self
                .db
                .iter_mut(tx, *table_id)?
                .take(row_count as usize)
                .map(|row| row.to_product_value())
                .collect::<Vec<_>>();

            assert_eq!(rows.len(), row_count as usize, "not enough rows found for update_bulk!");
            let mut scratch = Vec::new();
            for mut row in rows {
                // It would likely be faster to collect a vector of IDs and delete + insert them all at once,
                // but this implementation is closer to how `update` works in modules.
                // (update_by_{field} -> spacetimedb::query::update_by_field -> (delete_by_col_eq; insert))
                let id = self
                    .db
                    .iter_by_col_eq_mut(tx, *table_id, 0, &row.elements[0])?
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

                scratch.clear();
                bsatn::to_writer(&mut scratch, &row).unwrap();
                self.db.insert(tx, *table_id, &scratch)?;
            }
            Ok(())
        })
    }

    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            for row in self.db.iter_mut(tx, *table_id)? {
                black_box(row);
            }
            Ok(())
        })
    }

    fn filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        col_id: impl Into<ColId>,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        self.db.with_auto_commit(Workload::Internal, |tx| {
            for row in self.db.iter_by_col_eq_mut(tx, *table_id, col_id, &value)? {
                black_box(row);
            }
            Ok(())
        })
    }
}
