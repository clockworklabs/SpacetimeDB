use crate::errors::ErrorVm;
use crate::rel_ops::RelOps;
use crate::relation::{MemTable, RelValue};
use core::mem;
use spacetimedb_sats::relation::{Header, RowCount};
use std::sync::Arc;

/// Common wrapper for relational iterators that work like cursors.
#[derive(Debug)]
pub struct RelIter<T> {
    pub head: Arc<Header>,
    pub row_count: RowCount,
    pub pos: usize,
    pub of: T,
}

impl<T> RelIter<T> {
    pub fn new(head: Arc<Header>, row_count: RowCount, of: T) -> Self {
        Self {
            head,
            row_count,
            pos: 0,
            of,
        }
    }
}

impl<'a> RelOps<'a> for RelIter<MemTable> {
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        Ok((self.pos < self.of.data.len()).then(|| {
            let row = &mut self.of.data[self.pos];
            self.pos += 1;

            RelValue::Projection(mem::take(row))
        }))
    }
}
