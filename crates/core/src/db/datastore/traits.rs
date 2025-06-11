use core::ops::Deref;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::{ops::RangeBounds, sync::Arc};

use super::locking_tx_datastore::datastore::TxMetrics;
use super::system_tables::ModuleKind;
use super::Result;
use crate::db::datastore::system_tables::ST_TABLE_ID;
use crate::execution_context::{ReducerContext, Workload};
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::{hash_bytes, Identity};
use spacetimedb_primitives::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use spacetimedb_schema::schema::{IndexSchema, SequenceSchema, TableSchema};
use spacetimedb_table::table::RowRef;

/// The `IsolationLevel` enum specifies the degree to which a transaction is
/// isolated from concurrently running transactions. The higher the isolation
/// level, the more protection a transaction has from the effects of other
/// transactions. The highest isolation level, `Serializable`, guarantees that
/// transactions produce effects which are indistinguishable from the effects
/// that would have been created by running the transactions one at a time, in
/// some order, even though they may not actually have been run one at a time.
///
/// NOTE: It is always possible to achieve `Serializable` isolation by running
/// transactions one at a time, although it is not necessarily performant.
///
/// Relaxing the isolation level can allow certain implementations to improve
/// performance at the cost of allowing the produced transactions to include
/// isolation anomalies. An isolation anomaly is a situation in which the
/// results of a transaction are affected by the presence of other transactions
/// running concurrently. Isolation anomalies can cause the database to violate
/// the Isolation properties of the ACID guarantee, but not Atomicity or
/// Durability.
///
/// Whether relaxing isolation level should be allowed to violate Consistency
/// guarantees of the datastore is of some debate, although most databases
/// choose to maintain consistency guarantees regardless of the isolation level,
/// and we should too even at the cost of performance. See the following for a
/// nuanced example of how postgres deals with consistency guarantees at lower
/// isolation levels.
///
/// - https://stackoverflow.com/questions/55254236/do-i-need-higher-transaction-isolation-to-make-constraints-work-reliably-in-post
///
/// Thus from an application perspective, isolation anomalies may cause the data
/// to be inconsistent or incorrect but will **not** cause it to violate the
/// consistency constraints of the database like referential integrity,
/// uniqueness, check constraints, etc.
///
/// NOTE: The datastore must treat unsupported isolation levels as though they
/// ran at the strongest supported level.
///
/// The SQL standard defines four levels of transaction isolation.
/// - Read Uncommitted
/// - Read Committed
/// - Repeatable Read
/// - Serializable
///
/// We include an additional isolation level, `Snapshot`, which is not part of
/// the SQL standard which offers a higher level of isolation than `Repeatable
/// Read`. Snapshot is the same as Serializable, but permits certain
/// serialization anomalies, such as write skew, to occur.
///
/// The ANSI SQL standard defined three anomalies in 1992:
///
/// - Dirty Reads: Occur when a transaction reads data written by a concurrent
///   uncommitted transaction.
///
/// - Non-repeatable Reads: Occur when a transaction reads the same row twice
///   and gets different data each time because another transaction has modified
///   the data in between the reads.
///
/// - Phantom Reads: Occur when a transaction re-executes a query returning a
///   set of rows that satisfy a search condition and finds that the set of rows
///   satisfying the condition has changed due to another recently-committed
///   transaction.
///
/// However since then database researchers have identified and cataloged many
/// more. See:
///
/// - https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/tr-95-51.pdf
/// - https://pmg.csail.mit.edu/papers/adya-phd.pdf
///
/// See the following table of anomalies for a more complete list used as a
/// reference for database implementers:
///
/// - https://github.com/ept/hermitage?tab=readme-ov-file#summary-of-test-results
///
/// The following anomalies are not part of the SQL standard, but are important:
///
/// - Write Skew: Occurs when two transactions concurrently read the same data,
///   make decisions based on that data, and then write back modifications that
///   are mutually inconsistent with the decisions made by the other transaction,
///   despite no direct conflict on the same row being detected. e.g. I read what
///   you write and you read what I write.
///
/// - Serialization Anomalies: Occur when the results of a set of transactions
///   are inconsistent with any serial execution of those transactions.
///
/// PostgreSQL's documentation provides a good summary of the anomalies and
/// isolation levels that it supports:
///
/// - https://www.postgresql.org/docs/current/transaction-iso.html
///
/// IMPORTANT!!! The order of these isolation levels in the enum is significant
/// because we often must check if one isolation level is higher (offers more
/// protection) than another, and the order is derived based on the lexical
/// order of the enum variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IsolationLevel {
    /// ReadUncommitted allows transactions to see changes made by other
    /// transactions even if those changes have not been committed. This level does
    /// not protect against any of the isolation anomalies, including dirty reads.
    ReadUncommitted,

    /// ReadCommitted guarantees that any data read is committed at the moment
    /// it is read.  Thus, it prevents dirty reads but does not prevent
    /// non-repeatable reads or phantom reads.
    ReadCommitted,

    /// RepeatableRead ensures that if a transaction reads the same data more
    /// than once, it will read the same data each time, thereby preventing
    /// non-repeatable reads.  However, it does not necessarily prevent phantom
    /// reads.
    RepeatableRead,

    /// Snapshot isolation provides a view of the database as it was at the
    /// beginning of the transaction, ensuring that the transaction can only see
    /// data committed before it started. This level of isolation guarantees
    /// consistency across multiple reads by providing each transaction with a
    /// "snapshot" of the database, preventing dirty reads, non-repeatable
    /// reads, and phantom reads. However, snapshot isolation does not
    /// completely eliminate all concurrency-related anomalies. One such anomaly
    /// is write skew, a situation where two transactions concurrently read the
    /// same data, make decisions based on that data, and then write back
    /// modifications that are mutually inconsistent with the decisions made by
    /// the other transaction, despite no direct conflict on the same row being
    /// detected. This can occur because each transaction operates on its
    /// snapshot without being aware of the other's uncommitted changes.  For
    /// instance, in a scheduling application, two transactions might
    /// concurrently check a condition (e.g., that a shift is not overstaffed),
    /// and both decide to add a worker based on that condition, leading to an
    /// overstaffing situation because they are unaware of each other's
    /// decisions. Snapshot isolation requires additional mechanisms, such as
    /// explicit locking or application-level checks, to prevent write skew
    /// anomalies.
    ///
    /// NOTE: Snapshot isolation does not permit write-write conflicts and any
    /// implementations of snapshot isolation must ensure that write-write
    /// conflicts cannot occur.
    Snapshot,

    /// Serializable is the highest isolation level, where transactions are
    /// executed with the illusion of being the only transaction running in the
    /// system. This level prevents dirty reads, non-repeatable reads, and
    /// phantom reads, effectively serializing access to the database to ensure
    /// complete isolation.
    ///
    /// Correct implementations of Serializable isolation must either actually
    /// permit only one transaction to run at a time , or track reads to ensure
    /// that the data which has been read by one transaction has not been
    /// modified by another transaction before the first transaction commits.
    Serializable,
}

/// A record of all the operations within a transaction.
///
/// Some extra information is embedded here
/// so that the recording of execution metrics can be done without holding the tx lock.
#[derive(Default)]
pub struct TxData {
    /// The inserted rows per table.
    inserts: BTreeMap<TableId, Arc<[ProductValue]>>,
    /// The deleted rows per table.
    deletes: BTreeMap<TableId, Arc<[ProductValue]>>,
    /// Map of all `TableId`s in both `inserts` and `deletes` to their
    /// corresponding table name.
    tables: IntMap<TableId, String>,
    /// Tx offset of the transaction which performed these operations.
    ///
    /// `None` implies that `inserts` and `deletes` are both empty,
    /// but `Some` does not necessarily imply that either is non-empty.
    tx_offset: Option<u64>,
    // TODO: Store an `Arc<String>` or equivalent instead.
}

impl TxData {
    /// Set `tx_offset` as the expected on-disk transaction offset of this transaction.
    pub fn set_tx_offset(&mut self, tx_offset: u64) {
        self.tx_offset = Some(tx_offset);
    }

    /// Read the expected on-disk transaction offset of this transaction.
    ///
    /// `None` implies that this [`TxData`] contains zero inserted or deleted rows,
    /// but the inverse is not necessarily true;
    /// a [`TxData`] may have a `tx_offset` but no row operations.
    pub fn tx_offset(&self) -> Option<u64> {
        self.tx_offset
    }

    /// Set `rows` as the inserted rows for `(table_id, table_name)`.
    pub fn set_inserts_for_table(&mut self, table_id: TableId, table_name: &str, rows: Arc<[ProductValue]>) {
        self.inserts.insert(table_id, rows);
        self.tables.entry(table_id).or_insert_with(|| table_name.to_owned());
    }

    /// Set `rows` as the deleted rows for `(table_id, table_name)`.
    pub fn set_deletes_for_table(&mut self, table_id: TableId, table_name: &str, rows: Arc<[ProductValue]>) {
        self.deletes.insert(table_id, rows);
        self.tables.entry(table_id).or_insert_with(|| table_name.to_owned());
    }

    /// Obtain an iterator over the inserted rows per table.
    pub fn inserts(&self) -> impl Iterator<Item = (&TableId, &Arc<[ProductValue]>)> + '_ {
        self.inserts.iter()
    }

    /// Get the `i`th inserted row for `table_id` if it exists
    pub fn get_ith_insert(&self, table_id: TableId, i: usize) -> Option<&ProductValue> {
        self.inserts.get(&table_id).and_then(|rows| rows.get(i))
    }

    /// Obtain an iterator over the inserted rows per table.
    ///
    /// If you don't need access to the table name, [`Self::inserts`] is
    /// slightly more efficient.
    pub fn inserts_with_table_name(&self) -> impl Iterator<Item = (&TableId, &str, &Arc<[ProductValue]>)> + '_ {
        self.inserts.iter().map(|(table_id, rows)| {
            let table_name = self
                .tables
                .get(table_id)
                .expect("invalid `TxData`: partial table name mapping");
            (table_id, table_name.as_str(), rows)
        })
    }

    /// Obtain an iterator over the deleted rows per table.
    pub fn deletes(&self) -> impl Iterator<Item = (&TableId, &Arc<[ProductValue]>)> + '_ {
        self.deletes.iter()
    }

    /// Get the `i`th deleted row for `table_id` if it exists
    pub fn get_ith_delete(&self, table_id: TableId, i: usize) -> Option<&ProductValue> {
        self.deletes.get(&table_id).and_then(|rows| rows.get(i))
    }

    /// Obtain an iterator over the inserted rows per table.
    ///
    /// If you don't need access to the table name, [`Self::deletes`] is
    /// slightly more efficient.
    pub fn deletes_with_table_name(&self) -> impl Iterator<Item = (&TableId, &str, &Arc<[ProductValue]>)> + '_ {
        self.deletes.iter().map(|(table_id, rows)| {
            let table_name = self
                .tables
                .get(table_id)
                .expect("invalid `TxData`: partial table name mapping");
            (table_id, table_name.as_str(), rows)
        })
    }

    /// Check if this [`TxData`] contains any `inserted | deleted` rows or `connect/disconnect` operations.
    ///
    /// This is used to determine if a transaction should be written to disk.
    pub fn has_rows_or_connect_disconnect(&self, reducer_context: Option<&ReducerContext>) -> bool {
        self.inserts().any(|(_, inserted_rows)| !inserted_rows.is_empty())
            || self.deletes().any(|(_, deleted_rows)| !deleted_rows.is_empty())
            || matches!(
                reducer_context.map(|rcx| rcx.name.strip_prefix("__identity_")),
                Some(Some("connected__" | "disconnected__"))
            )
    }

    /// Returns a list of tables affected in this transaction.
    pub fn table_ids_and_names(&self) -> impl '_ + Iterator<Item = (TableId, &str)> {
        self.tables.iter().map(|(k, v)| (*k, &**v))
    }

    /// Returns the number o tables affected in this transaction.
    pub fn num_tables_affected(&self) -> usize {
        self.tables.len()
    }
}

/// The result of [`MutTxDatastore::row_type_for_table_mut_tx`] and friends.
/// This is a smart pointer returning a `&ProductType`.
pub enum RowTypeForTable<'a> {
    /// A reference can be stored to the type.
    Ref(&'a ProductType),
    /// The type is within the schema.
    Arc(Arc<TableSchema>),
}

impl Deref for RowTypeForTable<'_> {
    type Target = ProductType;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(x) => x,
            Self::Arc(x) => x.get_row_type(),
        }
    }
}

pub trait Data: Into<ProductValue> {
    fn view(&self) -> Cow<'_, ProductValue>;
}

pub trait DataRow: Send + Sync {
    type RowId: Copy;

    type RowRef<'a>;

    /// Assuming `row_ref` refers to a row in `st_tables`,
    /// read out the table id from the row.
    fn read_table_id(&self, row_ref: Self::RowRef<'_>) -> Result<TableId>;
}

pub trait Tx {
    type Tx;

    /// Begins a read-only transaction under the given `workload`.
    fn begin_tx(&self, workload: Workload) -> Self::Tx;

    /// Release this read-only transaction.
    ///
    /// Returns:
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran within this transaction.
    fn release_tx(&self, tx: Self::Tx) -> (TxMetrics, String);
}

pub trait MutTx {
    type MutTx;

    /// Begins a mutable transaction under the given `isolation_level` and `workload`.
    fn begin_mut_tx(&self, isolation_level: IsolationLevel, workload: Workload) -> Self::MutTx;

    /// Commits `tx`, applying its changes to the committed state.
    ///
    /// Returns:
    /// - [`TxData`], the set of inserts and deletes performed by this transaction.
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran during this transaction.
    fn commit_mut_tx(&self, tx: Self::MutTx) -> Result<Option<(TxData, TxMetrics, String)>>;

    /// Rolls back this transaction, discarding its changes.
    ///
    /// Returns:
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran within this transaction.
    fn rollback_mut_tx(&self, tx: Self::MutTx) -> (TxMetrics, String);
}

/// Standard metadata associated with a database.
#[derive(Debug)]
pub struct Metadata {
    /// The stable [`Identity`] of the database.
    pub database_identity: Identity,
    /// The identity of the database's owner.
    pub owner_identity: Identity,
    /// The hash of the binary module set for the database.
    pub program_hash: Hash,
}

/// Program associated with a database.
pub struct Program {
    /// Hash over the program's bytes.
    pub hash: Hash,
    /// The raw bytes of the program.
    pub bytes: Box<[u8]>,
}

impl Program {
    /// Create a [`Program`] from its raw bytes.
    ///
    /// This computes the hash over `bytes`, so prefer constructing [`Program`]
    /// directly if the hash is already known.
    pub fn from_bytes(bytes: impl Into<Box<[u8]>>) -> Self {
        let bytes = bytes.into();
        let hash = hash_bytes(&bytes);
        Self { hash, bytes }
    }

    /// Create a [`Program`] with no bytes.
    pub fn empty() -> Self {
        Self::from_bytes([])
    }
}

/// Additional information about an insert operation.
pub struct InsertFlags {
    /// Is the table a scheduler table?
    pub is_scheduler_table: bool,
}

/// Additional information about an update operation.
// TODO(centril): consider fusing this with `InsertFlags`.
pub struct UpdateFlags {
    /// Is the table a scheduler table?
    pub is_scheduler_table: bool,
}

pub trait TxDatastore: DataRow + Tx {
    type IterTx<'a>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    type IterByColRangeTx<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;
    type IterByColEqTx<'a, 'r>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    fn iter_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Self::IterTx<'a>>;

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRangeTx<'a, R>>;

    fn iter_by_col_eq_tx<'a, 'r>(
        &'a self,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEqTx<'a, 'r>>;

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool;
    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>>;
    fn table_name_from_id_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>>;
    fn schema_for_table_tx(&self, tx: &Self::Tx, table_id: TableId) -> super::Result<Arc<TableSchema>>;
    fn get_all_tables_tx(&self, tx: &Self::Tx) -> super::Result<Vec<Arc<TableSchema>>>;

    /// Obtain the [`Metadata`] for this datastore.
    ///
    /// A `None` return value means that the datastore is not fully initialized yet.
    fn metadata(&self, tx: &Self::Tx) -> Result<Option<Metadata>>;

    /// Obtain the compiled module associated with this datastore.
    ///
    /// A `None` return value means that the datastore is not fully initialized yet.
    fn program(&self, tx: &Self::Tx) -> Result<Option<Program>>;
}

pub trait MutTxDatastore: TxDatastore + MutTx {
    type IterMutTx<'a>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    type IterByColRangeMutTx<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    type IterByColEqMutTx<'a, 'r>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    // Tables
    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableSchema) -> Result<TableId>;
    // In these methods, we use `'tx` because the return type must borrow data
    // from `Inner` in the `Locking` implementation,
    // and `Inner` lives in `tx: &MutTxId`.
    fn row_type_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<RowTypeForTable<'tx>>;
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTx, table_id: TableId) -> Result<Arc<TableSchema>>;
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId) -> Result<()>;
    fn rename_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, new_name: &str) -> Result<()>;
    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTx, table_name: &str) -> Result<Option<TableId>>;
    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool;
    fn table_name_from_id_mut_tx<'a>(&'a self, tx: &'a Self::MutTx, table_id: TableId) -> Result<Option<Cow<'a, str>>>;
    fn get_all_tables_mut_tx(&self, tx: &Self::MutTx) -> super::Result<Vec<Arc<TableSchema>>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_mut_tx(tx, ST_TABLE_ID)?.collect::<Vec<_>>();
        for row in table_rows {
            let table_id = self.read_table_id(row)?;
            tables.push(self.schema_for_table_mut_tx(tx, table_id)?);
        }
        Ok(tables)
    }

    // Indexes

    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, index_schema: IndexSchema, is_unique: bool) -> Result<IndexId>;
    fn drop_index_mut_tx(&self, tx: &mut Self::MutTx, index_id: IndexId) -> Result<()>;
    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTx, index_name: &str) -> super::Result<Option<IndexId>>;

    // TODO: Index data
    // - index_scan_mut_tx
    // - index_range_scan_mut_tx
    // - index_seek_mut_tx

    // Sequences
    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<i128>;
    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTx, sequence_schema: SequenceSchema) -> Result<SequenceId>;
    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<()>;
    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> super::Result<Option<SequenceId>>;

    // Constraints
    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> super::Result<()>;
    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> super::Result<Option<ConstraintId>>;

    // Data
    fn iter_mut_tx<'a>(&'a self, tx: &'a Self::MutTx, table_id: TableId) -> Result<Self::IterMutTx<'a>>;
    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRangeMutTx<'a, R>>;
    fn iter_by_col_eq_mut_tx<'a, 'r>(
        &'a self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEqMutTx<'a, 'r>>;
    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> Result<Option<Self::RowRef<'a>>>;
    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row_ids: impl IntoIterator<Item = Self::RowId>,
    ) -> u32;
    fn delete_by_rel_mut_tx(
        &self,
        tx: &mut Self::MutTx,
        table_id: TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32;
    /// Inserts `row`, encoded in BSATN, into the table identified by `table_id`.
    ///
    /// Returns the list of columns with sequence-trigger values that were replaced with generated ones
    /// and a reference to the row as a [`RowRef`].
    /// Also returns any additional insert flags.
    ///
    /// Generated columns are columns with an auto-inc sequence
    /// and where the column was `0` in `row`.
    // TODO(centril): consider making the tuple into a struct.
    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, InsertFlags)>;
    /// Updates a row to `row`, encoded in BSATN, into the table identified by `table_id`
    /// using the index identified by `index_id`.
    ///
    /// Returns the list of columns with sequence-trigger values that were replaced with generated ones
    /// and a reference to the row as a [`RowRef`].
    /// Also returns any additional update flags.
    ///
    /// Generated columns are columns with an auto-inc sequence
    /// and where the column was `0` in `row`.
    // TODO(centril): consider making the tuple into a struct.
    fn update_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, UpdateFlags)>;

    /// Obtain the [`Metadata`] for this datastore.
    ///
    /// Like [`TxDatastore`], but in a mutable transaction context.
    fn metadata_mut_tx(&self, tx: &Self::MutTx) -> Result<Option<Metadata>>;

    /// Update the datastore with the supplied binary program.
    fn update_program(&self, tx: &mut Self::MutTx, program_kind: ModuleKind, program: Program) -> Result<()>;
}
