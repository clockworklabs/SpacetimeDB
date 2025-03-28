use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::ColId;

use crate::schemas::{BenchTable, IndexStrategy};
use crate::ResultBench;

/// A database we can execute a standard benchmark suite against.
/// Currently implemented for SQLite, raw Spacetime outside a module boundary
/// (RelationalDB), and Spacetime through the module boundary.
///
/// Not all benchmarks have to go through this trait.
pub trait BenchDatabase: Sized {
    fn name() -> String;

    type TableId: Clone + 'static;

    fn build(in_memory: bool) -> ResultBench<Self>
    where
        Self: Sized;

    fn create_table<T: BenchTable>(&mut self, table_style: IndexStrategy) -> ResultBench<Self::TableId>;

    /// Should not drop the table, only delete all the rows.
    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()>;

    /// Count the number of rows in the table.
    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32>;

    /// Perform an empty transaction.
    fn empty_transaction(&mut self) -> ResultBench<()>;

    /// Perform a transaction that commits many rows.
    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()>;

    /// Perform a transaction that updates many rows, without
    /// sending updated rows across a serialization boundary.
    /// (e.g. in a module, the updates are generated *inside* the reducer).
    /// Update logic should be fast, e.g. wrapping integer increment.
    /// Requires that the table was created with `IndexStrategy::Unique`
    fn update_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, row_count: u32) -> ResultBench<()>;

    /// Perform a transaction that iterates an entire database table.
    /// Note: this can be non-generic because none of the implementations use the relevant generic argument.
    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()>;

    /// Filter the table on the specified column id for the specified value.
    fn filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        col_id: impl Into<ColId>,
        value: AlgebraicValue,
    ) -> ResultBench<()>;
}
