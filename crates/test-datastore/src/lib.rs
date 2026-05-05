//! In-memory datastore support for SpacetimeDB module unit tests.

#[cfg(target_arch = "wasm32")]
compile_error!("spacetimedb-test-datastore is only supported for native module tests");

use std::cell::RefCell;
use std::sync::Arc;

use spacetimedb_core::db::relational_db::{MutTx, RelationalDB, Tx};
use spacetimedb_core::error::{DBError, DatastoreError, IndexError, SequenceError};
use spacetimedb_datastore::locking_tx_datastore::IndexScanPointOrRange;
use spacetimedb_lib::bsatn::EncodeError;
use spacetimedb_lib::bsatn::ToBsatn;
use spacetimedb_lib::{ProductValue, RawModuleDef};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::error::ValidationErrors;
use spacetimedb_schema::schema::{Schema, TableSchema};
use thiserror::Error;

/// A [`RelationalDB`] initialized from a module definition for native unit tests.
#[derive(Debug)]
pub struct TestDatastore {
    db: Arc<RelationalDB>,
    module_def: ModuleDef,
}

/// A single pending mutable transaction for procedure unit tests.
pub struct TestTransaction {
    db: Arc<RelationalDB>,
    tx: RefCell<Option<MutTx>>,
}

impl TestDatastore {
    /// Create an in-memory datastore initialized with the tables in `raw`.
    pub fn from_module_def(raw: RawModuleDef) -> Result<Self, TestDatastoreError> {
        let module_def = ModuleDef::try_from(raw)?;
        let test_db = spacetimedb_core::db::relational_db::tests_utils::TestDB::in_memory()?;
        let db = test_db.db;

        spacetimedb_core::db::relational_db::tests_utils::with_auto_commit(&db, |tx| {
            for table in module_def.tables() {
                let schema = TableSchema::from_module_def(&module_def, table, (), TableId::SENTINEL);
                db.create_table(tx, schema)?;
            }
            Ok::<(), TestDatastoreError>(())
        })?;

        Ok(Self { db, module_def })
    }

    /// The underlying in-memory relational database.
    pub fn relational_db(&self) -> &Arc<RelationalDB> {
        &self.db
    }

    /// The validated module definition used to initialize this datastore.
    pub fn module_def(&self) -> &ModuleDef {
        &self.module_def
    }

    /// Resolve a table name to its datastore id.
    pub fn table_id(&self, table_name: &str) -> Result<TableId, TestDatastoreError> {
        let id = spacetimedb_core::db::relational_db::tests_utils::with_read_only(&self.db, |tx| {
            self.db.table_id_from_name(tx, table_name)
        })?;

        id.ok_or_else(|| TestDatastoreError::MissingTable(table_name.into()))
    }

    /// Resolve an index name to its datastore id.
    pub fn index_id(&self, index_name: &str) -> Result<IndexId, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let id = self.db.index_id_from_name(tx, index_name)?;
            id.ok_or_else(|| TestDatastoreError::MissingIndex(index_name.into()))
        })
    }

    /// Execute a read-only operation against the datastore.
    pub fn with_read_only<T>(&self, f: impl FnOnce(&mut Tx) -> T) -> T {
        spacetimedb_core::db::relational_db::tests_utils::with_read_only(&self.db, f)
    }

    /// Execute a mutable operation and commit it if `f` succeeds.
    pub fn with_auto_commit<T>(
        &self,
        f: impl FnOnce(&mut MutTx) -> Result<T, TestDatastoreError>,
    ) -> Result<T, TestDatastoreError> {
        spacetimedb_core::db::relational_db::tests_utils::with_auto_commit(&self.db, f)
    }

    /// Begin an explicit mutable transaction.
    ///
    /// The transaction must be committed or rolled back. If it is dropped while
    /// still pending, it rolls back.
    pub fn begin_mut_tx(&self) -> TestTransaction {
        TestTransaction {
            db: self.db.clone(),
            tx: RefCell::new(Some(spacetimedb_core::db::relational_db::tests_utils::begin_mut_tx(
                &self.db,
            ))),
        }
    }

    /// Insert a BSATN-encoded row into `table_id`.
    pub fn insert_bsatn(&self, table_id: TableId, row: &[u8]) -> Result<Vec<u8>, TestDatastoreError> {
        self.insert_bsatn_generated_cols(table_id, row)
    }

    /// Insert a BSATN-encoded row and return BSATN-encoded generated columns.
    pub fn insert_bsatn_generated_cols(&self, table_id: TableId, row: &[u8]) -> Result<Vec<u8>, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (generated_cols, row_ref, _) = self.db.insert(tx, table_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }

    /// Return the number of rows in `table_id`.
    pub fn table_row_count(&self, table_id: TableId) -> Result<u64, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            self.db
                .table_row_count_mut(tx, table_id)
                .ok_or(TestDatastoreError::MissingTableId(table_id))
        })
    }

    /// Collect every row in `table_id` as [`ProductValue`]s.
    pub fn table_rows(&self, table_id: TableId) -> Result<Vec<ProductValue>, TestDatastoreError> {
        spacetimedb_core::db::relational_db::tests_utils::with_read_only(&self.db, |tx| {
            let rows = self
                .db
                .iter(tx, table_id)?
                .map(|row_ref| row_ref.to_product_value())
                .collect();
            Ok(rows)
        })
    }

    /// Collect every row in `table_id` as BSATN-encoded row bytes.
    pub fn table_rows_bsatn(&self, table_id: TableId) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        spacetimedb_core::db::relational_db::tests_utils::with_read_only(&self.db, |tx| {
            let rows = self
                .db
                .iter(tx, table_id)?
                .map(|row_ref| row_ref.to_bsatn_vec())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Collect rows matching a point index scan as BSATN-encoded row bytes.
    pub fn index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (_, _, iter) = self.db.index_scan_point(tx, index_id, point)?;
            iter.map(|row_ref| row_ref.to_bsatn_vec())
                .collect::<Result<Vec<_>, _>>()
                .map_err(TestDatastoreError::from)
        })
    }

    /// Collect rows matching a range index scan as BSATN-encoded row bytes.
    pub fn index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (_, iter) = self
                .db
                .index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
            match iter {
                IndexScanPointOrRange::Point(_, iter) => iter
                    .map(|row_ref| row_ref.to_bsatn_vec())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(TestDatastoreError::from),
                IndexScanPointOrRange::Range(iter) => iter
                    .map(|row_ref| row_ref.to_bsatn_vec())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(TestDatastoreError::from),
            }
        })
    }

    /// Delete rows matching a point index scan.
    pub fn delete_by_index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<u32, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (table_id, _, iter) = self.db.index_scan_point(tx, index_id, point)?;
            let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>();
            Ok(self.db.delete(tx, table_id, rows_to_delete))
        })
    }

    /// Delete rows matching a range index scan.
    pub fn delete_by_index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (table_id, iter) = self
                .db
                .index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
            let rows_to_delete = match iter {
                IndexScanPointOrRange::Point(_, iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
                IndexScanPointOrRange::Range(iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
            };
            Ok(self.db.delete(tx, table_id, rows_to_delete))
        })
    }

    /// Update a BSATN-encoded row by matching the existing row through `index_id`.
    pub fn update_bsatn_generated_cols(
        &self,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<Vec<u8>, TestDatastoreError> {
        self.with_auto_commit(|tx| {
            let (generated_cols, row_ref, _) = self.db.update(tx, table_id, index_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }
}

impl TestTransaction {
    fn with_mut_tx<T>(
        &self,
        f: impl FnOnce(&mut MutTx) -> Result<T, TestDatastoreError>,
    ) -> Result<T, TestDatastoreError> {
        let mut tx = self.tx.borrow_mut();
        let tx = tx.as_mut().ok_or(TestDatastoreError::TransactionAlreadyFinished)?;
        f(tx)
    }

    /// Commit this transaction.
    pub fn commit(&self) -> Result<(), TestDatastoreError> {
        let tx = self
            .tx
            .borrow_mut()
            .take()
            .ok_or(TestDatastoreError::TransactionAlreadyFinished)?;
        self.db.commit_tx(tx)?;
        Ok(())
    }

    /// Roll back this transaction.
    pub fn rollback(&self) -> Result<(), TestDatastoreError> {
        let Some(tx) = self.tx.borrow_mut().take() else {
            return Ok(());
        };
        let _ = self.db.rollback_mut_tx(tx);
        Ok(())
    }

    /// Resolve a table name to its datastore id inside this transaction.
    pub fn table_id(&self, table_name: &str) -> Result<TableId, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let id = self.db.table_id_from_name_mut(tx, table_name)?;
            id.ok_or_else(|| TestDatastoreError::MissingTable(table_name.into()))
        })
    }

    /// Resolve an index name to its datastore id inside this transaction.
    pub fn index_id(&self, index_name: &str) -> Result<IndexId, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let id = self.db.index_id_from_name_mut(tx, index_name)?;
            id.ok_or_else(|| TestDatastoreError::MissingIndex(index_name.into()))
        })
    }

    /// Insert a BSATN-encoded row and return BSATN-encoded generated columns.
    pub fn insert_bsatn_generated_cols(&self, table_id: TableId, row: &[u8]) -> Result<Vec<u8>, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (generated_cols, row_ref, _) = self.db.insert(tx, table_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }

    /// Return the number of rows in `table_id`.
    pub fn table_row_count(&self, table_id: TableId) -> Result<u64, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            self.db
                .table_row_count_mut(tx, table_id)
                .ok_or(TestDatastoreError::MissingTableId(table_id))
        })
    }

    /// Collect every row in `table_id` as BSATN-encoded row bytes.
    pub fn table_rows_bsatn(&self, table_id: TableId) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let rows = self
                .db
                .iter_mut(tx, table_id)?
                .map(|row_ref| row_ref.to_bsatn_vec())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Collect rows matching a point index scan as BSATN-encoded row bytes.
    pub fn index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (_, _, iter) = self.db.index_scan_point(tx, index_id, point)?;
            iter.map(|row_ref| row_ref.to_bsatn_vec())
                .collect::<Result<Vec<_>, _>>()
                .map_err(TestDatastoreError::from)
        })
    }

    /// Collect rows matching a range index scan as BSATN-encoded row bytes.
    pub fn index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Vec<Vec<u8>>, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (_, iter) = self
                .db
                .index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
            match iter {
                IndexScanPointOrRange::Point(_, iter) => iter
                    .map(|row_ref| row_ref.to_bsatn_vec())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(TestDatastoreError::from),
                IndexScanPointOrRange::Range(iter) => iter
                    .map(|row_ref| row_ref.to_bsatn_vec())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(TestDatastoreError::from),
            }
        })
    }

    /// Delete rows matching a point index scan.
    pub fn delete_by_index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<u32, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (table_id, _, iter) = self.db.index_scan_point(tx, index_id, point)?;
            let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>();
            Ok(self.db.delete(tx, table_id, rows_to_delete))
        })
    }

    /// Delete rows matching a range index scan.
    pub fn delete_by_index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (table_id, iter) = self
                .db
                .index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
            let rows_to_delete = match iter {
                IndexScanPointOrRange::Point(_, iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
                IndexScanPointOrRange::Range(iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
            };
            Ok(self.db.delete(tx, table_id, rows_to_delete))
        })
    }

    /// Update a BSATN-encoded row by matching the existing row through `index_id`.
    pub fn update_bsatn_generated_cols(
        &self,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<Vec<u8>, TestDatastoreError> {
        self.with_mut_tx(|tx| {
            let (generated_cols, row_ref, _) = self.db.update(tx, table_id, index_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }
}

impl Drop for TestTransaction {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.get_mut().take() {
            let _ = self.db.rollback_mut_tx(tx);
        }
    }
}

/// Errors returned by [`TestDatastore`].
#[derive(Debug, Error)]
pub enum TestDatastoreError {
    #[error("invalid module definition: {0}")]
    ModuleDef(#[from] ValidationErrors),
    #[error("database error: {0}")]
    Database(#[from] DBError),
    #[error("missing table `{0}`")]
    MissingTable(Box<str>),
    #[error("missing index `{0}`")]
    MissingIndex(Box<str>),
    #[error("missing table id `{0:?}`")]
    MissingTableId(TableId),
    #[error("invalid generated column projection: {0}")]
    InvalidProjection(#[from] spacetimedb_lib::sats::product_value::InvalidFieldError),
    #[error("BSATN encode error: {0}")]
    BsatnEncode(#[from] EncodeError),
    #[error("transaction already finished")]
    TransactionAlreadyFinished,
}

impl TestDatastoreError {
    /// Convert a datastore insertion error to the public syscall errno shape.
    pub fn insert_errno_code(&self) -> Option<u16> {
        match self {
            Self::Database(DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_)))) => {
                Some(spacetimedb_primitives::errno::UNIQUE_ALREADY_EXISTS.get())
            }
            Self::Database(DBError::Sequence2(SequenceError::UnableToAllocate(_))) => {
                Some(spacetimedb_primitives::errno::AUTO_INC_OVERFLOW.get())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::db::raw_def::v9::{btree, RawModuleDefV9Builder};
    use spacetimedb_lib::{bsatn, AlgebraicType, ProductType, RawModuleDef};

    use super::*;

    fn raw_module_def() -> RawModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                "person",
                ProductType::from([("id", AlgebraicType::I64), ("value", AlgebraicType::I64)]),
                true,
            )
            .with_unique_constraint(0)
            .with_index_no_accessor_name(btree(0));

        RawModuleDef::V9(builder.finish())
    }

    #[test]
    fn from_module_def_creates_distinct_databases() {
        let first = TestDatastore::from_module_def(raw_module_def()).unwrap();
        let second = TestDatastore::from_module_def(raw_module_def()).unwrap();

        let first_table = first.table_id("person").unwrap();
        let second_table = second.table_id("person").unwrap();

        first
            .insert_bsatn(first_table, &bsatn::to_vec(&(1_i64, 0_i64)).unwrap())
            .unwrap();

        assert_eq!(first.table_rows(first_table).unwrap().len(), 1);
        assert_eq!(second.table_rows(second_table).unwrap().len(), 0);
    }

    #[test]
    fn resolves_tables_and_indexes() {
        let datastore = TestDatastore::from_module_def(raw_module_def()).unwrap();

        assert!(datastore.table_id("person").is_ok());
        assert!(datastore.index_id("person_id_idx_btree").is_ok());
    }

    #[test]
    fn unique_constraints_are_enforced() {
        let datastore = TestDatastore::from_module_def(raw_module_def()).unwrap();
        let table_id = datastore.table_id("person").unwrap();
        let first = bsatn::to_vec(&(1_i64, 0_i64)).unwrap();
        let duplicate = bsatn::to_vec(&(1_i64, 1_i64)).unwrap();

        datastore.insert_bsatn(table_id, &first).unwrap();

        assert!(datastore.insert_bsatn(table_id, &duplicate).is_err());
    }

    #[test]
    fn invalid_module_def_returns_validation_error() {
        let mut builder = RawModuleDefV9Builder::new();
        builder.build_table("broken", spacetimedb_lib::sats::AlgebraicTypeRef(999));

        let err = TestDatastore::from_module_def(RawModuleDef::V9(builder.finish())).unwrap_err();
        assert!(matches!(err, TestDatastoreError::ModuleDef(_)));
    }

    #[test]
    fn auto_inc_sequences_are_materialized() {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                "counter",
                ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0));

        let datastore = TestDatastore::from_module_def(RawModuleDef::V9(builder.finish())).unwrap();
        let table_id = datastore.table_id("counter").unwrap();

        datastore
            .insert_bsatn(table_id, &bsatn::to_vec(&(0_u64, "one")).unwrap())
            .unwrap();
        datastore
            .insert_bsatn(table_id, &bsatn::to_vec(&(0_u64, "two")).unwrap())
            .unwrap();

        assert_eq!(datastore.table_rows(table_id).unwrap().len(), 2);
    }
}
