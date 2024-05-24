use crate::relation::RelValue;
use core::hash::{BuildHasher, Hash};
use smallvec::SmallVec;
use spacetimedb_data_structures::map::{HashMap, IntMap};
use spacetimedb_sats::relation::ColExpr;
use spacetimedb_sats::AlgebraicValue;

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
        P: FnMut(&RelValue<'_>) -> bool,
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
    fn project<'b, P>(self, cols: &'b [ColExpr], extractor: P) -> Project<'b, Self, P>
    where
        P: for<'c> FnMut(&[ColExpr], RelValue<'c>) -> RelValue<'c>,
        Self: Sized,
    {
        Project::new(self, cols, extractor)
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
        Pred: FnMut(&RelValue<'a>, &RelValue<'a>) -> bool,
        Proj: FnMut(RelValue<'a>, RelValue<'a>) -> RelValue<'a>,
        KeyLhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
        KeyRhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
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
    P: FnMut(&RelValue<'a>) -> bool,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        let filter = &mut self.predicate;
        while let Some(v) = self.iter.next() {
            if filter(&v) {
                return Some(v);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct Project<'a, I, P> {
    pub(crate) cols: &'a [ColExpr],
    pub(crate) iter: I,
    pub(crate) extractor: P,
}

impl<'a, I, P> Project<'a, I, P> {
    pub fn new(iter: I, cols: &'a [ColExpr], extractor: P) -> Project<'a, I, P> {
        Project { iter, cols, extractor }
    }
}

impl<'a, I, P> RelOps<'a> for Project<'_, I, P>
where
    I: RelOps<'a>,
    P: FnMut(&[ColExpr], RelValue<'a>) -> RelValue<'a>,
{
    fn next(&mut self) -> Option<RelValue<'a>> {
        self.iter.next().map(|v| (self.extractor)(self.cols, v))
    }
}

type IndexValues<'a> = SmallVec<[RelValue<'a>; 1]>;
#[derive(Clone, Debug)]
enum HashIndex<'a> {
    Bool(HashMap<bool, IndexValues<'a>>),
    U8(IntMap<u8, IndexValues<'a>>),
    I8(IntMap<i8, IndexValues<'a>>),
    U16(IntMap<u16, IndexValues<'a>>),
    I16(IntMap<i16, IndexValues<'a>>),
    U32(IntMap<u32, IndexValues<'a>>),
    I32(IntMap<i32, IndexValues<'a>>),
    U64(IntMap<u64, IndexValues<'a>>),
    I64(IntMap<i64, IndexValues<'a>>),
    U128(HashMap<u128, IndexValues<'a>>),
    I128(HashMap<i128, IndexValues<'a>>),
    String(HashMap<Box<str>, IndexValues<'a>>),
    AV(HashMap<AlgebraicValue, IndexValues<'a>>),
    Empty,
}

#[derive(Clone, Debug)]
pub struct JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub(crate) lhs: Lhs,
    pub(crate) rhs: Rhs,
    pub(crate) key_lhs: KeyLhs,
    pub(crate) key_rhs: KeyRhs,
    pub(crate) predicate: Pred,
    pub(crate) projection: Proj,
    map: HashIndex<'a>,
    filled_rhs: bool,
    left: Option<RelValue<'a>>,
}

impl<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> JoinInner<'a, Lhs, Rhs, KeyLhs, KeyRhs, Pred, Proj> {
    pub fn new(lhs: Lhs, rhs: Rhs, key_lhs: KeyLhs, key_rhs: KeyRhs, predicate: Pred, projection: Proj) -> Self {
        Self {
            map: HashIndex::Empty,
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

    fn build_index<K: Hash + Eq, S: Default + BuildHasher>(
        &mut self,
        first_key: K,
        first_row: RelValue<'a>,
        mut ext: impl FnMut(AlgebraicValue) -> Result<K, AlgebraicValue>,
    ) -> HashMap<K, IndexValues<'a>, S>
    where
        Rhs: RelOps<'a>,
        KeyRhs: FnMut(&RelValue<'a>) -> AlgebraicValue,
    {
        let mut map: HashMap<K, IndexValues<'a>, S> = <_>::default();
        map.entry(first_key).or_default().push(first_row);
        while let Some(row_rhs) = self.rhs.next() {
            let key_rhs = ext((self.key_rhs)(&row_rhs)).unwrap();
            map.entry(key_rhs).or_default().push(row_rhs);
        }
        map
    }
}

fn relate_loop<'a, K: Hash + Eq, S: BuildHasher>(
    left: &mut Option<RelValue<'a>>,
    lhs: &mut impl RelOps<'a>,
    key_lhs: &mut impl FnMut(&RelValue<'a>) -> AlgebraicValue,
    pred: &mut impl FnMut(&RelValue<'a>, &RelValue<'a>) -> bool,
    proj: &mut impl FnMut(RelValue<'a>, RelValue<'a>) -> RelValue<'a>,
    map: &mut HashMap<K, IndexValues<'a>, S>,
    mut ext: impl FnMut(AlgebraicValue) -> Result<K, AlgebraicValue>,
) -> Option<RelValue<'a>> {
    loop {
        // Consume a row in `Lhs` and project to `KeyLhs`.
        let lhs = match &left {
            Some(left) => left,
            None => left.insert(lhs.next()?),
        };
        let k = ext((key_lhs)(lhs)).unwrap();

        // If we can relate `KeyLhs` and `KeyRhs`, we have candidate.
        // If that candidate still has rhs elements, test against the predicate and yield.
        if let Some(rvv) = map.get_mut(&k) {
            if let Some(rhs) = rvv.pop() {
                if (pred)(lhs, &rhs) {
                    return Some((proj)(lhs.clone(), rhs));
                }
            }
        }
        *left = None;
        continue;
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
    fn next(&mut self) -> Option<RelValue<'a>> {
        use HashIndex::*;

        // Consume `Rhs`, building a map `KeyRhs => Rhs`.
        if !self.filled_rhs {
            if let Some(row_rhs) = self.rhs.next() {
                let key_rhs = (self.key_rhs)(&row_rhs);
                self.map = match key_rhs {
                    AlgebraicValue::Bool(k) => Bool(self.build_index(k, row_rhs, AlgebraicValue::into_bool)),
                    AlgebraicValue::I8(k) => I8(self.build_index(k, row_rhs, AlgebraicValue::into_i8)),
                    AlgebraicValue::U8(k) => U8(self.build_index(k, row_rhs, AlgebraicValue::into_u8)),
                    AlgebraicValue::I16(k) => I16(self.build_index(k, row_rhs, AlgebraicValue::into_i16)),
                    AlgebraicValue::U16(k) => U16(self.build_index(k, row_rhs, AlgebraicValue::into_u16)),
                    AlgebraicValue::I32(k) => I32(self.build_index(k, row_rhs, AlgebraicValue::into_i32)),
                    AlgebraicValue::U32(k) => U32(self.build_index(k, row_rhs, AlgebraicValue::into_u32)),
                    AlgebraicValue::I64(k) => I64(self.build_index(k, row_rhs, AlgebraicValue::into_i64)),
                    AlgebraicValue::U64(k) => U64(self.build_index(k, row_rhs, AlgebraicValue::into_u64)),
                    AlgebraicValue::I128(k) => I128(self.build_index(k.0, row_rhs, |k| k.into_i128().map(|k| k.0))),
                    AlgebraicValue::U128(k) => U128(self.build_index(k.0, row_rhs, |k| k.into_u128().map(|k| k.0))),
                    AlgebraicValue::String(k) => String(self.build_index(k, row_rhs, AlgebraicValue::into_string)),
                    k => AV(self.build_index(k, row_rhs, Ok)),
                };
            } else {
                self.map = Empty;
            }
            self.filled_rhs = true;
        }

        let left = &mut self.left;
        let lhs = &mut self.lhs;
        let klhs = &mut self.key_lhs;
        let pred = &mut self.predicate;
        let proj = &mut self.projection;
        match &mut self.map {
            Bool(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_bool),
            U8(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_u8),
            I8(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_i8),
            U16(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_u16),
            I16(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_i16),
            U32(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_u32),
            I32(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_i32),
            U64(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_u64),
            I64(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_i64),
            U128(map) => relate_loop(left, lhs, klhs, pred, proj, map, |k| k.into_u128().map(|k| k.0)),
            I128(map) => relate_loop(left, lhs, klhs, pred, proj, map, |k| k.into_i128().map(|k| k.0)),
            String(map) => relate_loop(left, lhs, klhs, pred, proj, map, AlgebraicValue::into_string),
            AV(map) => relate_loop(left, lhs, klhs, pred, proj, map, Ok),
            Empty => None,
        }
    }
}
