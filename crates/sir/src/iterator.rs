use crate::errors::ErrorVm;
use crate::stat::Stat;
use crate::table::TableGenerator;
use spacetimedb_lib::auth::StAccess;
use spacetimedb_lib::relation::{Header, MemTable, RelValue, RowCount};
use std::mem::size_of_val;

/// A trait for dealing with fallible iterators for the database.
pub trait RelOps {
    fn head(&self) -> &Header;
    fn access(&self) -> &StAccess;
    fn stat(&self) -> &Stat;
    /// Advances the `iterator` and returns the next [RelValue].
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm>;
    /// Applies a function over the elements of the iterator, producing a single final value.
    ///
    /// This is used as the "base" of many methods on `FallibleIterator`.
    #[inline]
    fn try_fold<B, E, F>(&mut self, mut init: B, mut f: F) -> Result<B, E>
    where
        Self: Sized,
        E: From<ErrorVm>,
        F: FnMut(B, RelValue) -> Result<B, ErrorVm>,
    {
        while let Some(v) = self.next()? {
            init = f(init, v)?;
        }
        Ok(init)
    }

    /// Utility to collect the results into a [Vec]
    #[inline]
    fn collect_vec(mut self) -> Result<Vec<RelValue>, ErrorVm>
    where
        Self: Sized,
    {
        let count = self.stat().rows;
        let estimate = count.max.unwrap_or(count.min);
        let mut result = Vec::with_capacity(estimate);

        while let Some(row) = self.next()? {
            result.push(row);
        }

        Ok(result)
    }
}

/// Common wrapper for relational iterators that work like cursors.
#[derive(Debug)]
pub struct RelIter<T> {
    pub head: Header,
    pub stat: Stat,
    pub pos: usize,
    pub of: T,
}

impl<T> RelIter<T> {
    pub fn new(head: Header, rows: RowCount, of: T) -> Self {
        Self {
            stat: Stat::new(rows, size_of_val(&head)),
            pos: 0,
            head,
            of,
        }
    }
}

impl RelOps for RelIter<&MemTable> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn access(&self) -> &StAccess {
        &StAccess::Public
    }

    fn stat(&self) -> &Stat {
        &self.stat
    }

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if self.pos < self.of.data.len() {
            let row = &self.of.data[self.pos];
            self.pos += 1;

            Ok(Some(row.clone()))
        } else {
            Ok(None)
        }
    }
}

impl RelOps for TableGenerator<'_> {
    fn head(&self) -> &Header {
        self.iter.head()
    }

    fn access(&self) -> &StAccess {
        self.iter.access()
    }

    fn stat(&self) -> &Stat {
        self.iter.stat()
    }

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if let Some(row) = self.iter.next()? {
            Ok(Some(row))
        } else {
            Ok(None)
        }
    }
}
