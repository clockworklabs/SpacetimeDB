/// Metrics collected during the course of a transaction
#[derive(Default)]
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
        }: ExecutionMetrics,
    ) {
        self.index_seeks += index_seeks;
        self.rows_scanned += rows_scanned;
        self.bytes_scanned += bytes_scanned;
        self.bytes_written += bytes_written;
        self.bytes_sent_to_clients += bytes_sent_to_clients;
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
        });

        assert_eq!(a.index_seeks, 1);
        assert_eq!(a.rows_scanned, 1);
        assert_eq!(a.bytes_scanned, 1);
        assert_eq!(a.bytes_written, 1);
        assert_eq!(a.bytes_sent_to_clients, 1);
    }
}
