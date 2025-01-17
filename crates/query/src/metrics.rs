use spacetimedb_execution::ExecutionMetrics;

#[derive(Default)]
pub struct QueryMetrics {
    /// How many times do we probe an index?
    pub index_seeks: usize,
    /// How many rows does each operator iterate over?
    pub rows_scanned: usize,
}

impl QueryMetrics {
    pub fn merge(&mut self, with: QueryMetrics) {
        self.index_seeks += with.index_seeks;
        self.rows_scanned += with.rows_scanned;
    }
}

impl ExecutionMetrics for QueryMetrics {
    fn inc_index_seeks_by(&mut self, seeks: usize) {
        self.index_seeks += seeks;
    }

    fn inc_rows_scanned_by(&mut self, rows: usize) {
        self.rows_scanned += rows;
    }
}
