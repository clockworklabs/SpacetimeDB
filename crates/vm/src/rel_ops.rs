use crate::errors::ErrorVm;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{FieldExpr, Header, RelValue, RelValueRef, RowCount};
use std::collections::HashMap;

pub(crate) trait ResultExt<T> {
    fn unpack_fold(self) -> Result<T, ErrorVm>;
}

/// A trait for dealing with fallible iterators for the database.
pub trait RelOps {
    fn head(&self) -> &Header;
    fn row_count(&self) -> RowCount;
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

    /// Creates an `Iterator` which uses a closure to determine if a [RelValueRef] should be yielded.
    ///
    /// Given a [RelValueRef] the closure must return true or false.
    /// The returned iterator will yield only the elements for which the closure returns true.
    ///
    /// Note:
    ///
    /// It is the equivalent of a `WHERE` clause on SQL.
    #[inline]
    fn select<P>(self, predicate: P) -> Select<Self, P>
    where
        P: FnMut(RelValueRef) -> Result<bool, ErrorVm>,
        Self: Sized,
    {
        let count = self.row_count();
        let head = self.head().clone();
        Select::new(self, count, head, predicate)
    }

    /// Creates an `Iterator` which uses a closure that project a new [ProductValue] extracted from the current.
    ///
    /// Given a [ProductValue] the closure must return a subset of the current one.
    ///
    /// The [Header] is pre-checked that all the fields exist and return a error if any field is not found.
    ///
    /// Note:
    ///
    /// It is the equivalent of a `SELECT` clause on SQL.
    #[inline]
    fn project<P>(self, cols: &[FieldExpr], extractor: P) -> Result<Project<Self, P>, ErrorVm>
    where
        P: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
        Self: Sized,
    {
        let count = self.row_count();
        let head = self.head().project(cols)?;
        Ok(Project::new(self, count, head, extractor))
    }

    /// Intersection between the left and the right, both (non-sorted) `iterators`.
    ///
    /// The hash join strategy requires the right iterator can be collected to a `HashMap`.
    /// The left iterator can be arbitrarily long.
    ///
    /// It is therefore asymmetric (you can't flip the iterators to get a right_outer join).
    ///    
    /// Note:
    ///
    /// It is the equivalent of a `INNER JOIN` clause on SQL.
    #[inline]
    fn join_inner<Pred, Proj, KeyLhs, KeyRhs, Rhs>(
        self,
        with: Rhs,
        head: Header,
        key_lhs: KeyLhs,
        key_rhs: KeyRhs,
        predicate: Pred,
        project: Proj,
    ) -> Result<JoinInner<Self, Rhs, KeyLhs, KeyRhs, Pred, Proj>, ErrorVm>
    where
        Self: Sized,
        Pred: FnMut(RelValueRef, RelValueRef) -> Result<bool, ErrorVm>,
        Proj: FnMut(RelValue, RelValue) -> RelValue,
        KeyLhs: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
        KeyRhs: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
        Rhs: RelOps,
    {
        Ok(JoinInner::new(head, self, with, key_lhs, key_rhs, predicate, project))
    }

    /// Utility to collect the results into a [Vec]
    #[inline]
    fn collect_vec(mut self) -> Result<Vec<RelValue>, ErrorVm>
    where
        Self: Sized,
    {
        let count = self.row_count();
        let estimate = count.max.unwrap_or(count.min);
        let mut result = Vec::with_capacity(estimate);

        while let Some(row) = self.next()? {
            result.push(row);
        }

        Ok(result)
    }
}

impl<I: RelOps + ?Sized> RelOps for Box<I> {
    fn head(&self) -> &Header {
        (**self).head()
    }

    fn row_count(&self) -> RowCount {
        (**self).row_count()
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        (**self).next()
    }
}

#[derive(Clone, Debug)]
pub struct Select<I, P> {
    pub(crate) head: Header,
    pub(crate) count: RowCount,
    pub(crate) iter: I,
    pub(crate) predicate: P,
}

impl<I, P> Select<I, P> {
    pub fn new(iter: I, count: RowCount, head: Header, predicate: P) -> Select<I, P> {
        Select {
            iter,
            count,
            predicate,
            head,
        }
    }
}

impl<I, P> RelOps for Select<I, P>
where
    I: RelOps,
    P: FnMut(RelValueRef) -> Result<bool, ErrorVm>,
{
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.count
    }

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        let filter = &mut self.predicate;
        while let Some(v) = self.iter.next()? {
            if filter(v.as_val_ref())? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct Project<I, P> {
    pub(crate) head: Header,
    pub(crate) count: RowCount,
    pub(crate) iter: I,
    pub(crate) extractor: P,
}

impl<I, P> Project<I, P> {
    pub fn new(iter: I, count: RowCount, head: Header, extractor: P) -> Project<I, P> {
        Project {
            iter,
            count,
            extractor,
            head,
        }
    }
}

impl<I, P> RelOps for Project<I, P>
where
    I: RelOps,
    P: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
{
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.count
    }

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        let extract = &mut self.extractor;
        if let Some(v) = self.iter.next()? {
            let row = extract(v.as_val_ref())?;
            return Ok(Some(RelValue::new(row, None)));
        }
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct JoinInner<Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub(crate) head: Header,
    pub(crate) count: RowCount,
    pub(crate) lhs: Lhs,
    pub(crate) rhs: Rhs,
    pub(crate) key_lhs: KeyLhs,
    pub(crate) key_rhs: KeyRhs,
    pub(crate) predicate: Pred,
    pub(crate) projection: Proj,
    map: HashMap<ProductValue, Vec<RelValue>>,
    filled: bool,
    left: Option<RelValue>,
}

impl<Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> JoinInner<Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub fn new(
        head: Header,
        lhs: Lhs,
        rhs: Rhs,
        key_lhs: KeyLhs,
        key_rhs: KeyRhs,
        predicate: Pred,
        projection: Proj,
    ) -> Self {
        Self {
            head,
            count: RowCount::unknown(),
            map: HashMap::new(),
            lhs,
            rhs,
            key_lhs,
            key_rhs,
            predicate,
            projection,
            filled: false,
            left: None,
        }
    }
}

impl<Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> RelOps for JoinInner<Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj>
where
    Lhs: RelOps,
    Rhs: RelOps,
    KeyLhs: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
    KeyRhs: FnMut(RelValueRef) -> Result<ProductValue, ErrorVm>,
    Pred: FnMut(RelValueRef, RelValueRef) -> Result<bool, ErrorVm>,
    Proj: FnMut(RelValue, RelValue) -> RelValue,
{
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.count
    }

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if !self.filled {
            self.map = HashMap::with_capacity(self.rhs.row_count().min);
            while let Some(v) = self.rhs.next()? {
                let k = (self.key_rhs)(v.as_val_ref())?;
                let values = self.map.entry(k).or_insert_with(|| Vec::with_capacity(1));
                values.push(v);
            }
            self.filled = true;
        }
        loop {
            let lhs = if let Some(left) = &self.left {
                left.clone()
            } else {
                match self.lhs.next()? {
                    None => return Ok(None),
                    Some(x) => {
                        self.left = Some(x.clone());
                        x
                    }
                }
            };

            let k = (self.key_lhs)(lhs.as_val_ref())?;
            if let Some(rvv) = self.map.get_mut(&k) {
                if let Some(rhs) = rvv.pop() {
                    if (self.predicate)(lhs.as_val_ref(), rhs.as_val_ref())? {
                        self.count.add_exact(1);
                        return Ok(Some((self.projection)(lhs, rhs)));
                    }
                }
            }
            self.left = None;
            continue;
        }
    }
}
