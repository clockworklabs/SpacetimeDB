/// Metrics collected during the course of a transaction.
#[derive(Debug, Default, Copy, Clone)]
pub struct ExecutionMetrics {
    /// How many times is an index probed?
    ///
    /// Note that a single btree scan may return many values,
    /// but will only result in a single index seek.
    pub index_seeks: usize,
    /// How many rows are iterated over?
    ///
    /// It is independent of the number of rows returned.
    /// A query for example may return a single row,
    /// but if it scans the entire table to find that row,
    /// this metric will reflect that.
    pub rows_scanned: usize,
    /// How many bytes are read?
    ///
    /// This metric is incremented anytime we dereference a `RowPointer`.
    ///
    /// For reducers this happens at the WASM boundary,
    /// when serializing entire rows via the BSATN encoding.
    ///
    /// In addition to the same BSATN serialization of the output rows,
    /// queries will dereference a `RowPointer` for column projections.
    /// Such is the case for fiters as well as index and hash joins.
    ///
    /// One place where this metric is not tracked is index scans.
    /// Specifically the key comparisons that occur during the scan.
    pub bytes_scanned: usize,
    /// How many bytes are written?
    ///
    /// Note, this is the same as bytes inserted,
    /// because deletes just update a free list in the datastore.
    /// They don't actually write or clear page memory.
    pub bytes_written: usize,
    /// How many bytes did we send to clients?
    ///
    /// This is not necessarily the same as bytes scanned,
    /// since a single query may send bytes to multiple clients.
    ///
    /// In general, these are BSATN bytes, but JSON is also possible.
    pub bytes_sent_to_clients: usize,
    /// How many rows were inserted?
    pub rows_inserted: u64,
    /// How many rows were deleted?
    pub rows_deleted: u64,
    /// How many rows were updated?
    pub rows_updated: u64,
    /// How many subscription updates did we execute?
    pub delta_queries_evaluated: u64,
    /// How many subscriptions had some updates?
    pub delta_queries_matched: u64,
    /// How many times do we evaluate the same row in a subscription update?
    pub duplicate_rows_evaluated: u64,
    /// How many duplicate rows do we send in a subscription update?
    pub duplicate_rows_sent: u64,
}

impl ExecutionMetrics {
    pub fn merge(
        &mut self,
        ExecutionMetrics {
            index_seeks,
            rows_scanned,
            bytes_scanned,
            bytes_written,
            bytes_sent_to_clients,
            rows_inserted,
            rows_deleted,
            rows_updated,
            delta_queries_evaluated,
            delta_queries_matched,
            duplicate_rows_evaluated,
            duplicate_rows_sent,
        }: ExecutionMetrics,
    ) {
        self.index_seeks += index_seeks;
        self.rows_scanned += rows_scanned;
        self.bytes_scanned += bytes_scanned;
        self.bytes_written += bytes_written;
        self.bytes_sent_to_clients += bytes_sent_to_clients;
        self.rows_inserted += rows_inserted;
        self.rows_deleted += rows_deleted;
        self.rows_updated += rows_updated;
        self.delta_queries_evaluated += delta_queries_evaluated;
        self.delta_queries_matched += delta_queries_matched;
        self.duplicate_rows_evaluated += duplicate_rows_evaluated;
        self.duplicate_rows_sent += duplicate_rows_sent;
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionMetrics;

    #[test]
    fn test_merge() {
        let mut a = ExecutionMetrics::default();

        a.merge(ExecutionMetrics {
            index_seeks: 1,
            rows_scanned: 1,
            bytes_scanned: 1,
            bytes_written: 1,
            bytes_sent_to_clients: 1,
            rows_inserted: 1,
            rows_deleted: 1,
            rows_updated: 1,
            delta_queries_evaluated: 2,
            delta_queries_matched: 3,
            duplicate_rows_evaluated: 4,
            duplicate_rows_sent: 2,
        });

        assert_eq!(a.index_seeks, 1);
        assert_eq!(a.rows_scanned, 1);
        assert_eq!(a.bytes_scanned, 1);
        assert_eq!(a.bytes_written, 1);
        assert_eq!(a.bytes_sent_to_clients, 1);
        assert_eq!(a.rows_inserted, 1);
        assert_eq!(a.rows_deleted, 1);
        assert_eq!(a.delta_queries_evaluated, 2);
        assert_eq!(a.delta_queries_matched, 3);
        assert_eq!(a.duplicate_rows_evaluated, 4);
        assert_eq!(a.duplicate_rows_sent, 2);
    }
}
