/// A type used by the query planner for incremental evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delta {
    Inserts(usize),
    Deletes(usize),
}
