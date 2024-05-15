use crate::relation::RelValue;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_sats::AlgebraicValue;

pub trait ApplyMut<Args> {
    type Ret;
    fn apply_mut(&mut self, args: Args) -> Self::Ret;
}

/// A trait for dealing with fallible iterators for the database.
pub trait RelOps<'a> {
    /// Advances the `iterator` and returns the next [RelValue].
    fn next(&mut self) -> Option<RelValue<'a>>;

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
        P: for<'b> ApplyMut<&'b RelValue<'a>, Ret = bool>,
        Self: Sized,
    {
        Select::new(self, predicate)
    }

    /// Creates an `Iterator` which uses a closure that projects to a new [RelValue] extracted from the current.
    ///
    /// Given a [RelValue] the closure must return a subset of the current one.
    ///
    /// Note:
    ///
    /// It is the equivalent of a `SELECT` clause on SQL.
    #[inline]
    fn project<P>(self, extractor: P) -> Project<Self, P>
    where
        P: for<'c> ApplyMut<RelValue<'a>, Ret = RelValue<'a>>,
        Self: Sized,
    {
        Project::new(self, extractor)
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
        key_lhs: KeyLhs,
        key_rhs: KeyRhs,
        predicate: Pred,
        project: Proj,
    ) -> JoinInner<'a, Self, Rhs, KeyLhs, KeyRhs, Pred, Proj>
    where
        Self: Sized,
        Pred: for<'b> ApplyMut<(&'b RelValue<'a>, &'b RelValue<'a>), Ret = bool>,
        Proj: ApplyMut<(RelValue<'a>, RelValue<'a>), Ret = RelValue<'a>>,
        KeyLhs: for<'b> ApplyMut<&'b RelValue<'a>, Ret = AlgebraicValue>,
        KeyRhs: for<'b> ApplyMut<&'b RelValue<'a>, Ret = AlgebraicValue>,
        Rhs: RelOps<'a>,
    {
        JoinInner::new(self, with, key_lhs, key_rhs, predicate, project)
    }

    /// Collect all the rows in this relation into a `Vec<T>` given a function `RelValue<'a> -> T`.
    #[inline]
    fn collect_vec<T>(mut self, mut convert: impl FnMut(RelValue<'a>) -> T) -> Vec<T>
    where
        Self: Sized,
    {
        let mut result = Vec::new();
        while let Some(row) = self.next() {
            result.push(convert(row));
        }
        result
    }
}

impl<'a, I: RelOps<'a> + ?Sized> RelOps<'a> for Box<I> {
    fn next(&mut self) -> Option<RelValue<'a>> {
        (**self).next()
    }
}

/// `RelOps` iterator which never returns any rows.
///
/// Used to compile queries with unsatisfiable bounds, like `WHERE x < 5 AND x > 5`.
#[derive(Clone, Debug)]
pub struct EmptyRelOps;

impl<'a> RelOps<'a> for EmptyRelOps {
    fn next(&mut self) -> Option<RelValue<'a>> {
        None
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
    P: for<'b> ApplyMut<&'b RelValue<'a>, Ret = bool>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        let filter = &mut self.predicate;
        while let Some(v) = self.iter.next() {
            if filter.apply_mut(&v) {
                return Some(v);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct Project<I, P> {
    pub(crate) iter: I,
    pub(crate) extractor: P,
}

impl<I, P> Project<I, P> {
    pub fn new(iter: I, extractor: P) -> Project<I, P> {
        Project { iter, extractor }
    }
}

impl<'a, I, P> RelOps<'a> for Project<I, P>
where
    I: RelOps<'a>,
    P: ApplyMut<RelValue<'a>, Ret = RelValue<'a>>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        self.iter.next().map(|v| self.extractor.apply_mut(v))
    }
}

#[derive(Clone, Debug)]
pub struct JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
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
    pub fn new(lhs: Lhs, rhs: Rhs, key_lhs: KeyLhs, key_rhs: KeyRhs, predicate: Pred, projection: Proj) -> Self {
        Self {
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
    Pred: for<'b> ApplyMut<(&'b RelValue<'a>, &'b RelValue<'a>), Ret = bool>,
    Proj: ApplyMut<(RelValue<'a>, RelValue<'a>), Ret = RelValue<'a>>,
    KeyLhs: for<'b> ApplyMut<&'b RelValue<'a>, Ret = AlgebraicValue>,
    KeyRhs: for<'b> ApplyMut<&'b RelValue<'a>, Ret = AlgebraicValue>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        // Consume `Rhs`, building a map `KeyRhs => Rhs`.
        if !self.filled_rhs {
            self.map = HashMap::new();
            while let Some(row_rhs) = self.rhs.next() {
                let key_rhs = self.key_rhs.apply_mut(&row_rhs);
                self.map.entry(key_rhs).or_default().push(row_rhs);
            }
            self.filled_rhs = true;
        }

        loop {
            // Consume a row in `Lhs` and project to `KeyLhs`.
            let lhs = match &self.left {
                Some(left) => left,
                None => self.left.insert(self.lhs.next()?),
            };
            let k = self.key_lhs.apply_mut(lhs);

            // If we can relate `KeyLhs` and `KeyRhs`, we have candidate.
            // If that candidate still has rhs elements, test against the predicate and yield.
            if let Some(rvv) = self.map.get_mut(&k) {
                if let Some(rhs) = rvv.pop() {
                    if self.predicate.apply_mut((lhs, &rhs)) {
                        return Some(self.projection.apply_mut((lhs.clone(), rhs)));
                    }
                }
            }
            self.left = None;
            continue;
        }
    }
}
