use std::borrow::Cow;
use std::{ops::RangeBounds, sync::Arc};

use super::Result;
use crate::db::datastore::system_tables::ST_TABLES_ID;
use crate::execution_context::ExecutionContext;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::DataKey;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};

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
/// uncommitted transaction.
///
/// - Non-repeatable Reads: Occur when a transaction reads the same row twice
/// and gets different data each time because another transaction has modified
/// the data in between the reads.
///
/// - Phantom Reads: Occur when a transaction re-executes a query returning a
/// set of rows that satisfy a search condition and finds that the set of rows
/// satisfying the condition has changed due to another recently-committed
/// transaction.
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
/// make decisions based on that data, and then write back modifications that
/// are mutually inconsistent with the decisions made by the other transaction,
/// despite no direct conflict on the same row being detected. e.g. I read what
/// you write and you read what I write.
///
/// - Serialization Anomalies: Occur when the results of a set of transactions
/// are inconsistent with any serial execution of those transactions.

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

/// Operations in a transaction are either Inserts or Deletes.
/// Inserts report the byte objects they inserted, to be persisted
/// later in an object store.
pub enum TxOp {
    Insert(Arc<Vec<u8>>),
    Delete,
}

/// A record of a single operation within a transaction.
pub struct TxRecord {
    /// Whether the operation was an insert or a delete.
    pub(crate) op: TxOp,
    /// The value of the modified row.
    pub(crate) product_value: ProductValue,
    /// The key of the modified row.
    pub(crate) key: DataKey,
    /// The table that was modified.
    pub(crate) table_id: TableId,
    /// The table that was modified.
    pub(crate) table_name: String,
}

/// A record of all the operations within a transaction.
pub struct TxData {
    pub(crate) records: Vec<TxRecord>,
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

    fn begin_tx(&self) -> Self::Tx;
    fn release_tx(&self, ctx: &ExecutionContext, tx: Self::Tx);
}

pub trait MutTx {
    type MutTx;

    fn begin_mut_tx(&self, isolation_level: IsolationLevel) -> Self::MutTx;
    fn commit_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) -> Result<Option<TxData>>;
    fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx);

    #[cfg(test)]
    fn commit_mut_tx_for_test(&self, tx: Self::MutTx) -> Result<Option<TxData>>;

    #[cfg(test)]
    fn rollback_mut_tx_for_test(&self, tx: Self::MutTx);
}

pub trait TxDatastore: DataRow + Tx {
    type Iter<'a>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    type IterByColEq<'a, 'r>: Iterator<Item = Self::RowRef<'a>>
    where
        Self: 'a;

    fn iter_tx<'a>(&'a self, ctx: &'a ExecutionContext, tx: &'a Self::Tx, table_id: TableId) -> Result<Self::Iter<'a>>;

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;

    fn iter_by_col_eq_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>>;

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool;
    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>>;
    fn table_name_from_id_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>>;
    fn schema_for_table_tx<'tx>(&self, tx: &'tx Self::Tx, table_id: TableId) -> super::Result<Cow<'tx, TableSchema>>;
    fn get_all_tables_tx<'tx>(
        &self,
        ctx: &ExecutionContext,
        tx: &'tx Self::Tx,
    ) -> super::Result<Vec<Cow<'tx, TableSchema>>>;
}

pub trait MutTxDatastore: TxDatastore + MutTx {
    // Tables
    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableDef) -> Result<TableId>;
    // In these methods, we use `'tx` because the return type must borrow data
    // from `Inner` in the `Locking` implementation,
    // and `Inner` lives in `tx: &MutTxId`.
    fn row_type_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<Cow<'tx, ProductType>>;
    fn schema_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<Cow<'tx, TableSchema>>;
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId) -> Result<()>;
    fn rename_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, new_name: &str) -> Result<()>;
    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTx, table_name: &str) -> Result<Option<TableId>>;
    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool;
    fn table_name_from_id_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> Result<Option<Cow<'a, str>>>;
    fn get_all_tables_mut_tx<'tx>(
        &self,
        ctx: &ExecutionContext,
        tx: &'tx Self::MutTx,
    ) -> super::Result<Vec<Cow<'tx, TableSchema>>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_mut_tx(ctx, tx, ST_TABLES_ID)?.collect::<Vec<_>>();
        for row in table_rows {
            let table_id = self.read_table_id(row)?;
            tables.push(self.schema_for_table_mut_tx(tx, table_id)?);
        }
        Ok(tables)
    }

    // Indexes
    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, index: IndexDef) -> Result<IndexId>;
    fn drop_index_mut_tx(&self, tx: &mut Self::MutTx, index_id: IndexId) -> Result<()>;
    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTx, index_name: &str) -> super::Result<Option<IndexId>>;

    // TODO: Index data
    // - index_scan_mut_tx
    // - index_range_scan_mut_tx
    // - index_seek_mut_tx

    // Sequences
    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<i128>;
    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, seq: SequenceDef) -> Result<SequenceId>;
    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<()>;
    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> super::Result<Option<SequenceId>>;

    // Constraints
    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> super::Result<()>;
    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> super::Result<Option<ConstraintId>>;

    // Data
    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> Result<Self::Iter<'a>>;
    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;
    fn iter_by_col_eq_mut_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>>;
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
    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row: ProductValue,
    ) -> Result<ProductValue>;
}

/// Describes a programmable [`TxDatastore`].
///
/// A programmable datastore is one which has a program of some kind associated
/// with it.
pub trait Programmable: TxDatastore {
    /// Retrieve the [`Hash`] of the program currently associated with the
    /// datastore.
    ///
    /// A `None` result means that no program is currently associated, e.g.
    /// because the datastore has not been fully initialized yet.
    fn program_hash(&self, tx: &Self::Tx) -> Result<Option<Hash>>;
}

/// Describes a [`Programmable`] datastore which allows to update the program
/// associated with it.
pub trait MutProgrammable: MutTxDatastore {
    /// A fencing token (usually a monotonic counter) which allows to order
    /// `set_module_hash` with respect to a distributed locking service.
    type FencingToken: Eq + Ord;

    /// Update the [`Hash`] of the program currently associated with the
    /// datastore.
    ///
    /// The operation runs within the transactional context `tx`. The fencing
    /// token `fence` must be verified to be greater than in any previous
    /// invocations of this method.
    fn set_program_hash(&self, tx: &mut Self::MutTx, fence: Self::FencingToken, hash: Hash) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use spacetimedb_primitives::{col_list, ColId, Constraints};
    use spacetimedb_sats::db::def::ConstraintDef;
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, ProductTypeElement, Typespace};

    use super::{ColumnDef, IndexDef, TableDef};

    #[test]
    fn test_tabledef_from_lib_tabledef() -> anyhow::Result<()> {
        let mut expected_schema = TableDef::new(
            "Person".into(),
            vec![
                ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                },
                ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                },
            ],
        )
        .with_indexes(vec![
            IndexDef::btree("id_and_name".into(), col_list![0, 1], false),
            IndexDef::btree("just_name".into(), ColId(1), false),
        ])
        .with_constraints(vec![ConstraintDef::new(
            "identity".into(),
            Constraints::identity(),
            ColId(0),
        )]);

        let lib_table_def = spacetimedb_lib::TableDesc {
            schema: expected_schema.clone(),
            data: AlgebraicTypeRef(0),
        };
        let row_type = ProductType::new(vec![
            ProductTypeElement {
                name: Some("id".into()),
                algebraic_type: AlgebraicType::U32,
            },
            ProductTypeElement {
                name: Some("name".into()),
                algebraic_type: AlgebraicType::String,
            },
        ]);

        let mut datastore_schema = spacetimedb_lib::TableDesc::into_table_def(
            Typespace::new(vec![row_type.into()]).with_type(&lib_table_def),
        )?;

        for schema in [&mut datastore_schema, &mut expected_schema] {
            schema.columns.sort_by(|a, b| a.col_name.cmp(&b.col_name));
            schema.indexes.sort_by(|a, b| a.index_name.cmp(&b.index_name));
        }

        assert_eq!(expected_schema, datastore_schema);

        Ok(())
    }
}
