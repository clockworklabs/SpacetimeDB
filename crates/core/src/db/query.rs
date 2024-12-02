use crate::db::datastore::locking_tx_datastore::committed_state::CommittedState;
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::Tx;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_execution::iter::{Datastore, DeltaScanIter};
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_table::blob_store::BlobStore;
use spacetimedb_table::btree_index::BTreeIndex;
use spacetimedb_table::table::{IndexScanIter, RowRef, Table, TableScanIter};
use std::ops::RangeBounds;

pub trait DatastoreEx {
    fn get_committed_state(&self) -> &CommittedState;
    fn get_tables(&self) -> &IntMap<TableId, Table>;
}

impl Datastore for TxId {
    fn delta_scan_iter(&self, _table_id: TableId) -> DeltaScanIter {
        DeltaScanIter::empty_iter()
    }

    fn table_scan_iter(&self, table_id: TableId) -> TableScanIter {
        let table = self.committed_state_shared_lock.tables.get(&table_id).unwrap();
        table.scan_rows(self.get_blob_store())
    }

    fn index_scan_iter(&self, index_id: IndexId, range: &impl RangeBounds<AlgebraicValue>) -> IndexScanIter {
        let table = self.get_table_for_index(&index_id);
        let index = self.get_index(&index_id);

        let btree_index_iter = index.seek(range);
        IndexScanIter::new(table, self.get_blob_store(), btree_index_iter)
    }

    fn get_table_for_index(&self, index_id: &IndexId) -> &Table {
        let (table_id, _) = self.committed_state_shared_lock.index_id_map.get(index_id).unwrap();
        self.committed_state_shared_lock.tables.get(table_id).unwrap()
    }

    fn get_index(&self, index_id: &IndexId) -> &BTreeIndex {
        let table = self.get_table_for_index(index_id);
        table.indexes.values().find(|idx| idx.index_id == *index_id).unwrap()
    }

    fn get_blob_store(&self) -> &dyn BlobStore {
        &self.committed_state_shared_lock.blob_store
    }
}

impl DatastoreEx for TxId {
    fn get_committed_state(&self) -> &CommittedState {
        &self.committed_state_shared_lock
    }

    fn get_tables(&self) -> &IntMap<TableId, Table> {
        &self.committed_state_shared_lock.tables
    }
}

impl Datastore for MutTxId {
    fn delta_scan_iter(&self, _table_id: TableId) -> DeltaScanIter {
        DeltaScanIter::empty_iter()
    }

    fn table_scan_iter(&self, table_id: TableId) -> TableScanIter {
        let table = self.committed_state_write_lock.tables.get(&table_id).unwrap();
        table.scan_rows(self.get_blob_store())
    }

    fn index_scan_iter(&self, index_id: IndexId, range: &impl RangeBounds<AlgebraicValue>) -> IndexScanIter {
        let table = self.get_table_for_index(&index_id);
        let index = self.get_index(&index_id);

        let btree_index_iter = index.seek(range);
        IndexScanIter::new(table, self.get_blob_store(), btree_index_iter)
    }

    fn get_table_for_index(&self, index_id: &IndexId) -> &Table {
        let (table_id, _) = self.committed_state_write_lock.index_id_map.get(index_id).unwrap();
        self.committed_state_write_lock.tables.get(table_id).unwrap()
    }

    fn get_index(&self, index_id: &IndexId) -> &BTreeIndex {
        let table = self.get_table_for_index(index_id);
        table.indexes.values().find(|idx| idx.index_id == *index_id).unwrap()
    }

    fn get_blob_store(&self) -> &dyn BlobStore {
        &self.committed_state_write_lock.blob_store
    }
}

impl DatastoreEx for MutTxId {
    fn get_committed_state(&self) -> &CommittedState {
        &self.committed_state_write_lock
    }

    fn get_tables(&self) -> &IntMap<TableId, Table> {
        &self.committed_state_write_lock.tables
    }
}

pub struct Query<T> {
    pub tx: T,
}

impl<T> Query<T>
where
    T: Datastore + DatastoreEx,
{
    pub fn new(tx: T) -> Self {
        Self { tx }
    }

    pub fn into_tx(self) -> T {
        self.tx
    }

    pub fn iter_by_col_eq(&self, table_id: TableId, cols: impl Into<ColList>, value: &AlgebraicValue) -> IndexScanIter {
        self.tx
            .get_committed_state()
            .index_seek(table_id, &cols.into(), value)
            .unwrap()
    }

    pub fn iter_by_col_range(
        &self,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> IndexScanIter {
        self.tx
            .get_committed_state()
            .index_seek(table_id, &cols.into(), range)
            .unwrap()
    }
}

impl<T> DatastoreEx for Query<T>
where
    T: Datastore + DatastoreEx,
{
    fn get_committed_state(&self) -> &CommittedState {
        self.tx.get_committed_state()
    }

    fn get_tables(&self) -> &IntMap<TableId, Table> {
        self.tx.get_tables()
    }
}

pub fn collect_rows<'a>(iter: impl Iterator<Item = RowRef<'a>>) -> Vec<ProductValue> {
    iter.map(|row| row.to_product_value()).collect()
}

impl<T> Datastore for Query<T>
where
    T: Datastore + DatastoreEx,
{
    fn delta_scan_iter(&self, table_id: TableId) -> DeltaScanIter {
        self.tx.delta_scan_iter(table_id)
    }

    fn table_scan_iter(&self, table_id: TableId) -> TableScanIter {
        self.tx.table_scan_iter(table_id)
    }

    fn index_scan_iter(&self, index_id: IndexId, range: &impl RangeBounds<AlgebraicValue>) -> IndexScanIter {
        self.tx.index_scan_iter(index_id, range)
    }

    fn get_table_for_index(&self, index_id: &IndexId) -> &Table {
        self.tx.get_table_for_index(index_id)
    }

    fn get_index(&self, index_id: &IndexId) -> &BTreeIndex {
        self.tx.get_index(index_id)
    }

    fn get_blob_store(&self) -> &dyn BlobStore {
        self.tx.get_blob_store()
    }
}

impl<T: Tx + Datastore + DatastoreEx> From<T> for Query<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::error::DBError;
    use crate::execution_context::Workload;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::{product, AlgebraicType};

    fn create_data(total_rows: u64) -> ResultTest<(TestDB, TableId)> {
        let db = TestDB::in_memory()?;

        let rows: Vec<_> = (1..=total_rows)
            .map(|i| product!(i, format!("health{i}").into_boxed_str()))
            .collect();
        let schema = &[("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)];
        let indexes = &[(0.into(), "inventory_id")];
        let table_id = db.create_table_for_test("test", schema, indexes)?;

        db.with_auto_commit(Workload::ForTests, |tx| {
            for row in rows {
                db.insert(tx, table_id, row)?;
            }
            Ok::<(), DBError>(())
        })?;

        Ok((db, table_id))
    }

    #[test]
    fn table_scan() -> ResultTest<()> {
        let (db, table_id) = create_data(2)?;
        let tx = db.begin_tx(Workload::ForTests);

        let query = Query::new(tx);

        let iter = query.table_scan_iter(table_id);

        assert_eq!(
            collect_rows(iter),
            vec![product![1u64, "health1"], product![2u64, "health2"]]
        );

        Ok(())
    }

    #[test]
    fn table_scan_mut() -> ResultTest<()> {
        let (db, table_id) = create_data(2)?;

        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);

        let query = Query::new(tx);

        let iter = query.table_scan_iter(table_id);

        assert_eq!(
            collect_rows(iter),
            vec![product![1u64, "health1"], product![2u64, "health2"]]
        );

        Ok(())
    }

    #[test]
    fn index_scan() -> ResultTest<()> {
        let (db, table_id) = create_data(2)?;
        let tx = db.begin_tx(Workload::ForTests);

        let query = Query::new(tx);
        let index = query
            .get_committed_state()
            .tables
            .get(&table_id)
            .unwrap()
            .indexes
            .values()
            .next()
            .unwrap();

        let iter = query.index_scan_iter(index.index_id, &(AlgebraicValue::U64(1)..=AlgebraicValue::U64(2)));

        assert_eq!(
            collect_rows(iter),
            vec![product![1u64, "health1"], product![2u64, "health2"]]
        );

        Ok(())
    }

    #[test]
    fn eq() -> ResultTest<()> {
        let (db, table_id) = create_data(10)?;
        let tx = db.begin_tx(Workload::ForTests);

        let query = Query::new(tx);

        let iter = query.iter_by_col_eq(table_id, 0, &AlgebraicValue::U64(1));

        assert_eq!(collect_rows(iter), vec![product![1u64, "health1"]]);

        Ok(())
    }
}
