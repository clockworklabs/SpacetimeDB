//! Wasm-compatible in-memory datastore support for SpacetimeDB module unit tests.

use std::collections::HashMap;

use spacetimedb_datastore::error::{DatastoreError, IndexError, SequenceError};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::datastore::Locking;
use spacetimedb_datastore::locking_tx_datastore::IndexScanPointOrRange;
use spacetimedb_datastore::traits::{IsolationLevel, MutTx, MutTxDatastore, Tx, TxDatastore};
use spacetimedb_lib::bsatn::{DecodeError, EncodeError, ToBsatn};
use spacetimedb_lib::{ConnectionId, Identity, ProductType, ProductValue, RawModuleDef};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::error::ValidationErrors;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::page_pool::PagePool;
use thiserror::Error;

/// Commit-time behavior for a pending transaction.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CommitMode {
    /// Commit all rows normally.
    Normal,
    /// Drop event-table rows before committing to shared state.
    DropEventTableRows,
}

/// Auth state validated for a test caller.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidatedAuth {
    pub sender: Identity,
    pub connection_id: Option<ConnectionId>,
}

/// A [`Locking`] datastore initialized from a module definition for portable unit tests.
pub struct PortableDatastore {
    datastore: Locking,
    module_def: ModuleDef,
    raw_module_def: RawModuleDef,
    module_identity: Identity,
    event_table_ids: Vec<TableId>,
    table_ids: HashMap<Box<str>, TableId>,
    index_ids: HashMap<Box<str>, IndexId>,
}

/// A single pending mutable transaction.
pub struct PortableTransaction {
    datastore: Locking,
    tx: Option<<Locking as MutTx>::MutTx>,
}

impl PortableDatastore {
    /// Create an in-memory datastore initialized with the tables in `raw`.
    pub fn from_module_def(raw: RawModuleDef, module_identity: Identity) -> Result<Self, PortableDatastoreError> {
        let module_def = ModuleDef::try_from(raw.clone())?;
        let datastore = Locking::bootstrap(module_identity, PagePool::new(None))?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let mut event_table_ids = Vec::new();
        let mut table_ids = HashMap::new();
        let mut index_ids = HashMap::new();

        let result = (|| {
            for table in module_def.tables() {
                let schema = TableSchema::from_module_def(&module_def, table, (), TableId::SENTINEL);
                let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
                table_ids.insert(table.name.to_string().into_boxed_str(), table_id);
                table_ids.insert(table.accessor_name.to_string().into_boxed_str(), table_id);

                for index in table.indexes.values() {
                    let Some(index_id) = datastore.index_id_from_name_mut_tx(&tx, index.name.as_ref())? else {
                        return Err(PortableDatastoreError::MissingIndex(index.name.as_ref().into()));
                    };
                    index_ids.insert(index.name.to_string().into_boxed_str(), index_id);
                    index_ids.insert(index.source_name.to_string().into_boxed_str(), index_id);
                    if let Some(accessor_name) = &index.accessor_name {
                        index_ids.insert(accessor_name.to_string().into_boxed_str(), index_id);
                    }
                }

                if table.is_event {
                    event_table_ids.push(table_id);
                }
            }
            Ok::<_, PortableDatastoreError>(())
        })();

        if let Err(err) = result {
            let _ = datastore.rollback_mut_tx(tx);
            return Err(err);
        }

        datastore.commit_mut_tx(tx)?;

        Ok(Self {
            datastore,
            module_def,
            raw_module_def: raw,
            module_identity,
            event_table_ids,
            table_ids,
            index_ids,
        })
    }

    /// The validated module definition used to initialize this datastore.
    pub fn module_def(&self) -> &ModuleDef {
        &self.module_def
    }

    /// Resolve a table name to its datastore id.
    pub fn table_id(&self, table_name: &str) -> Result<TableId, PortableDatastoreError> {
        self.table_ids
            .get(table_name)
            .copied()
            .ok_or_else(|| PortableDatastoreError::MissingTable(table_name.into()))
    }

    /// Resolve an index name to its datastore id.
    pub fn index_id(&self, index_name: &str) -> Result<IndexId, PortableDatastoreError> {
        self.index_ids
            .get(index_name)
            .copied()
            .ok_or_else(|| PortableDatastoreError::MissingIndex(index_name.into()))
    }

    /// Begin an explicit mutable transaction.
    pub fn begin_mut_tx(&self) -> PortableTransaction {
        PortableTransaction {
            datastore: self.datastore.clone(),
            tx: Some(
                self.datastore
                    .begin_mut_tx(IsolationLevel::Serializable, Workload::Internal),
            ),
        }
    }

    /// Commit this transaction.
    pub fn commit_tx(&self, mut tx: PortableTransaction, mode: CommitMode) -> Result<(), PortableDatastoreError> {
        let mut inner = tx.take()?;
        if mode == CommitMode::DropEventTableRows {
            for table_id in &self.event_table_ids {
                inner.clear_table(*table_id)?;
            }
        }
        self.datastore.commit_mut_tx(inner)?;
        Ok(())
    }

    /// Roll back this transaction.
    pub fn rollback_tx(&self, mut tx: PortableTransaction) -> Result<(), PortableDatastoreError> {
        if let Some(inner) = tx.tx.take() {
            let _ = self.datastore.rollback_mut_tx(inner);
        }
        Ok(())
    }

    /// Reset the datastore to the post-bootstrap empty module state.
    pub fn reset(&mut self) -> Result<(), PortableDatastoreError> {
        *self = Self::from_module_def(self.raw_module_def.clone(), self.module_identity)?;
        Ok(())
    }

    /// Return the number of rows in `table_id`.
    pub fn table_row_count(
        &self,
        tx: Option<&PortableTransaction>,
        table_id: TableId,
    ) -> Result<u64, PortableDatastoreError> {
        match tx {
            Some(tx) => self.with_tx(tx, |tx| {
                self.datastore
                    .iter_mut_tx(tx, table_id)
                    .map(|rows| rows.count() as u64)
                    .map_err(Into::into)
            }),
            None => {
                let tx = self.datastore.begin_tx(Workload::Internal);
                let result = self.datastore.iter_tx(&tx, table_id).map(|rows| rows.count() as u64);
                let _ = self.datastore.release_tx(tx);
                result.map_err(Into::into)
            }
        }
    }

    /// Collect every row in `table_id` as BSATN-encoded row bytes.
    pub fn table_rows_bsatn(
        &self,
        tx: Option<&PortableTransaction>,
        table_id: TableId,
    ) -> Result<Vec<Vec<u8>>, PortableDatastoreError> {
        match tx {
            Some(tx) => self.with_tx(tx, |tx| {
                self.datastore
                    .iter_mut_tx(tx, table_id)?
                    .map(|row_ref| row_ref.to_bsatn_vec())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(Into::into)
            }),
            None => {
                let tx = self.datastore.begin_tx(Workload::Internal);
                let result = (|| {
                    let rows = self.datastore.iter_tx(&tx, table_id)?;
                    rows.map(|row_ref| row_ref.to_bsatn_vec())
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(PortableDatastoreError::from)
                })();
                let _ = self.datastore.release_tx(tx);
                result
            }
        }
    }

    /// Collect rows matching a point index scan as BSATN-encoded row bytes.
    pub fn index_scan_point_bsatn(
        &self,
        tx: Option<&PortableTransaction>,
        index_id: IndexId,
        point: &[u8],
    ) -> Result<Vec<Vec<u8>>, PortableDatastoreError> {
        match tx {
            Some(tx) => self.with_tx(tx, |tx| collect_point_scan(tx, index_id, point)),
            None => self.with_rollback_tx(|tx| collect_point_scan(tx, index_id, point)),
        }
    }

    /// Collect rows matching a range index scan as BSATN-encoded row bytes.
    pub fn index_scan_range_bsatn(
        &self,
        tx: Option<&PortableTransaction>,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Vec<Vec<u8>>, PortableDatastoreError> {
        match tx {
            Some(tx) => self.with_tx(tx, |tx| {
                collect_range_scan(tx, index_id, prefix, prefix_elems, rstart, rend)
            }),
            None => self.with_rollback_tx(|tx| collect_range_scan(tx, index_id, prefix, prefix_elems, rstart, rend)),
        }
    }

    /// Insert a BSATN-encoded row and return BSATN-encoded generated columns.
    pub fn insert_bsatn_generated_cols(
        &self,
        tx: &mut PortableTransaction,
        table_id: TableId,
        row: &[u8],
    ) -> Result<Vec<u8>, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| {
            let (generated_cols, row_ref, _) = self.datastore.insert_mut_tx(tx, table_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }

    /// Update a BSATN-encoded row by matching the existing row through `index_id`.
    pub fn update_bsatn_generated_cols(
        &self,
        tx: &mut PortableTransaction,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<Vec<u8>, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| {
            let (generated_cols, row_ref, _) = self.datastore.update_mut_tx(tx, table_id, index_id, row)?;
            Ok(row_ref.project_product(&generated_cols)?.to_bsatn_vec()?)
        })
    }

    /// Delete rows matching a BSATN-encoded relation.
    pub fn delete_by_rel_bsatn(
        &self,
        tx: &mut PortableTransaction,
        table_id: TableId,
        relation: &[u8],
    ) -> Result<u32, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| {
            let row_ty = self.datastore.row_type_for_table_mut_tx(tx, table_id)?;
            let rows = decode_relation(&row_ty, relation)?;
            Ok(self.datastore.delete_by_rel_mut_tx(tx, table_id, rows))
        })
    }

    /// Delete rows matching a point index scan.
    pub fn delete_by_index_scan_point_bsatn(
        &self,
        tx: &mut PortableTransaction,
        index_id: IndexId,
        point: &[u8],
    ) -> Result<u32, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| {
            let (table_id, _, iter) = tx.index_scan_point(index_id, point)?;
            let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>();
            Ok(self.datastore.delete_mut_tx(tx, table_id, rows_to_delete))
        })
    }

    /// Delete rows matching a range index scan.
    pub fn delete_by_index_scan_range_bsatn(
        &self,
        tx: &mut PortableTransaction,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| {
            let (table_id, iter) = tx.index_scan_range(index_id, prefix, prefix_elems, rstart, rend)?;
            let rows_to_delete = match iter {
                IndexScanPointOrRange::Point(_, iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
                IndexScanPointOrRange::Range(iter) => iter.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>(),
            };
            Ok(self.datastore.delete_mut_tx(tx, table_id, rows_to_delete))
        })
    }

    /// Clear all rows from a table.
    pub fn clear_table(&self, tx: &mut PortableTransaction, table_id: TableId) -> Result<u64, PortableDatastoreError> {
        self.with_tx_mut(tx, |tx| Ok(tx.clear_table(table_id)?))
    }

    /// Validate JWT payload claims and derive sender identity.
    pub fn validate_jwt_payload(
        &self,
        payload: &str,
        connection_id: ConnectionId,
    ) -> Result<ValidatedAuth, PortableDatastoreError> {
        let claims: serde_json::Value = serde_json::from_str(payload)?;
        let sender = validate_test_jwt_claims(&claims)?;
        Ok(ValidatedAuth {
            sender,
            connection_id: Some(connection_id),
        })
    }

    fn with_rollback_tx<T>(
        &self,
        f: impl FnOnce(&<Locking as MutTx>::MutTx) -> Result<T, PortableDatastoreError>,
    ) -> Result<T, PortableDatastoreError> {
        let tx = self
            .datastore
            .begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let result = f(&tx);
        let _ = self.datastore.rollback_mut_tx(tx);
        result
    }

    fn with_tx<T>(
        &self,
        tx: &PortableTransaction,
        f: impl FnOnce(&<Locking as MutTx>::MutTx) -> Result<T, PortableDatastoreError>,
    ) -> Result<T, PortableDatastoreError> {
        let tx = tx
            .tx
            .as_ref()
            .ok_or(PortableDatastoreError::TransactionAlreadyFinished)?;
        f(tx)
    }

    fn with_tx_mut<T>(
        &self,
        tx: &mut PortableTransaction,
        f: impl FnOnce(&mut <Locking as MutTx>::MutTx) -> Result<T, PortableDatastoreError>,
    ) -> Result<T, PortableDatastoreError> {
        let tx = tx
            .tx
            .as_mut()
            .ok_or(PortableDatastoreError::TransactionAlreadyFinished)?;
        f(tx)
    }
}

impl PortableTransaction {
    fn take(&mut self) -> Result<<Locking as MutTx>::MutTx, PortableDatastoreError> {
        self.tx.take().ok_or(PortableDatastoreError::TransactionAlreadyFinished)
    }
}

impl Drop for PortableTransaction {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = self.datastore.rollback_mut_tx(tx);
        }
    }
}

/// Errors returned by [`PortableDatastore`].
#[derive(Debug, Error)]
pub enum PortableDatastoreError {
    #[error("invalid module definition: {0}")]
    ModuleDef(#[from] ValidationErrors),
    #[error("datastore error: {0}")]
    Datastore(#[from] DatastoreError),
    #[error("missing table `{0}`")]
    MissingTable(Box<str>),
    #[error("missing index `{0}`")]
    MissingIndex(Box<str>),
    #[error("invalid generated column projection: {0}")]
    InvalidProjection(#[from] spacetimedb_lib::sats::product_value::InvalidFieldError),
    #[error("BSATN encode error: {0}")]
    BsatnEncode(#[from] EncodeError),
    #[error("BSATN decode error: {0}")]
    BsatnDecode(#[from] DecodeError),
    #[error("invalid JWT payload: {0}")]
    InvalidJwtPayload(#[from] serde_json::Error),
    #[error("invalid JWT claims: {0}")]
    InvalidJwtClaims(#[from] anyhow::Error),
    #[error("invalid relation buffer: {0}")]
    InvalidRelation(anyhow::Error),
    #[error("transaction already finished")]
    TransactionAlreadyFinished,
}

impl PortableDatastoreError {
    /// Convert a datastore insertion error to the public syscall errno shape.
    pub fn insert_errno_code(&self) -> Option<u16> {
        match self {
            Self::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_))) => {
                Some(spacetimedb_primitives::errno::UNIQUE_ALREADY_EXISTS.get())
            }
            Self::Datastore(DatastoreError::Sequence(SequenceError::UnableToAllocate(_))) => {
                Some(spacetimedb_primitives::errno::AUTO_INC_OVERFLOW.get())
            }
            _ => None,
        }
    }
}

fn collect_point_scan(
    tx: &<Locking as MutTx>::MutTx,
    index_id: IndexId,
    point: &[u8],
) -> Result<Vec<Vec<u8>>, PortableDatastoreError> {
    let (_, _, iter) = tx.index_scan_point(index_id, point)?;
    iter.map(|row_ref| row_ref.to_bsatn_vec())
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn collect_range_scan(
    tx: &<Locking as MutTx>::MutTx,
    index_id: IndexId,
    prefix: &[u8],
    prefix_elems: ColId,
    rstart: &[u8],
    rend: &[u8],
) -> Result<Vec<Vec<u8>>, PortableDatastoreError> {
    let (_, iter) = tx.index_scan_range(index_id, prefix, prefix_elems, rstart, rend)?;
    match iter {
        IndexScanPointOrRange::Point(_, iter) => iter
            .map(|row_ref| row_ref.to_bsatn_vec())
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into),
        IndexScanPointOrRange::Range(iter) => iter
            .map(|row_ref| row_ref.to_bsatn_vec())
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into),
    }
}

fn decode_relation(row_ty: &ProductType, relation: &[u8]) -> Result<Vec<ProductValue>, PortableDatastoreError> {
    let Some((row_count, mut relation)) = relation.split_first_chunk::<4>() else {
        return Err(PortableDatastoreError::InvalidRelation(anyhow::anyhow!(
            "relation buffer missing row count prefix"
        )));
    };
    let row_count = u32::from_le_bytes(*row_count);
    (0..row_count)
        .map(|_| spacetimedb_lib::bsatn::decode(row_ty, &mut relation).map_err(PortableDatastoreError::from))
        .collect()
}

fn validate_test_jwt_claims(claims: &serde_json::Value) -> anyhow::Result<Identity> {
    let issuer = required_claim(claims, "iss")?;
    let subject = required_claim(claims, "sub")?;

    if issuer.len() > 128 {
        anyhow::bail!("Issuer too long: {issuer:?}");
    }
    if subject.len() > 128 {
        anyhow::bail!("Subject too long: {subject:?}");
    }
    if issuer.is_empty() {
        anyhow::bail!("Issuer empty");
    }
    if subject.is_empty() {
        anyhow::bail!("Subject empty");
    }

    let computed_identity = Identity::from_claims(issuer, subject);
    if let Some(token_identity) = claims.get("hex_identity") {
        let token_identity = token_identity
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Claim `hex_identity` must be a string"))
            .and_then(|hex| {
                Identity::from_hex(hex).map_err(|err| anyhow::anyhow!("invalid hex_identity claim: {err}"))
            })?;
        if token_identity != computed_identity {
            anyhow::bail!(
                "Identity mismatch: token identity {token_identity:?} does not match computed identity {computed_identity:?}",
            );
        }
    }

    Ok(computed_identity)
}

fn required_claim<'a>(claims: &'a serde_json::Value, name: &str) -> anyhow::Result<&'a str> {
    claims
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Missing `{name}` claim"))?
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Claim `{name}` must be a string"))
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::bsatn;
    use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
    use spacetimedb_lib::db::raw_def::v9::{btree, RawModuleDefV9Builder};
    use spacetimedb_lib::{AlgebraicType, ProductType};

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
        builder
            .build_table_with_new_type(
                "pet",
                ProductType::from([
                    ("id", AlgebraicType::I64),
                    ("owner_id", AlgebraicType::I64),
                    ("name", AlgebraicType::String),
                ]),
                true,
            )
            .with_unique_constraint(0)
            .with_index_no_accessor_name(btree(0));

        RawModuleDef::V9(builder.finish())
    }

    fn event_module_def() -> RawModuleDef {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "person",
                ProductType::from([("id", AlgebraicType::I64), ("value", AlgebraicType::I64)]),
                true,
            )
            .finish();
        builder
            .build_table_with_new_type(
                "event",
                ProductType::from([("id", AlgebraicType::I64), ("value", AlgebraicType::I64)]),
                true,
            )
            .with_event(true)
            .finish();

        RawModuleDef::V10(builder.finish())
    }

    fn counter_module_def() -> RawModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                "counter",
                ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0));

        RawModuleDef::V9(builder.finish())
    }

    fn datastore(raw: RawModuleDef) -> PortableDatastore {
        PortableDatastore::from_module_def(raw, Identity::ZERO).unwrap()
    }

    fn insert_person(ds: &PortableDatastore, id: i64, value: i64) {
        let table_id = ds.table_id("person").unwrap();
        let mut tx = ds.begin_mut_tx();
        ds.insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(id, value)).unwrap())
            .unwrap();
        ds.commit_tx(tx, CommitMode::Normal).unwrap();
    }

    fn relation_row(row: &(i64, i64)) -> Vec<u8> {
        let mut bytes = bsatn::to_vec(&1_u32).unwrap();
        bytes.extend(bsatn::to_vec(row).unwrap());
        bytes
    }

    #[test]
    fn from_module_def_creates_distinct_databases() {
        let first = datastore(raw_module_def());
        let second = datastore(raw_module_def());

        insert_person(&first, 1, 10);

        let first_table = first.table_id("person").unwrap();
        let second_table = second.table_id("person").unwrap();
        assert_eq!(first.table_row_count(None, first_table).unwrap(), 1);
        assert_eq!(second.table_row_count(None, second_table).unwrap(), 0);
    }

    #[test]
    fn resolves_tables_and_indexes() {
        let ds = datastore(raw_module_def());

        assert!(ds.table_id("person").is_ok());
        assert!(ds.index_id("person_id_idx_btree").is_ok());
    }

    #[test]
    fn insert_scan_and_index_point_scan_work() {
        let ds = datastore(raw_module_def());
        let table_id = ds.table_id("person").unwrap();
        let index_id = ds.index_id("person_id_idx_btree").unwrap();

        insert_person(&ds, 1, 10);
        insert_person(&ds, 2, 20);

        assert_eq!(ds.table_row_count(None, table_id).unwrap(), 2);
        assert_eq!(ds.table_rows_bsatn(None, table_id).unwrap().len(), 2);
        assert_eq!(
            ds.index_scan_point_bsatn(None, index_id, &bsatn::to_vec(&1_i64).unwrap())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn delete_update_and_clear_work() {
        let ds = datastore(raw_module_def());
        let table_id = ds.table_id("person").unwrap();
        let index_id = ds.index_id("person_id_idx_btree").unwrap();
        insert_person(&ds, 1, 10);
        insert_person(&ds, 2, 20);

        let mut tx = ds.begin_mut_tx();
        assert_eq!(
            ds.delete_by_rel_bsatn(&mut tx, table_id, &relation_row(&(1, 10)))
                .unwrap(),
            1
        );
        ds.update_bsatn_generated_cols(&mut tx, table_id, index_id, &bsatn::to_vec(&(2_i64, 22_i64)).unwrap())
            .unwrap();
        ds.commit_tx(tx, CommitMode::Normal).unwrap();

        assert_eq!(ds.table_row_count(None, table_id).unwrap(), 1);

        let mut tx = ds.begin_mut_tx();
        assert_eq!(ds.clear_table(&mut tx, table_id).unwrap(), 1);
        ds.commit_tx(tx, CommitMode::Normal).unwrap();
        assert_eq!(ds.table_row_count(None, table_id).unwrap(), 0);
    }

    #[test]
    fn unique_constraints_are_enforced() {
        let ds = datastore(raw_module_def());
        let table_id = ds.table_id("person").unwrap();
        let mut tx = ds.begin_mut_tx();

        ds.insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(1_i64, 10_i64)).unwrap())
            .unwrap();
        let err = ds
            .insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(1_i64, 20_i64)).unwrap())
            .unwrap_err();

        assert_eq!(
            err.insert_errno_code(),
            Some(spacetimedb_primitives::errno::UNIQUE_ALREADY_EXISTS.get())
        );
    }

    #[test]
    fn auto_inc_sequences_are_materialized() {
        let ds = datastore(counter_module_def());
        let table_id = ds.table_id("counter").unwrap();
        let mut tx = ds.begin_mut_tx();

        let first = ds
            .insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(0_u64, "one")).unwrap())
            .unwrap();
        let second = ds
            .insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(0_u64, "two")).unwrap())
            .unwrap();

        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert_ne!(first, second);
    }

    #[test]
    fn rollback_discards_rows_and_sequence_state() {
        let ds = datastore(counter_module_def());
        let table_id = ds.table_id("counter").unwrap();

        let mut rolled_back = ds.begin_mut_tx();
        let first = ds
            .insert_bsatn_generated_cols(&mut rolled_back, table_id, &bsatn::to_vec(&(0_u64, "one")).unwrap())
            .unwrap();
        ds.rollback_tx(rolled_back).unwrap();

        let mut committed = ds.begin_mut_tx();
        let second = ds
            .insert_bsatn_generated_cols(&mut committed, table_id, &bsatn::to_vec(&(0_u64, "one")).unwrap())
            .unwrap();
        ds.commit_tx(committed, CommitMode::Normal).unwrap();

        assert_eq!(first, second);
        assert_eq!(ds.table_row_count(None, table_id).unwrap(), 1);
    }

    #[test]
    fn transactional_reads_see_pending_writes() {
        let ds = datastore(raw_module_def());
        let table_id = ds.table_id("person").unwrap();
        let mut tx = ds.begin_mut_tx();

        ds.insert_bsatn_generated_cols(&mut tx, table_id, &bsatn::to_vec(&(1_i64, 10_i64)).unwrap())
            .unwrap();

        assert_eq!(ds.table_row_count(Some(&tx), table_id).unwrap(), 1);
        ds.commit_tx(tx, CommitMode::Normal).unwrap();
        assert_eq!(ds.table_row_count(None, table_id).unwrap(), 1);
    }

    #[test]
    fn event_table_rows_are_dropped_on_reducer_commit() {
        let ds = datastore(event_module_def());
        let person = ds.table_id("person").unwrap();
        let event = ds.table_id("event").unwrap();
        let mut tx = ds.begin_mut_tx();

        ds.insert_bsatn_generated_cols(&mut tx, person, &bsatn::to_vec(&(1_i64, 10_i64)).unwrap())
            .unwrap();
        ds.insert_bsatn_generated_cols(&mut tx, event, &bsatn::to_vec(&(1_i64, 99_i64)).unwrap())
            .unwrap();
        assert_eq!(ds.table_row_count(Some(&tx), event).unwrap(), 1);
        ds.commit_tx(tx, CommitMode::DropEventTableRows).unwrap();

        assert_eq!(ds.table_row_count(None, person).unwrap(), 1);
        assert_eq!(ds.table_row_count(None, event).unwrap(), 0);
    }

    #[test]
    fn invalid_module_def_returns_validation_error() {
        let mut builder = RawModuleDefV9Builder::new();
        builder.build_table("broken", spacetimedb_lib::sats::AlgebraicTypeRef(999));

        let err = match PortableDatastore::from_module_def(RawModuleDef::V9(builder.finish()), Identity::ZERO) {
            Ok(_) => panic!("invalid module definition unexpectedly succeeded"),
            Err(err) => err,
        };
        assert!(matches!(err, PortableDatastoreError::ModuleDef(_)));
    }

    #[test]
    fn jwt_payload_validation() {
        let ds = datastore(raw_module_def());
        let connection_id = ConnectionId::ZERO;
        let expected = Identity::from_claims("issuer", "subject");
        let payload = r#"{"iss":"issuer","sub":"subject"}"#;

        let auth = ds.validate_jwt_payload(payload, connection_id).unwrap();
        assert_eq!(auth.sender, expected);
        assert_eq!(auth.connection_id, Some(connection_id));

        assert!(ds.validate_jwt_payload(r#"{"sub":"subject"}"#, connection_id).is_err());
        let mismatch = format!(
            r#"{{"iss":"issuer","sub":"subject","hex_identity":"{}"}}"#,
            Identity::ZERO.to_hex()
        );
        assert!(ds.validate_jwt_payload(&mismatch, connection_id).is_err());
    }
}
