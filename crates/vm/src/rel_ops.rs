use crate::errors::ErrorVm;
use crate::relation::RelValue;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_sats::relation::{FieldExpr, Header, RowCount};
use spacetimedb_sats::AlgebraicValue;
use std::sync::Arc;

/// A trait for dealing with fallible iterators for the database.
pub trait RelOps<'a> {
    fn head(&self) -> &Arc<Header>;
    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }
    /// Advances the `iterator` and returns the next [RelValue].
    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm>;

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
        P: FnMut(&RelValue<'_>) -> Result<bool, ErrorVm>,
        Self: Sized,
    {
        Select::new(self, predicate)
    }

    /// Creates an `Iterator` which uses a closure that projects to a new [RelValue] extracted from the current.
    ///
    /// Given a [RelValue] the closure must return a subset of the current one.
    ///
    /// The [Header] is pre-checked that all the fields exist and return a error if any field is not found.
    ///
    /// Note:
    ///
    /// It is the equivalent of a `SELECT` clause on SQL.
    #[inline]
    fn project<'b, P>(self, after_head: &'b Arc<Header>, cols: &'b [FieldExpr], extractor: P) -> Project<'b, Self, P>
    where
        P: for<'c> FnMut(&[FieldExpr], RelValue<'c>) -> Result<RelValue<'c>, ErrorVm>,
        Self: Sized,
    {
        let count = self.row_count();
        Project::new(self, count, after_head, cols, extractor)
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
        head: Arc<Header>,
        key_lhs: KeyLhs,
        key_rhs: KeyRhs,
        predicate: Pred,
        project: Proj,
    ) -> JoinInner<'a, Self, Rhs, KeyLhs, KeyRhs, Pred, Proj>
    where
        Self: Sized,
        Pred: FnMut(&RelValue<'a>, &RelValue<'a>) -> bool,
        Proj: FnMut(RelValue<'a>, RelValue<'a>) -> RelValue<'a>,
        KeyLhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
        KeyRhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
        Rhs: RelOps<'a>,
    {
        JoinInner::new(head, self, with, key_lhs, key_rhs, predicate, project)
    }

    /// Collect all the rows in this relation into a `Vec<T>` given a function `RelValue<'a> -> T`.
    #[inline]
    fn collect_vec<T>(mut self, mut convert: impl FnMut(RelValue<'a>) -> T) -> Result<Vec<T>, ErrorVm>
    where
        Self: Sized,
    {
        let count = self.row_count();
        let estimate = count.max.unwrap_or(count.min);
        let mut result = Vec::with_capacity(estimate);

        while let Some(row) = self.next()? {
            result.push(convert(row));
        }

        Ok(result)
    }
}

impl<'a, I: RelOps<'a> + ?Sized> RelOps<'a> for Box<I> {
    fn head(&self) -> &Arc<Header> {
        (**self).head()
    }

    fn row_count(&self) -> RowCount {
        (**self).row_count()
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        (**self).next()
    }
}

/// `RelOps` iterator which never returns any rows.
///
/// Used to compile queries with unsatisfiable bounds, like `WHERE x < 5 AND x > 5`.
#[derive(Clone, Debug)]
pub struct EmptyRelOps {
    head: Arc<Header>,
}

impl EmptyRelOps {
    pub fn new(head: Arc<Header>) -> Self {
        Self { head }
    }
}

impl<'a> RelOps<'a> for EmptyRelOps {
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct Select<I, P> {
    pub(crate) iter: I,
    pub(crate) predicate: P,
}

impl<I, P> Select<I, P> {
    pub fn new(iter: I, predicate: P) -> Select<I, P> {
        Select { iter, predicate }
    }
}

impl<'a, I, P> RelOps<'a> for Select<I, P>
where
    I: RelOps<'a>,
    P: FnMut(&RelValue<'a>) -> Result<bool, ErrorVm>,
{
    fn head(&self) -> &Arc<Header> {
        self.iter.head()
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        let filter = &mut self.predicate;
        while let Some(v) = self.iter.next()? {
            if filter(&v)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct Project<'a, I, P> {
    pub(crate) head: &'a Arc<Header>,
    pub(crate) count: RowCount,
    pub(crate) cols: &'a [FieldExpr],
    pub(crate) iter: I,
    pub(crate) extractor: P,
}

impl<'a, I, P> Project<'a, I, P> {
    pub fn new(
        iter: I,
        count: RowCount,
        head: &'a Arc<Header>,
        cols: &'a [FieldExpr],
        extractor: P,
    ) -> Project<'a, I, P> {
        Project {
            iter,
            count,
            cols,
            extractor,
            head,
        }
    }
}

impl<'a, I, P> RelOps<'a> for Project<'_, I, P>
where
    I: RelOps<'a>,
    P: FnMut(&[FieldExpr], RelValue<'a>) -> Result<RelValue<'a>, ErrorVm>,
{
    fn head(&self) -> &Arc<Header> {
        self.head
    }

    fn row_count(&self) -> RowCount {
        self.count
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        let extract = &mut self.extractor;
        if let Some(v) = self.iter.next()? {
            return Ok(Some(extract(self.cols, v)?));
        }
        Ok(None)
    }
}

#[derive(Clone, Debug)]
pub struct JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub(crate) head: Arc<Header>,
    pub(crate) lhs: Lhs,
    pub(crate) rhs: Rhs,
    pub(crate) key_lhs: KeyLhs,
    pub(crate) key_rhs: KeyRhs,
    pub(crate) predicate: Pred,
    pub(crate) projection: Proj,
    map: HashMap<AlgebraicValue, Vec<RelValue<'a>>>,
    filled_rhs: bool,
    left: Option<RelValue<'a>>,
}

impl<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub fn new(
        head: Arc<Header>,
        lhs: Lhs,
        rhs: Rhs,
        key_lhs: KeyLhs,
        key_rhs: KeyRhs,
        predicate: Pred,
        projection: Proj,
    ) -> Self {
        Self {
            head,
            map: HashMap::new(),
            lhs,
            rhs,
            key_lhs,
            key_rhs,
            predicate,
            projection,
            filled_rhs: false,
            left: None,
        }
    }
}

impl<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> RelOps<'a> for JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj>
where
    Lhs: RelOps<'a>,
    Rhs: RelOps<'a>,
    KeyLhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
    KeyRhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
    Pred: FnMut(&RelValue<'a>, &RelValue<'a>) -> bool,
    Proj: FnMut(RelValue<'a>, RelValue<'a>) -> RelValue<'a>,
{
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        // Consume `Rhs`, building a map `KeyRhs => Rhs`.
        if !self.filled_rhs {
            self.map = HashMap::with_capacity(self.rhs.row_count().min);
            while let Some(row_rhs) = self.rhs.next()? {
                let key_rhs = (self.key_rhs)(&row_rhs);
                self.map.entry(key_rhs).or_default().push(row_rhs);
            }
            self.filled_rhs = true;
        }

        loop {
            // Consume a row in `Lhs` and project to `KeyLhs`.
            let lhs = match &self.left {
                Some(left) => left,
                None => match self.lhs.next()? {
                    Some(x) => self.left.insert(x),
                    None => return Ok(None),
                },
            };
            let k = (self.key_lhs)(lhs);

            // If we can relate `KeyLhs` and `KeyRhs`, we have candidate.
            // If that candidate still has rhs elements, test against the predicate and yield.
            if let Some(rvv) = self.map.get_mut(&k) {
                if let Some(rhs) = rvv.pop() {
                    if (self.predicate)(lhs, &rhs) {
                        return Ok(Some((self.projection)(lhs.clone(), rhs)));
                    }
                }
            }
            self.left = None;
            continue;
        }
    }
}
