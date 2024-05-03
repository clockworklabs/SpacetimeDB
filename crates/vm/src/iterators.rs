use crate::rel_ops::RelOps;
use crate::relation::RelValue;
use spacetimedb_sats::relation::{Header, RowCount};
use std::sync::Arc;

/// Turns an iterator over `ProductValue`s into a `RelOps`.
#[derive(Debug)]
pub struct RelIter<I> {
    pub head: Arc<Header>,
    pub row_count: RowCount,
    pub iter: I,
}

impl<I> RelIter<I> {
    pub fn new(head: Arc<Header>, row_count: RowCount, iter: impl IntoIterator<IntoIter = I>) -> Self {
        let iter = iter.into_iter();
        Self { head, row_count, iter }
    }
}

impl<'a, I: Iterator<Item = RelValue<'a>>> RelOps<'a> for RelIter<I> {
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Option<RelValue<'a>> {
        self.iter.next()
    }
}
