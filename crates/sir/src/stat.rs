use crate::ast::RelId;
use spacetimedb_lib::relation::RowCount;
use std::time::Instant;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Stat {
    pub rows: RowCount,
    /// size_of(RelId)
    pub width: usize,
    pub start: Instant,
}

impl Stat {
    pub fn new(rows: RowCount, width: usize) -> Self {
        Self {
            rows,
            width,
            start: Instant::now(),
        }
    }
}
