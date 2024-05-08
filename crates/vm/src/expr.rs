use crate::errors::{ErrorKind, ErrorLang};
use crate::operator::{OpCmp, OpLogic, OpQuery};
use crate::relation::{MemTable, RelValue};
use arrayvec::ArrayVec;
use derive_more::From;
use smallvec::{smallvec, SmallVec};
use spacetimedb_data_structures::map::{HashSet, IntMap};
use spacetimedb_lib::{AlgebraicType, Identity};
use spacetimedb_primitives::*;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::{TableDef, TableSchema};
use spacetimedb_sats::db::error::{AuthError, RelationError};
use spacetimedb_sats::relation::{ColExpr, DbTable, FieldName, Header};
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::ProductValue;
use std::cmp::Reverse;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, VecDeque};
use std::ops::Bound;
use std::sync::Arc;
use std::{fmt, iter, mem};

/// Trait for checking if the `caller` have access to `Self`
pub trait AuthAccess {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From)]
pub enum FieldExpr {
    Name(FieldName),
    Value(AlgebraicValue),
}

impl FieldExpr {
    pub fn strip_table(self) -> ColExpr {
        match self {
            Self::Name(field) => ColExpr::Col(field.col),
            Self::Value(value) => ColExpr::Value(value),
        }
    }

    pub fn name_to_col(self, head: &Header) -> Result<ColExpr, RelationError> {
        match self {
            Self::Value(val) => Ok(ColExpr::Value(val)),
            Self::Name(field) => head.column_pos_or_err(field).map(ColExpr::Col),
        }
    }
}

impl fmt::Display for FieldExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldExpr::Name(x) => write!(f, "{x}"),
            FieldExpr::Value(x) => write!(f, "{}", x.to_satn()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From)]
pub enum FieldOp {
    #[from]
    Field(FieldExpr),
    Cmp {
        op: OpQuery,
        lhs: Box<FieldOp>,
        rhs: Box<FieldOp>,
    },
}

type FieldOpFlat = SmallVec<[FieldOp; 1]>;

impl FieldOp {
    pub fn new(op: OpQuery, lhs: Self, rhs: Self) -> Self {
        Self::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn cmp(field: impl Into<FieldName>, op: OpCmp, value: impl Into<AlgebraicValue>) -> Self {
        Self::new(
            OpQuery::Cmp(op),
            Self::Field(FieldExpr::Name(field.into())),
            Self::Field(FieldExpr::Value(value.into())),
        )
    }

    pub fn names_to_cols(self, head: &Header) -> Result<ColumnOp, RelationError> {
        match self {
            Self::Field(field) => field.name_to_col(head).map(ColumnOp::Col),
            Self::Cmp { op, lhs, rhs } => {
                let lhs = lhs.names_to_cols(head)?;
                let rhs = rhs.names_to_cols(head)?;
                Ok(ColumnOp::new(op, lhs, rhs))
            }
        }
    }

    /// Flattens a nested conjunction of AND expressions.
    ///
    /// For example, `a = 1 AND b = 2 AND c = 3` becomes `[a = 1, b = 2, c = 3]`.
    ///
    /// This helps with splitting the kinds of `queries`,
    /// that *could* be answered by a `index`,
    /// from the ones that need to be executed with a `scan`.
    pub fn flatten_ands(self) -> FieldOpFlat {
        fn fill_vec(buf: &mut FieldOpFlat, op: FieldOp) {
            match op {
                FieldOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs,
                    rhs,
                } => {
                    fill_vec(buf, *lhs);
                    fill_vec(buf, *rhs);
                }
                op => buf.push(op),
            }
        }
        let mut buf = SmallVec::new();
        fill_vec(&mut buf, self);
        buf
    }
}

impl fmt::Display for FieldOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field(x) => {
                write!(f, "{}", x)
            }
            Self::Cmp { op, lhs, rhs } => {
                write!(f, "{} {} {}", lhs, op, rhs)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From)]
pub enum ColumnOp {
    #[from]
    Col(ColExpr),
    Cmp {
        op: OpQuery,
        lhs: Box<ColumnOp>,
        rhs: Box<ColumnOp>,
    },
}

type ColumnOpRefFlat<'a> = SmallVec<[&'a ColumnOp; 1]>;

impl ColumnOp {
    pub fn new(op: OpQuery, lhs: Self, rhs: Self) -> Self {
        Self::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn cmp(col: impl Into<ColId>, op: OpCmp, value: impl Into<AlgebraicValue>) -> Self {
        Self::new(
            OpQuery::Cmp(op),
            Self::Col(ColExpr::Col(col.into())),
            Self::Col(ColExpr::Value(value.into())),
        )
    }

    /// Returns a new op where `lhs` and `rhs` are logically AND-ed together.
    fn and(lhs: Self, rhs: Self) -> Self {
        Self::new(OpQuery::Logic(OpLogic::And), lhs, rhs)
    }

    /// Returns an op where `col_i op value_i` are all `AND`ed together.
    fn and_cmp(op: OpCmp, cols: &ColList, value: AlgebraicValue) -> Self {
        let eq = |(col, value): (ColId, _)| Self::cmp(col, op, value);

        // For singleton constraints, the `value` must be used directly.
        if cols.is_singleton() {
            return eq((cols.head(), value));
        }

        // Otherwise, pair column ids and product fields together.
        cols.iter()
            .zip(value.into_product().unwrap())
            .map(eq)
            .reduce(Self::and)
            .unwrap()
    }

    /// Returns an op where `cols` must be within bounds.
    /// This handles both the case of single-col bounds and multi-col bounds.
    fn from_op_col_bounds(cols: &ColList, bounds: (Bound<AlgebraicValue>, Bound<AlgebraicValue>)) -> Self {
        let (cmp, value) = match bounds {
            // Equality; field <= value && field >= value <=> field = value
            (Bound::Included(a), Bound::Included(b)) if a == b => (OpCmp::Eq, a),
            // Inclusive lower bound => field >= value
            (Bound::Included(value), Bound::Unbounded) => (OpCmp::GtEq, value),
            // Exclusive lower bound => field > value
            (Bound::Excluded(value), Bound::Unbounded) => (OpCmp::Gt, value),
            // Inclusive upper bound => field <= value
            (Bound::Unbounded, Bound::Included(value)) => (OpCmp::LtEq, value),
            // Exclusive upper bound => field < value
            (Bound::Unbounded, Bound::Excluded(value)) => (OpCmp::Lt, value),
            (Bound::Unbounded, Bound::Unbounded) => unreachable!(),
            (lower_bound, upper_bound) => {
                let lhs = Self::from_op_col_bounds(cols, (lower_bound, Bound::Unbounded));
                let rhs = Self::from_op_col_bounds(cols, (Bound::Unbounded, upper_bound));
                return ColumnOp::and(lhs, rhs);
            }
        };
        ColumnOp::and_cmp(cmp, cols, value)
    }

    fn reduce(&self, row: &RelValue<'_>, value: &Self) -> AlgebraicValue {
        match value {
            Self::Col(field) => row.get(field.borrowed()).into_owned(),
            Self::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs).into(),
        }
    }

    fn reduce_bool(&self, row: &RelValue<'_>, value: &Self) -> bool {
        match value {
            Self::Col(field) => *row.get(field.borrowed()).as_bool().unwrap(),
            Self::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs),
        }
    }

    fn compare_bin_op(&self, row: &RelValue<'_>, op: OpQuery, lhs: &Self, rhs: &Self) -> bool {
        match op {
            OpQuery::Cmp(op) => {
                let lhs = self.reduce(row, lhs);
                let rhs = self.reduce(row, rhs);
                match op {
                    OpCmp::Eq => lhs == rhs,
                    OpCmp::NotEq => lhs != rhs,
                    OpCmp::Lt => lhs < rhs,
                    OpCmp::LtEq => lhs <= rhs,
                    OpCmp::Gt => lhs > rhs,
                    OpCmp::GtEq => lhs >= rhs,
                }
            }
            OpQuery::Logic(op) => {
                let lhs = self.reduce_bool(row, lhs);
                let rhs = self.reduce_bool(row, rhs);

                match op {
                    OpLogic::And => lhs && rhs,
                    OpLogic::Or => lhs || rhs,
                }
            }
        }
    }

    pub fn compare(&self, row: &RelValue<'_>) -> bool {
        match self {
            Self::Col(field) => {
                let lhs = row.get(field.borrowed());
                *lhs.as_bool().unwrap()
            }
            Self::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs),
        }
    }

    /// Flattens a nested conjunction of AND expressions.
    ///
    /// For example, `a = 1 AND b = 2 AND c = 3` becomes `[a = 1, b = 2, c = 3]`.
    ///
    /// This helps with splitting the kinds of `queries`,
    /// that *could* be answered by a `index`,
    /// from the ones that need to be executed with a `scan`.
    pub fn flatten_ands_ref(&self) -> ColumnOpRefFlat<'_> {
        fn fill_vec<'a>(buf: &mut ColumnOpRefFlat<'a>, op: &'a ColumnOp) {
            match op {
                ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs,
                    rhs,
                } => {
                    fill_vec(buf, lhs);
                    fill_vec(buf, rhs);
                }
                op => buf.push(op),
            }
        }
        let mut buf = SmallVec::new();
        fill_vec(&mut buf, self);
        buf
    }
}

impl fmt::Display for ColumnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnOp::Col(x) => {
                write!(f, "{}", x)
            }
            ColumnOp::Cmp { op, lhs, rhs } => {
                write!(f, "{} {} {}", lhs, op, rhs)
            }
        }
    }
}

impl From<ColId> for ColumnOp {
    fn from(value: ColId) -> Self {
        ColumnOp::Col(value.into())
    }
}
impl From<AlgebraicValue> for ColumnOp {
    fn from(value: AlgebraicValue) -> Self {
        Self::Col(value.into())
    }
}

impl From<Query> for Option<ColumnOp> {
    fn from(value: Query) -> Self {
        match value {
            Query::IndexScan(op) => Some(ColumnOp::from_op_col_bounds(&op.columns, op.bounds)),
            Query::Select(op) => Some(op),
            _ => None,
        }
    }
}

/// An identifier for a data source (i.e. a table) in a query plan.
///
/// When compiling a query plan, rather than embedding the inputs in the plan,
/// we annotate each input with a `SourceId`, and the compiled plan refers to its inputs by id.
/// This allows the plan to be re-used with distinct inputs,
/// assuming the inputs obey the same schema.
///
/// Note that re-using a query plan is only a good idea
/// if the new inputs are similar to those used for compilation
/// in terms of cardinality and distribution.
#[derive(Debug, Copy, Clone, PartialEq, Eq, From, Hash)]
pub struct SourceId(pub usize);

/// Types that relate [`SourceId`]s to their in-memory tables.
///
/// Rather than embedding tables in query plans, we store a [`SourceExpr::InMemory`],
/// which contains the information necessary for optimization along with a `SourceId`.
/// Query execution then executes the plan, and when it encounters a `SourceExpr::InMemory`,
/// retrieves the `Self::Source` table from the corresponding provider.
/// This allows query plans to be re-used, though each execution might require a new provider.
///
/// An in-memory table `Self::Source` is a type capable of producing [`RelValue<'a>`]s.
/// The general form of this is `Iterator<Item = RelValue<'a>>`.
/// Depending on the situation, this could be e.g.,
/// - [`MemTable`], producing [`RelValue::Projection`],
/// - `&'a [ProductValue]` producing [`RelValue::ProjRef`].
pub trait SourceProvider<'a> {
    /// The type of in-memory tables that this provider uses.
    type Source: 'a + IntoIterator<Item = RelValue<'a>>;

    /// Retrieve the `Self::Source` associated with `id`, if any.
    ///
    /// Taking the same `id` a second time may or may not yield the same source.
    /// Callers should not assume that a generic provider will yield it more than once.
    /// This means that a query plan may not include multiple references to the same [`SourceId`].
    ///
    /// Implementations are also not obligated to inspect `id`, e.g., if there's only one option.
    fn take_source(&mut self, id: SourceId) -> Option<Self::Source>;
}

impl<'a, I: 'a + IntoIterator<Item = RelValue<'a>>, F: FnMut(SourceId) -> Option<I>> SourceProvider<'a> for F {
    type Source = I;
    fn take_source(&mut self, id: SourceId) -> Option<Self::Source> {
        self(id)
    }
}

impl<'a, I: 'a + IntoIterator<Item = RelValue<'a>>> SourceProvider<'a> for Option<I> {
    type Source = I;
    fn take_source(&mut self, _: SourceId) -> Option<Self::Source> {
        self.take()
    }
}

pub struct NoInMemUsed;

impl<'a> SourceProvider<'a> for NoInMemUsed {
    type Source = iter::Empty<RelValue<'a>>;
    fn take_source(&mut self, _: SourceId) -> Option<Self::Source> {
        None
    }
}

/// A [`SourceProvider`] backed by an `ArrayVec`.
///
/// Internally, the `SourceSet` stores an `Option<T>` for each planned [`SourceId`]
/// which are [`Option::take`]n out of the set.
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(transparent)]
pub struct SourceSet<T, const N: usize>(
    // Benchmarks showed an improvement in performance
    // on incr-select by ~10% by not using `Vec<Option<T>>`.
    ArrayVec<Option<T>, N>,
);

impl<'a, T: 'a + IntoIterator<Item = RelValue<'a>>, const N: usize> SourceProvider<'a> for SourceSet<T, N> {
    type Source = T;
    fn take_source(&mut self, id: SourceId) -> Option<T> {
        self.take(id)
    }
}

impl<T, const N: usize> From<[T; N]> for SourceSet<T, N> {
    #[inline]
    fn from(sources: [T; N]) -> Self {
        Self(sources.map(Some).into())
    }
}

impl<T, const N: usize> SourceSet<T, N> {
    /// Returns an empty source set.
    pub fn empty() -> Self {
        Self(ArrayVec::new())
    }

    /// Get a fresh `SourceId` which can be used as the id for a new entry.
    fn next_id(&self) -> SourceId {
        SourceId(self.0.len())
    }

    /// Insert an entry into this `SourceSet` so it can be used in a query plan,
    /// and return a [`SourceId`] which can be embedded in that plan.
    pub fn add(&mut self, table: T) -> SourceId {
        let source_id = self.next_id();
        self.0.push(Some(table));
        source_id
    }

    /// Extract the entry referred to by `id` from this `SourceSet`,
    /// leaving a "gap" in its place.
    ///
    /// Subsequent calls to `take` on the same `id` will return `None`.
    pub fn take(&mut self, id: SourceId) -> Option<T> {
        self.0.get_mut(id.0).map(mem::take).unwrap_or_default()
    }

    /// Returns the number of slots for [`MemTable`]s in this set.
    ///
    /// Calling `self.take_mem_table(...)` or `self.take_table(...)` won't affect this number.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether this set has any slots for [`MemTable`]s.
    ///
    /// Calling `self.take_mem_table(...)` or `self.take_table(...)` won't affect whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T, const N: usize> std::ops::Index<SourceId> for SourceSet<T, N> {
    type Output = Option<T>;

    fn index(&self, idx: SourceId) -> &Self::Output {
        &self.0[idx.0]
    }
}

impl<T, const N: usize> std::ops::IndexMut<SourceId> for SourceSet<T, N> {
    fn index_mut(&mut self, idx: SourceId) -> &mut Self::Output {
        &mut self.0[idx.0]
    }
}

impl<const N: usize> SourceSet<Vec<ProductValue>, N> {
    /// Insert a [`MemTable`] into this `SourceSet` so it can be used in a query plan,
    /// and return a [`SourceExpr`] which can be embedded in that plan.
    pub fn add_mem_table(&mut self, table: MemTable) -> SourceExpr {
        let id = self.add(table.data);
        SourceExpr::from_mem_table(table.head, table.table_access, id)
    }
}

/// A reference to a table within a query plan,
/// used as the source for selections, scans, filters and joins.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum SourceExpr {
    /// A plan for a "virtual" or projected table.
    ///
    /// The actual in-memory table, e.g., [`MemTable`] or `&'a [ProductValue]`
    /// is not stored within the query plan;
    /// rather, the `source_id` is an index which corresponds to the table in e.g., a [`SourceSet`].
    ///
    /// This allows query plans to be reused by supplying e.g., a new [`SourceSet`].
    InMemory {
        source_id: SourceId,
        header: Arc<Header>,
        table_type: StTableType,
        table_access: StAccess,
    },
    /// A plan for a database table. Because [`DbTable`] is small and efficiently cloneable,
    /// no indirection into a [`SourceSet`] is required.
    DbTable(DbTable),
}

impl SourceExpr {
    /// If `self` refers to a [`MemTable`], returns the [`SourceId`] for its location in the plan's [`SourceSet`].
    ///
    /// Returns `None` if `self` refers to a [`DbTable`], as [`DbTable`]s are stored directly in the `SourceExpr`,
    /// rather than indirected through the [`SourceSet`].
    pub fn source_id(&self) -> Option<SourceId> {
        if let SourceExpr::InMemory { source_id, .. } = self {
            Some(*source_id)
        } else {
            None
        }
    }

    pub fn table_name(&self) -> &str {
        &self.head().table_name
    }

    pub fn table_type(&self) -> StTableType {
        match self {
            SourceExpr::InMemory { table_type, .. } => *table_type,
            SourceExpr::DbTable(db_table) => db_table.table_type,
        }
    }

    pub fn table_access(&self) -> StAccess {
        match self {
            SourceExpr::InMemory { table_access, .. } => *table_access,
            SourceExpr::DbTable(db_table) => db_table.table_access,
        }
    }

    pub fn head(&self) -> &Arc<Header> {
        match self {
            SourceExpr::InMemory { header, .. } => header,
            SourceExpr::DbTable(db_table) => &db_table.head,
        }
    }

    pub fn is_mem_table(&self) -> bool {
        matches!(self, SourceExpr::InMemory { .. })
    }

    pub fn is_db_table(&self) -> bool {
        matches!(self, SourceExpr::DbTable(_))
    }

    pub fn from_mem_table(header: Arc<Header>, table_access: StAccess, id: SourceId) -> Self {
        SourceExpr::InMemory {
            source_id: id,
            header,
            table_type: StTableType::User,
            table_access,
        }
    }

    pub fn table_id(&self) -> Option<TableId> {
        if let SourceExpr::DbTable(db_table) = self {
            Some(db_table.table_id)
        } else {
            None
        }
    }

    /// If `self` refers to a [`DbTable`], get a reference to it.
    ///
    /// Returns `None` if `self` refers to a [`MemTable`].
    /// In that case, retrieving the [`MemTable`] requires inspecting the plan's corresponding [`SourceSet`]
    /// via [`SourceSet::take_mem_table`] or [`SourceSet::take_table`].
    pub fn get_db_table(&self) -> Option<&DbTable> {
        if let SourceExpr::DbTable(db_table) = self {
            Some(db_table)
        } else {
            None
        }
    }
}

impl From<&TableSchema> for SourceExpr {
    fn from(value: &TableSchema) -> Self {
        SourceExpr::DbTable(value.into())
    }
}

/// A descriptor for an index semi join operation.
///
/// The semantics are those of a semijoin with rows from the index or the probe side being returned.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IndexJoin {
    pub probe_side: QueryExpr,
    pub probe_col: ColId,
    pub index_side: SourceExpr,
    pub index_select: Option<ColumnOp>,
    pub index_col: ColId,
    /// If true, returns rows from the `index_side`.
    /// Otherwise, returns rows from the `probe_side`.
    pub return_index_rows: bool,
}

impl From<IndexJoin> for QueryExpr {
    fn from(join: IndexJoin) -> Self {
        let source: SourceExpr = if join.return_index_rows {
            join.index_side.clone()
        } else {
            join.probe_side.source.clone()
        };
        QueryExpr {
            source,
            query: vec![Query::IndexJoin(join)],
        }
    }
}

impl IndexJoin {
    // Reorder the index and probe sides of an index join.
    // This is necessary if the indexed table has been replaced by a delta table.
    // A delta table is a virtual table consisting of changes or updates to a physical table.
    pub fn reorder(self, row_count: impl Fn(TableId, &str) -> i64) -> Self {
        // The probe table must be a physical table.
        if self.probe_side.source.is_mem_table() {
            return self;
        }
        // It must have an index defined on the join field.
        if !self
            .probe_side
            .source
            .head()
            .has_constraint(self.probe_col, Constraints::indexed())
        {
            return self;
        }
        // It must be a linear pipeline of selections.
        if !self
            .probe_side
            .query
            .iter()
            .all(|op| matches!(op, Query::Select(_)) || matches!(op, Query::IndexScan(_)))
        {
            return self;
        }
        match self.index_side.get_db_table() {
            // If the size of the indexed table is sufficiently large,
            // do not reorder.
            //
            // TODO: This determination is quite arbitrary.
            // Ultimately we should be using cardinality estimation.
            Some(DbTable { head, table_id, .. }) if row_count(*table_id, &head.table_name) > 500 => self,
            // If this is a delta table, we must reorder.
            // If this is a sufficiently small physical table, we should reorder.
            _ => {
                // Merge all selections from the original probe side into a single predicate.
                // This includes an index scan if present.
                let predicate = self
                    .probe_side
                    .query
                    .into_iter()
                    .filter_map(<Query as Into<Option<ColumnOp>>>::into)
                    .reduce(ColumnOp::and);
                // Push any selections on the index side to the probe side.
                let probe_side = if let Some(predicate) = self.index_select {
                    QueryExpr {
                        source: self.index_side,
                        query: vec![predicate.into()],
                    }
                } else {
                    self.index_side.into()
                };
                IndexJoin {
                    // The new probe side consists of the updated rows.
                    // Plus any selections from the original index probe.
                    probe_side,
                    // The new probe field is the previous index field.
                    probe_col: self.index_col,
                    // The original probe table is now the table that is being probed.
                    index_side: self.probe_side.source,
                    // Any selections from the original probe side are pulled above the index lookup.
                    index_select: predicate,
                    // The new index field is the previous probe field.
                    index_col: self.probe_col,
                    // Because we have swapped the original index and probe sides of the join,
                    // the new index join needs to return rows from the opposite side.
                    return_index_rows: !self.return_index_rows,
                }
            }
        }
    }

    // Convert this index join to an inner join, followed by a projection.
    // This is needed for incremental evaluation of index joins.
    // In particular when there are updates to both the left and right tables.
    // In other words, when an index join has two delta tables.
    pub fn to_inner_join(self) -> QueryExpr {
        if self.return_index_rows {
            let (col_lhs, col_rhs) = (self.index_col, self.probe_col);
            let rhs = self.probe_side;

            let source = self.index_side;
            let inner_join = Query::JoinInner(JoinExpr::new(rhs, col_lhs, col_rhs, None));
            let query = if let Some(predicate) = self.index_select {
                vec![predicate.into(), inner_join]
            } else {
                vec![inner_join]
            };
            QueryExpr { source, query }
        } else {
            let (col_lhs, col_rhs) = (self.probe_col, self.index_col);
            let mut rhs: QueryExpr = self.index_side.into();

            if let Some(predicate) = self.index_select {
                rhs.query.push(predicate.into());
            }

            let source = self.probe_side.source;
            let inner_join = Query::JoinInner(JoinExpr::new(rhs, col_lhs, col_rhs, None));
            let query = vec![inner_join];
            QueryExpr { source, query }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct JoinExpr {
    pub rhs: QueryExpr,
    pub col_lhs: ColId,
    pub col_rhs: ColId,
    /// If None, this is a left semi-join, returning rows only from the source table,
    /// using the `rhs` as a filter.
    ///
    /// If Some(_), this is an inner join, returning the concatenation of the matching rows.
    pub inner: Option<Arc<Header>>,
}

impl JoinExpr {
    pub fn new(rhs: QueryExpr, col_lhs: ColId, col_rhs: ColId, inner: Option<Arc<Header>>) -> Self {
        Self {
            rhs,
            col_lhs,
            col_rhs,
            inner,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DbType {
    Table,
    Index,
    Sequence,
    Constraint,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Crud {
    Query,
    Insert,
    Update,
    Delete,
    Create(DbType),
    Drop(DbType),
    Config,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CrudExpr {
    Query(QueryExpr),
    Insert {
        table: DbTable,
        rows: Vec<ProductValue>,
    },
    Update {
        delete: QueryExpr,
        assignments: IntMap<ColId, ColExpr>,
    },
    Delete {
        query: QueryExpr,
    },
    CreateTable {
        table: Box<TableDef>,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
    },
    SetVar {
        name: String,
        value: AlgebraicValue,
    },
    ReadVar {
        name: String,
    },
}

impl CrudExpr {
    pub fn optimize(self, row_count: &impl Fn(TableId, &str) -> i64) -> Self {
        match self {
            CrudExpr::Query(x) => CrudExpr::Query(x.optimize(row_count)),
            _ => self,
        }
    }

    pub fn is_reads<'a>(exprs: impl IntoIterator<Item = &'a CrudExpr>) -> bool {
        exprs
            .into_iter()
            .all(|expr| matches!(expr, CrudExpr::Query(_) | CrudExpr::ReadVar { .. }))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IndexScan {
    pub table: DbTable,
    pub columns: ColList,
    pub bounds: (Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

impl IndexScan {
    /// Returns whether this is a point range.
    pub fn is_point(&self) -> bool {
        match &self.bounds {
            (Bound::Included(lower), Bound::Included(upper)) => lower == upper,
            _ => false,
        }
    }
}

/// A projection operation in a query.
#[derive(Debug, Clone, Eq, PartialEq, From, Hash)]
pub struct ProjectExpr {
    pub cols: Vec<ColExpr>,
    // The table id for a qualified wildcard project, if any.
    // If present, further optimizations are possible.
    pub wildcard_table: Option<TableId>,
    pub header_after: Arc<Header>,
}

// An individual operation in a query.
#[derive(Debug, Clone, Eq, PartialEq, From, Hash)]
pub enum Query {
    // Fetching rows via an index.
    IndexScan(IndexScan),
    // Joining rows via an index.
    // Equivalent to Index Nested Loop Join.
    IndexJoin(IndexJoin),
    // A filter over an intermediate relation.
    // In particular it does not utilize any indexes.
    // If it could it would have already been transformed into an IndexScan.
    Select(ColumnOp),
    // Projects a set of columns.
    Project(ProjectExpr),
    // A join of two relations (base or intermediate) based on equality.
    // Equivalent to a Nested Loop Join.
    // Its operands my use indexes but the join itself does not.
    JoinInner(JoinExpr),
}

impl Query {
    /// Iterate over all [`SourceExpr`]s involved in the [`Query`].
    ///
    /// Sources are yielded from left to right. Duplicates are not filtered out.
    pub fn sources(&self) -> QuerySources {
        match self {
            Self::Select(..) | Self::Project(..) => QuerySources::None,
            Self::IndexScan(scan) => QuerySources::One(Some(SourceExpr::DbTable(scan.table.clone()))),
            Self::IndexJoin(join) => QuerySources::Expr(join.probe_side.sources()),
            Self::JoinInner(join) => QuerySources::Expr(join.rhs.sources()),
        }
    }
}

// IndexArgument represents an equality or range predicate that can be answered
// using an index.
#[derive(Debug, PartialEq, Clone)]
enum IndexArgument<'a> {
    Eq {
        columns: &'a ColList,
        value: AlgebraicValue,
    },
    LowerBound {
        columns: &'a ColList,
        value: AlgebraicValue,
        inclusive: bool,
    },
    UpperBound {
        columns: &'a ColList,
        value: AlgebraicValue,
        inclusive: bool,
    },
}

#[derive(Debug, PartialEq, Clone)]
enum IndexColumnOp<'a> {
    Index(IndexArgument<'a>),
    Scan(&'a ColumnOp),
}

fn make_index_arg(cmp: OpCmp, columns: &ColList, value: AlgebraicValue) -> IndexColumnOp<'_> {
    let arg = match cmp {
        OpCmp::Eq => IndexArgument::Eq { columns, value },
        OpCmp::NotEq => unreachable!("No IndexArgument for NotEq, caller should've filtered out"),
        // a < 5 => exclusive upper bound
        OpCmp::Lt => IndexArgument::UpperBound {
            columns,
            value,
            inclusive: false,
        },
        // a > 5 => exclusive lower bound
        OpCmp::Gt => IndexArgument::LowerBound {
            columns,
            value,
            inclusive: false,
        },
        // a <= 5 => inclusive upper bound
        OpCmp::LtEq => IndexArgument::UpperBound {
            columns,
            value,
            inclusive: true,
        },
        // a >= 5 => inclusive lower bound
        OpCmp::GtEq => IndexArgument::LowerBound {
            columns,
            value,
            inclusive: true,
        },
    };
    IndexColumnOp::Index(arg)
}

#[derive(Debug)]
struct ColValue<'a> {
    parent: &'a ColumnOp,
    col: ColId,
    cmp: OpCmp,
    value: &'a AlgebraicValue,
}

impl<'a> ColValue<'a> {
    pub fn new(parent: &'a ColumnOp, col: ColId, cmp: OpCmp, value: &'a AlgebraicValue) -> Self {
        Self {
            parent,
            col,
            cmp,
            value,
        }
    }
}

type IndexColumnOpSink<'a> = SmallVec<[IndexColumnOp<'a>; 1]>;
type ColsIndexed = HashSet<(ColId, OpCmp)>;

/// Pick the best indices that can serve the constraints in `cols`
/// where the indices are taken from `header`.
///
/// This function is designed to handle complex scenarios when selecting the optimal index for a query.
/// The scenarios include:
///
/// - Combinations of multi- and single-column indexes that could refer to the same column.
///   For example, the table could have indexes `[a]` and `[a, b]]`
///   and a user could query for `WHERE a = 1 AND b = 2 AND a = 3`.
///
/// - Query constraints can be supplied in any order;
///   i.e., both `WHERE a = 1 AND b = 2`
///   and `WHERE b = 2 AND a = 1` are valid.
///
/// - Queries against multi-col indices must use the same operator in their constraints.
///   Otherwise, the index cannot be used.
///   That is, for `WHERE a < 1, b < 3`, we can use `ScanOrIndex::Index(Lt, [a, b], (1, 3))`,
///   whereas for `WHERE a < 1, b != 3`, we cannot.
///
/// - The use of multiple tables could generate redundant/duplicate operations like
///   `[ScanOrIndex::Index(a = 1), ScanOrIndex::Index(a = 1), ScanOrIndex::Scan(a = 1)]`.
///   This *cannot* be handled here.
///
/// # Returns
///
/// - A vector of `ScanOrIndex` representing the selected `index` OR `scan` operations.
///
/// - A HashSet of `(ColId, OpCmp)` representing the columns
///   and operators that can be served by an index.
///
///   This is required to remove the redundant operation on e.g.,
///   `[ScanOrIndex::Index(a = 1), ScanOrIndex::Index(a = 1), ScanOrIndex::Scan(a = 1)]`,
///   that could be generated by calling this function several times by using multiple `JOINS`.
///
/// # Example
///
/// If we have a table with `indexes`: `[a], [b], [b, c]` and then try to
/// optimize `WHERE a = 1 AND d > 2 AND c = 2 AND b = 1` we should return
///
/// -`ScanOrIndex::Index([c, b] = [1, 2])`
/// -`ScanOrIndex::Index(a = 1)`
/// -`ScanOrIndex::Scan(c = 2)`
///
/// # Note
///
/// NOTE: For a query like `SELECT * FROM students WHERE age > 18 AND height < 180`
/// we cannot serve this with a single `IndexScan`,
/// but rather, `select_best_index`
/// would give us two separate `IndexScan`s.
/// However, the upper layers of `QueryExpr` building will convert both of those into `Select`s.
/// In the case of `SELECT * FROM students WHERE age > 18 AND height > 180`
/// we would generate a single `IndexScan((age, height) > (18, 180))`.
/// However, and depending on the table data, this might not be efficient,
/// whereas `age = 18 AND height > 180` might.
/// TODO: Revisit this to see if we want to restrict this or use statistics.
fn select_best_index<'a>(
    cols_indexed: &mut ColsIndexed,
    header: &'a Header,
    ops: &[&'a ColumnOp],
) -> IndexColumnOpSink<'a> {
    // Collect and sort indices by their lengths, with longest first.
    // We do this so that multi-col indices are used first, as they are more efficient.
    // TODO(Centril): This could be computed when `Header` is constructed.
    let mut indices = header
        .constraints
        .iter()
        .filter(|(_, c)| c.has_indexed())
        .map(|(cl, _)| cl)
        .collect::<SmallVec<[_; 1]>>();
    indices.sort_unstable_by_key(|cl| Reverse(cl.len()));

    let mut found: IndexColumnOpSink = IndexColumnOpSink::new();

    // Collect fields into a multi-map `(col_id, cmp) -> [col value]`.
    // This gives us `log(N)` seek + deletion.
    // TODO(Centril): Consider https://docs.rs/small-map/0.1.3/small_map/enum.SmallMap.html
    let mut col_map = BTreeMap::<_, SmallVec<[_; 1]>>::new();
    extract_cols(ops, &mut col_map, &mut found);

    // Go through each operator and index,
    // consuming all column constraints that can be served by an index.
    //
    // NOTE: We do not consider `OpCmp::NotEq` at the moment
    // since those are typically not answered using an index.
    for (col_list, cmp) in [OpCmp::Eq, OpCmp::Lt, OpCmp::LtEq, OpCmp::Gt, OpCmp::GtEq]
        .into_iter()
        .flat_map(|cmp| indices.iter().map(move |cl| (*cl, cmp)))
    {
        // (1) No columns left? We're done.
        if col_map.is_empty() {
            break;
        }

        if col_list.is_singleton() {
            // For a single column index,
            // we want to avoid the `ProductValue` indirection of below.
            for ColValue { cmp, value, col, .. } in col_map.remove(&(col_list.head(), cmp)).into_iter().flatten() {
                found.push(make_index_arg(cmp, col_list, value.clone()));
                cols_indexed.insert((col, cmp));
            }
        } else if col_list
            .iter()
            // (2) Ensure that every col in the index has a column in `col_map`.
            .all(|col| col_map.get(&(col, cmp)).filter(|fs| !fs.is_empty()).is_some())
        {
            // We've ensured `col_list ⊆ columns_of(cols_map(cmp))`.
            // Construct the value to compare against.
            let mut elems = Vec::with_capacity(col_list.len() as usize);
            for col in col_list.iter() {
                // Retrieve the col value for this (col, cmp) key.
                // Remove the map entry if the list is empty now.
                let Entry::Occupied(mut entry) = col_map.entry((col, cmp)) else {
                    // We ensured in (2) that the map is occupied for `(col, cmp)`.
                    unreachable!()
                };
                let cvs = entry.get_mut();
                // We ensured in (2) that `cvs` is non-empty.
                let col_val = cvs.pop().unwrap();
                if cvs.is_empty() {
                    // Remove the entry so that (1) works.
                    entry.remove();
                }

                // Add the field value to the product value.
                elems.push(col_val.value.clone());
                cols_indexed.insert((col_val.col, cmp));
            }
            let value = AlgebraicValue::product(elems);
            found.push(make_index_arg(cmp, col_list, value));
        }
    }

    // The remaining constraints must be served by a scan.
    found.extend(
        col_map
            .into_iter()
            .flat_map(|(_, fs)| fs)
            .map(|f| IndexColumnOp::Scan(f.parent)),
    );

    found
}

/// Extracts `name = val` when `lhs` is a col and `rhs` is a value.
fn ext_field_val<'a>(lhs: &'a ColumnOp, rhs: &'a ColumnOp) -> Option<(ColId, &'a AlgebraicValue)> {
    if let (ColumnOp::Col(ColExpr::Col(col)), ColumnOp::Col(ColExpr::Value(val))) = (lhs, rhs) {
        return Some((*col, val));
    }
    None
}

/// Extracts `name = val` when `op` is `name = val`.
fn ext_cmp_field_val(op: &ColumnOp) -> Option<(OpCmp, ColId, &AlgebraicValue)> {
    match op {
        ColumnOp::Cmp {
            op: OpQuery::Cmp(op),
            lhs,
            rhs,
        } => ext_field_val(lhs, rhs).map(|(id, v)| (*op, id, v)),
        _ => None,
    }
}

/// Extracts a list of `col = val` constraints that *could* be answered by an index
/// and populates those into `col_map`.
/// The [`ColumnOp`]s that don't fit `col = val`
/// are made into [`IndexColumnOp::Scan`]s immediately which are added to `found`.
fn extract_cols<'a>(
    ops: &[&'a ColumnOp],
    col_map: &mut BTreeMap<(ColId, OpCmp), SmallVec<[ColValue<'a>; 1]>>,
    found: &mut IndexColumnOpSink<'a>,
) {
    let mut add_field = |parent, op, col, val| {
        let fv = ColValue::new(parent, col, op, val);
        col_map.entry((col, op)).or_default().push(fv);
    };

    for op in ops {
        match op {
            ColumnOp::Cmp {
                op: OpQuery::Cmp(cmp),
                lhs,
                rhs,
            } => {
                if let Some((field_col, val)) = ext_field_val(lhs, rhs) {
                    // `lhs` must be a field that exists and `rhs` must be a value.
                    add_field(op, *cmp, field_col, val);
                    continue;
                }
            }
            ColumnOp::Cmp {
                op: OpQuery::Logic(OpLogic::And),
                lhs,
                rhs,
            } => {
                if let Some((op_lhs, col_lhs, val_lhs)) = ext_cmp_field_val(lhs) {
                    if let Some((op_rhs, col_rhs, val_rhs)) = ext_cmp_field_val(rhs) {
                        // Both lhs and rhs columns must exist.
                        add_field(op, op_lhs, col_lhs, val_lhs);
                        add_field(op, op_rhs, col_rhs, val_rhs);
                        continue;
                    }
                }
            }
            ColumnOp::Cmp {
                op: OpQuery::Logic(OpLogic::Or),
                ..
            }
            | ColumnOp::Col(_) => {}
        }

        found.push(IndexColumnOp::Scan(op));
    }
}

/// Sargable stands for Search ARGument ABLE.
/// A sargable predicate is one that can be answered using an index.
fn find_sargable_ops<'a>(
    fields_indexed: &mut ColsIndexed,
    header: &'a Header,
    op: &'a ColumnOp,
) -> SmallVec<[IndexColumnOp<'a>; 1]> {
    let mut ops_flat = op.flatten_ands_ref();
    if ops_flat.len() == 1 {
        match ops_flat.swap_remove(0) {
            // Special case; fast path for a single field.
            op @ ColumnOp::Col(_) => smallvec![IndexColumnOp::Scan(op)],
            op => select_best_index(fields_indexed, header, &[op]),
        }
    } else {
        select_best_index(fields_indexed, header, &ops_flat)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
// TODO(bikeshedding): Refactor this struct so that `IndexJoin`s replace the `table`,
// rather than appearing as the first element of the `query`.
//
// `IndexJoin`s do not behave like filters; in fact they behave more like data sources.
// A query conceptually starts with either a single table or an `IndexJoin`,
// and then stacks a set of filters on top of that.
pub struct QueryExpr {
    pub source: SourceExpr,
    pub query: Vec<Query>,
}

impl From<SourceExpr> for QueryExpr {
    fn from(source: SourceExpr) -> Self {
        QueryExpr { source, query: vec![] }
    }
}

/// Iterator created by the [`Query::sources`] method.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub enum QuerySources {
    None,
    One(Option<SourceExpr>),
    Expr(QueryExprSources),
}

impl Iterator for QuerySources {
    type Item = SourceExpr;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::None => None,
            Self::One(src) => src.take(),
            Self::Expr(expr) => expr.next(),
        }
    }
}

impl QueryExpr {
    pub fn new<T: Into<SourceExpr>>(source: T) -> Self {
        Self {
            source: source.into(),
            query: vec![],
        }
    }

    /// Iterate over all [`SourceExpr`]s involved in the [`QueryExpr`].
    ///
    /// Sources are yielded from left to right. Duplicates are not filtered out.
    pub fn sources(&self) -> QueryExprSources {
        QueryExprSources {
            head: Some(self.source.clone()),
            tail: self.query.iter().map(Query::sources).collect(),
        }
    }

    /// Returns the last [`Header`] of this query.
    ///
    /// Starts the scan from the back to the front,
    /// looking for query operations that change the `Header`.
    /// These are `JoinInner` and `Project`.
    /// If there are no operations that alter the `Header`,
    /// this falls back to the origin `self.source.head()`.
    pub fn head(&self) -> &Arc<Header> {
        self.query
            .iter()
            .rev()
            .find_map(|op| match op {
                Query::Select(_) => None,
                Query::IndexScan(scan) => Some(&scan.table.head),
                Query::IndexJoin(join) if join.return_index_rows => Some(join.index_side.head()),
                Query::IndexJoin(join) => Some(join.probe_side.head()),
                Query::Project(proj) => Some(&proj.header_after),
                Query::JoinInner(join) => join.inner.as_ref(),
            })
            .unwrap_or_else(|| self.source.head())
    }

    /// Does this query read from a given table?
    pub fn reads_from_table(&self, id: &TableId) -> bool {
        self.source.table_id() == Some(*id)
            || self.query.iter().any(|q| match q {
                Query::Select(_) | Query::Project(..) => false,
                Query::IndexScan(scan) => scan.table.table_id == *id,
                Query::JoinInner(join) => join.rhs.reads_from_table(id),
                Query::IndexJoin(join) => {
                    join.index_side.table_id() == Some(*id) || join.probe_side.reads_from_table(id)
                }
            })
    }

    // Generate an index scan for an equality predicate if this is the first operator.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_eq(mut self, table: DbTable, columns: ColList, value: AlgebraicValue) -> Self {
        let point = |v: AlgebraicValue| (Bound::Included(v.clone()), Bound::Included(v));

        // if this is the first operator in the list, generate index scan
        let Some(query) = self.query.pop() else {
            let bounds = point(value);
            self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source: SourceExpr::DbTable(ref db_table),
                        ..
                    },
                ..
            }) if table.table_id != db_table.table_id => {
                self = self.with_index_eq(db_table.clone(), columns, value);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr {
                rhs,
                col_lhs,
                col_rhs,
                inner: semi,
            }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_eq(table, columns, value),
                    col_lhs,
                    col_rhs,
                    inner: semi,
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                let op = ColumnOp::and_cmp(OpCmp::Eq, &columns, value);
                self.query.push(Query::Select(ColumnOp::and(filter, op)));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                let op = ColumnOp::and_cmp(OpCmp::Eq, &columns, value);
                self.query.push(Query::Select(op));
                self
            }
        }
    }

    // Generate an index scan for a range predicate or try merging with a previous index scan.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_lower_bound(
        mut self,
        table: DbTable,
        columns: ColList,
        value: AlgebraicValue,
        inclusive: bool,
    ) -> Self {
        // if this is the first operator in the list, generate an index scan
        let Some(query) = self.query.pop() else {
            let bounds = (Self::bound(value, inclusive), Bound::Unbounded);
            self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source: SourceExpr::DbTable(ref db_table),
                        ..
                    },
                ..
            }) if table.table_id != db_table.table_id => {
                self = self.with_index_lower_bound(table, columns, value, inclusive);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr {
                rhs,
                col_lhs,
                col_rhs,
                inner: semi,
            }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_lower_bound(table, columns, value, inclusive),
                    col_lhs,
                    col_rhs,
                    inner: semi,
                }));
                self
            }
            // merge with a preceding upper bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                bounds: (Bound::Unbounded, Bound::Included(upper)),
                ..
            }) if columns == lhs_col_id => {
                let bounds = (Self::bound(value, inclusive), Bound::Included(upper));
                self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));
                self
            }
            // merge with a preceding upper bounded index scan (exclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                bounds: (Bound::Unbounded, Bound::Excluded(upper)),
                ..
            }) if columns == lhs_col_id => {
                // Queries like `WHERE x < 5 AND x > 5` never return any rows and are likely mistakes.
                // Detect such queries and log a warning.
                // Compute this condition early, then compute the resulting query and log it.
                // TODO: We should not emit an `IndexScan` in this case.
                // Further design work is necessary to decide whether this should be an error at query compile time,
                // or whether we should emit a query plan which explicitly says that it will return 0 rows.
                // The current behavior is a hack
                // because this patch was written (2024-04-01 pgoldman) a short time before the BitCraft alpha,
                // and a more invasive change was infeasible.
                let is_never = !inclusive && value == upper;

                let bounds = (Self::bound(value, inclusive), Bound::Excluded(upper));
                self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));

                if is_never {
                    log::warn!("Query will select no rows due to equal excluded bounds: {self:?}")
                }

                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                let bounds = (Self::bound(value, inclusive), Bound::Unbounded);
                let op = ColumnOp::from_op_col_bounds(&columns, bounds);
                self.query.push(Query::Select(ColumnOp::and(filter, op)));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                let bounds = (Self::bound(value, inclusive), Bound::Unbounded);
                let op = ColumnOp::from_op_col_bounds(&columns, bounds);
                self.query.push(Query::Select(op));
                self
            }
        }
    }

    // Generate an index scan for a range predicate or try merging with a previous index scan.
    // Otherwise generate a select.
    // TODO: Replace these methods with a proper query optimization pass.
    pub fn with_index_upper_bound(
        mut self,
        table: DbTable,
        columns: ColList,
        value: AlgebraicValue,
        inclusive: bool,
    ) -> Self {
        // if this is the first operator in the list, generate an index scan
        let Some(query) = self.query.pop() else {
            self.query.push(Query::IndexScan(IndexScan {
                table,
                columns,
                bounds: (Bound::Unbounded, Self::bound(value, inclusive)),
            }));
            return self;
        };
        match query {
            // try to push below join's lhs
            Query::JoinInner(JoinExpr {
                rhs:
                    QueryExpr {
                        source: SourceExpr::DbTable(ref db_table),
                        ..
                    },
                ..
            }) if table.table_id != db_table.table_id => {
                self = self.with_index_upper_bound(table, columns, value, inclusive);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr {
                rhs,
                col_lhs,
                col_rhs,
                inner: semi,
            }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_upper_bound(table, columns, value, inclusive),
                    col_lhs,
                    col_rhs,
                    inner: semi,
                }));
                self
            }
            // merge with a preceding lower bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                bounds: (Bound::Included(lower), Bound::Unbounded),
                ..
            }) if columns == lhs_col_id => {
                let bounds = (Bound::Included(lower), Self::bound(value, inclusive));
                self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));
                self
            }
            // merge with a preceding lower bounded index scan (exclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                bounds: (Bound::Excluded(lower), Bound::Unbounded),
                ..
            }) if columns == lhs_col_id => {
                // Queries like `WHERE x < 5 AND x > 5` never return any rows and are likely mistakes.
                // Detect such queries and log a warning.
                // Compute this condition early, then compute the resulting query and log it.
                // TODO: We should not emit an `IndexScan` in this case.
                // Further design work is necessary to decide whether this should be an error at query compile time,
                // or whether we should emit a query plan which explicitly says that it will return 0 rows.
                // The current behavior is a hack
                // because this patch was written (2024-04-01 pgoldman) a short time before the BitCraft alpha,
                // and a more invasive change was infeasible.
                let is_never = !inclusive && value == lower;

                let bounds = (Bound::Excluded(lower), Self::bound(value, inclusive));
                self.query.push(Query::IndexScan(IndexScan { table, columns, bounds }));

                if is_never {
                    log::warn!("Query will select no rows due to equal excluded bounds: {self:?}")
                }

                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                let bounds = (Bound::Unbounded, Self::bound(value, inclusive));
                let op = ColumnOp::from_op_col_bounds(&columns, bounds);
                self.query.push(Query::Select(ColumnOp::and(filter, op)));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                let bounds = (Bound::Unbounded, Self::bound(value, inclusive));
                let op = ColumnOp::from_op_col_bounds(&columns, bounds);
                self.query.push(Query::Select(op));
                self
            }
        }
    }

    pub fn with_select<O>(mut self, op: O) -> Result<Self, RelationError>
    where
        O: Into<FieldOp>,
    {
        let op = op.into();
        let Some(query) = self.query.pop() else {
            return self.add_base_select(op);
        };

        match (query, op) {
            (
                Query::JoinInner(JoinExpr {
                    rhs,
                    col_lhs,
                    col_rhs,
                    inner,
                }),
                FieldOp::Cmp {
                    op: OpQuery::Cmp(cmp),
                    lhs: field,
                    rhs: value,
                },
            ) => match (*field, *value) {
                (FieldOp::Field(FieldExpr::Name(field)), FieldOp::Field(FieldExpr::Value(value)))
                // Field is from lhs, so push onto join's left arg
                if self.head().column_pos(field).is_some() =>
                    {
                        // No typing restrictions on `field cmp value`,
                        // and there are no binary operators to recurse into.
                        self = self.with_select(FieldOp::cmp(field, cmp, value))?;
                        self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs, inner }));
                        Ok(self)
                    }
                (FieldOp::Field(FieldExpr::Name(field)), FieldOp::Field(FieldExpr::Value(value)))
                // Field is from rhs, so push onto join's right arg
                if rhs.head().column_pos(field).is_some() =>
                    {
                        // No typing restrictions on `field cmp value`,
                        // and there are no binary operators to recurse into.
                        let rhs = rhs.with_select(FieldOp::cmp(field, cmp, value))?;
                        self.query.push(Query::JoinInner(JoinExpr {
                            rhs,
                            col_lhs,
                            col_rhs,
                            inner,
                        }));
                        Ok(self)
                    }
                (field, value) => {
                    self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs, inner, }));

                    // As we have `field op value` we need not demand `bool`,
                    // but we must still recuse into each side.
                    self.check_field_op_logics(&field)?;
                    self.check_field_op_logics(&value)?;
                    // Convert to `ColumnOp`.
                    let col = field.names_to_cols(self.head()).unwrap();
                    let value = value.names_to_cols(self.head()).unwrap();
                    // Add `col op value` filter to query.
                    self.query.push(Query::Select(ColumnOp::new(OpQuery::Cmp(cmp), col, value)));
                    Ok(self)
                }
            },
            // We have a previous filter `lhs`, so join with `rhs` forming `lhs AND rhs`.
            (Query::Select(lhs), rhs) => {
                // Type check `rhs`, demanding `bool`.
                self.check_field_op(&rhs)?;
                // Convert to `ColumnOp`.
                let rhs = rhs.names_to_cols(self.head()).unwrap();
                // Add `lhs AND op` to query.
                self.query.push(Query::Select(ColumnOp::and(lhs, rhs)));
                Ok(self)
            }
            // No previous filter, so add a base one.
            (query, op) => {
                self.query.push(query);
                self.add_base_select(op)
            }
        }
    }

    /// Add a base `Select` query that filters according to `op`.
    /// The `op` is checked to produce a `bool` value.
    fn add_base_select(mut self, op: FieldOp) -> Result<Self, RelationError> {
        // Type check the filter, demanding `bool`.
        self.check_field_op(&op)?;
        // Convert to `ColumnOp`.
        let op = op.names_to_cols(self.head()).unwrap();
        // Add the filter.
        self.query.push(Query::Select(op));
        Ok(self)
    }

    /// Type checks a `FieldOp` with respect to `self`,
    /// ensuring that query evaluation cannot get stuck or panic due to `reduce_bool`.
    fn check_field_op(&self, op: &FieldOp) -> Result<(), RelationError> {
        use OpQuery::*;
        match op {
            // `lhs` and `rhs` must both be typed at `bool`.
            FieldOp::Cmp { op: Logic(_), lhs, rhs } => {
                self.check_field_op(lhs)?;
                self.check_field_op(rhs)?;
                Ok(())
            }
            // `lhs` and `rhs` have no typing restrictions.
            // The result of `lhs op rhs` will always be a `bool`
            // either by `Eq` or `Ord` on `AlgebraicValue` (see `ColumnOp::compare_bin_op`).
            // However, we still have to recurse into `lhs` and `rhs`
            // in case we have e.g., `a == (b == c)`.
            FieldOp::Cmp { op: Cmp(_), lhs, rhs } => {
                self.check_field_op_logics(lhs)?;
                self.check_field_op_logics(rhs)?;
                Ok(())
            }
            FieldOp::Field(FieldExpr::Value(AlgebraicValue::Bool(_))) => Ok(()),
            FieldOp::Field(FieldExpr::Value(v)) => Err(RelationError::NotBoolValue { val: v.clone() }),
            FieldOp::Field(FieldExpr::Name(field)) => {
                let field = *field;
                let head = self.head();
                let col_id = head.column_pos_or_err(field)?;
                let col_ty = &head.fields[col_id.idx()].algebraic_type;
                match col_ty {
                    &AlgebraicType::Bool => Ok(()),
                    ty => Err(RelationError::NotBoolType { field, ty: ty.clone() }),
                }
            }
        }
    }

    /// Traverses `op`, checking any logical operators for bool-typed operands.
    fn check_field_op_logics(&self, op: &FieldOp) -> Result<(), RelationError> {
        use OpQuery::*;
        match op {
            FieldOp::Field(_) => Ok(()),
            FieldOp::Cmp { op: Cmp(_), lhs, rhs } => {
                self.check_field_op_logics(lhs)?;
                self.check_field_op_logics(rhs)?;
                Ok(())
            }
            FieldOp::Cmp { op: Logic(_), lhs, rhs } => {
                self.check_field_op(lhs)?;
                self.check_field_op(rhs)?;
                Ok(())
            }
        }
    }

    pub fn with_select_cmp<LHS, RHS, O>(self, op: O, lhs: LHS, rhs: RHS) -> Result<Self, RelationError>
    where
        LHS: Into<FieldExpr>,
        RHS: Into<FieldExpr>,
        O: Into<OpQuery>,
    {
        let op = FieldOp::new(op.into(), FieldOp::Field(lhs.into()), FieldOp::Field(rhs.into()));
        self.with_select(op)
    }

    // Appends a project operation to the query operator pipeline.
    // The `wildcard_table_id` represents a projection of the form `table.*`.
    // This is used to determine if an inner join can be rewritten as an index join.
    pub fn with_project(
        mut self,
        fields: Vec<FieldExpr>,
        wildcard_table: Option<TableId>,
    ) -> Result<Self, RelationError> {
        if !fields.is_empty() {
            let header_before = self.head();

            // Translate the field expressions to column expressions.
            let mut cols = Vec::with_capacity(fields.len());
            for field in fields {
                cols.push(field.name_to_col(header_before)?);
            }

            // Project the header.
            // We'll store that so subsequent operations use that as a base.
            let header_after = Arc::new(header_before.project(&cols)?);

            // Add the projection.
            self.query.push(Query::Project(ProjectExpr {
                cols,
                wildcard_table,
                header_after,
            }));
        }
        Ok(self)
    }

    pub fn with_join_inner_raw(
        mut self,
        q_rhs: QueryExpr,
        c_lhs: ColId,
        c_rhs: ColId,
        inner: Option<Arc<Header>>,
    ) -> Self {
        self.query
            .push(Query::JoinInner(JoinExpr::new(q_rhs, c_lhs, c_rhs, inner)));
        self
    }

    pub fn with_join_inner(self, q_rhs: impl Into<QueryExpr>, c_lhs: ColId, c_rhs: ColId, semi: bool) -> Self {
        let q_rhs = q_rhs.into();
        let inner = (!semi).then(|| Arc::new(self.head().extend(q_rhs.head())));
        self.with_join_inner_raw(q_rhs, c_lhs, c_rhs, inner)
    }

    fn bound(value: AlgebraicValue, inclusive: bool) -> Bound<AlgebraicValue> {
        if inclusive {
            Bound::Included(value)
        } else {
            Bound::Excluded(value)
        }
    }

    /// Try to turn an inner join followed by a projection into a semijoin.
    ///
    /// This optimization recognizes queries of the form:
    ///
    /// ```ignore
    /// QueryExpr {
    ///   source: LHS,
    ///   query: [
    ///     JoinInner(JoinExpr {
    ///       rhs: RHS,
    ///       semi: false,
    ///       ..
    ///     }),
    ///     Project(LHS.*),
    ///     ...
    ///   ]
    /// }
    /// ```
    ///
    /// And combines the `JoinInner` with the `Project` into a `JoinInner` with `semi: true`.
    ///
    /// Current limitations of this optimization:
    /// - The `JoinInner` must be the first (0th) element of the `query`.
    ///   Future work could search through the `query` to find any applicable `JoinInner`s,
    ///   but the current implementation inspects only the first expr.
    ///   This is likely sufficient because this optimization is primarily useful for enabling `try_index_join`,
    ///   which is fundamentally limited to operate on the first expr.
    ///   Note that we still get to optimize incremental joins, because we first optimize the original query
    ///   with [`DbTable`] sources, which results in an [`IndexJoin`]
    ///   then we replace the sources with [`MemTable`]s and go back to a [`JoinInner`] with `semi: true`.
    /// - The `Project` must immediately follow the `JoinInner`, with no intervening exprs.
    ///   Future work could search through intervening exprs to detect that the RHS table is unused.
    /// - The LHS/source table must be a [`DbTable`], not a [`MemTable`].
    ///   This is so we can recognize a wildcard project by its table id.
    ///   Future work could inspect the set of projected fields and compare them to the LHS table's header instead.
    pub fn try_semi_join(self) -> QueryExpr {
        let QueryExpr { source, query } = self;

        let Some(source_table_id) = source.table_id() else {
            // Source is a `MemTable`, so we can't recognize a wildcard projection. Bail.
            return QueryExpr { source, query };
        };

        let mut exprs = query.into_iter();
        let Some(join_candidate) = exprs.next() else {
            // No first (0th) expr to be the join; bail.
            return QueryExpr { source, query: vec![] };
        };
        let Query::JoinInner(join) = join_candidate else {
            // First (0th) expr is not an inner join. Bail.
            return QueryExpr {
                source,
                query: itertools::chain![Some(join_candidate), exprs].collect(),
            };
        };

        let Some(project_candidate) = exprs.next() else {
            // No second (1st) expr to be the project. Bail.
            return QueryExpr {
                source,
                query: vec![Query::JoinInner(join)],
            };
        };

        let Query::Project(proj) = project_candidate else {
            // Second (1st) expr is not a wildcard projection. Bail.
            return QueryExpr {
                source,
                query: itertools::chain![Some(Query::JoinInner(join)), Some(project_candidate), exprs].collect(),
            };
        };

        if proj.wildcard_table != Some(source_table_id) {
            // Projection is selecting the RHS table. Bail.
            return QueryExpr {
                source,
                query: itertools::chain![Some(Query::JoinInner(join)), Some(Query::Project(proj)), exprs].collect(),
            };
        };

        // All conditions met; return a semijoin.
        let semijoin = JoinExpr { inner: None, ..join };

        QueryExpr {
            source,
            query: itertools::chain![Some(Query::JoinInner(semijoin)), exprs].collect(),
        }
    }

    // Try to turn an applicable join into an index join.
    // An applicable join is one that can use an index to probe the lhs.
    // It must also project only the columns from the lhs.
    //
    // Ex. SELECT Left.* FROM Left JOIN Right ON Left.id = Right.id ...
    // where `Left` has an index defined on `id`.
    fn try_index_join(self) -> QueryExpr {
        let mut query = self;
        // We expect a single operation - an inner join with `semi: true`.
        // These can be transformed by `try_semi_join` from a sequence of two queries, an inner join followed by a wildcard project.
        if query.query.len() != 1 {
            return query;
        }

        // If the source is a `MemTable`, it doesn't have any indexes,
        // so we can't plan an index join.
        if query.source.is_mem_table() {
            return query;
        }
        let source = query.source;
        let join = query.query.pop().unwrap();

        match join {
            Query::JoinInner(JoinExpr {
                rhs: probe_side,
                col_lhs: index_col,
                col_rhs: probe_col,
                inner: None,
            }) => {
                if !probe_side.query.is_empty() {
                    // An applicable join must have an index defined on the correct field.
                    if source.head().has_constraint(index_col, Constraints::indexed()) {
                        let index_join = IndexJoin {
                            probe_side,
                            probe_col,
                            index_side: source.clone(),
                            index_select: None,
                            index_col,
                            return_index_rows: true,
                        };
                        let query = [Query::IndexJoin(index_join)].into();
                        return QueryExpr { source, query };
                    }
                }
                let join = Query::JoinInner(JoinExpr {
                    rhs: probe_side,
                    col_lhs: index_col,
                    col_rhs: probe_col,
                    inner: None,
                });
                QueryExpr {
                    source,
                    query: vec![join],
                }
            }
            first => QueryExpr {
                source,
                query: vec![first],
            },
        }
    }

    /// Look for filters that could use indexes
    fn optimize_select(mut q: QueryExpr, op: ColumnOp, tables: &[SourceExpr]) -> QueryExpr {
        // Go through each table schema referenced in the query.
        // Find the first sargable condition and short-circuit.
        let mut fields_found = HashSet::new();
        for schema in tables {
            for op in find_sargable_ops(&mut fields_found, schema.head(), &op) {
                match &op {
                    IndexColumnOp::Index(_) | IndexColumnOp::Scan(ColumnOp::Col(_)) => {}
                    // Remove a duplicated/redundant operation on the same `field` and `op`
                    // like `[ScanOrIndex::Index(a = 1), ScanOrIndex::Index(a = 1), ScanOrIndex::Scan(a = 1)]`
                    IndexColumnOp::Scan(ColumnOp::Cmp { op, lhs, rhs: _ }) => {
                        if let (ColumnOp::Col(ColExpr::Col(col)), OpQuery::Cmp(cmp)) = (&**lhs, op) {
                            if !fields_found.insert((*col, *cmp)) {
                                continue;
                            }
                        }
                    }
                }

                match op {
                    // A sargable condition for on one of the table schemas,
                    // either an equality or range condition.
                    IndexColumnOp::Index(idx) => {
                        let table = schema
                            .get_db_table()
                            .expect("find_sargable_ops(schema, op) implies `schema.is_db_table()`")
                            .clone();

                        q = match idx {
                            IndexArgument::Eq { columns, value } => q.with_index_eq(table, columns.clone(), value),
                            IndexArgument::LowerBound {
                                columns,
                                value,
                                inclusive,
                            } => q.with_index_lower_bound(table, columns.clone(), value, inclusive),
                            IndexArgument::UpperBound {
                                columns,
                                value,
                                inclusive,
                            } => q.with_index_upper_bound(table, columns.clone(), value, inclusive),
                        };
                    }
                    // Filter condition cannot be answered using an index.
                    IndexColumnOp::Scan(rhs) => {
                        let rhs = rhs.clone();
                        let op = match q.query.pop() {
                            // Merge condition into any pre-existing `Select`.
                            Some(Query::Select(lhs)) => ColumnOp::and(lhs, rhs),
                            None => rhs,
                            Some(other) => {
                                q.query.push(other);
                                rhs
                            }
                        };
                        q.query.push(Query::Select(op));
                    }
                }
            }
        }

        q
    }

    pub fn optimize(mut self, row_count: &impl Fn(TableId, &str) -> i64) -> Self {
        let mut q = Self {
            source: self.source.clone(),
            query: Vec::with_capacity(self.query.len()),
        };

        let tables = self.sources();
        let tables: Vec<_> = core::iter::once(QuerySources::One(tables.head))
            .chain(tables.tail)
            .flat_map(|x| x.into_iter())
            .collect();

        if matches!(&*self.query, [Query::IndexJoin(_)]) {
            if let Some(Query::IndexJoin(join)) = self.query.pop() {
                q.query.push(Query::IndexJoin(join.reorder(row_count)));
                return q;
            }
        }

        for query in self.query {
            match query {
                Query::Select(op) => {
                    q = Self::optimize_select(q, op, &tables);
                }
                Query::JoinInner(join) => {
                    q = q.with_join_inner_raw(join.rhs.optimize(row_count), join.col_lhs, join.col_rhs, join.inner);
                }
                _ => q.query.push(query),
            };
        }

        // Make sure to `try_semi_join` before `try_index_join`, as the latter depends on the former.
        let q = q.try_semi_join();
        let q = q.try_index_join();
        if matches!(&*q.query, [Query::IndexJoin(_)]) {
            return q.optimize(row_count);
        }
        q
    }
}

/// Iterator created by the [`QueryExpr::sources`] method.
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct QueryExprSources {
    head: Option<SourceExpr>,
    tail: VecDeque<QuerySources>,
}

impl Iterator for QueryExprSources {
    type Item = SourceExpr;

    fn next(&mut self) -> Option<Self::Item> {
        self.head.take().or_else(|| {
            while let Some(cur) = self.tail.front_mut() {
                match cur.next() {
                    None => {
                        self.tail.pop_front();
                        continue;
                    }
                    Some(src) => return Some(src),
                }
            }

            None
        })
    }
}

impl AuthAccess for Query {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }

        for table in self.sources() {
            if table.table_access() == StAccess::Private {
                return Err(AuthError::TablePrivate {
                    named: table.table_name().to_owned(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, From)]
pub enum Expr {
    #[from]
    Value(AlgebraicValue),
    Block(Vec<Expr>),
    Ident(String),
    Crud(Box<CrudExpr>),
    Halt(ErrorLang),
}

impl From<QueryExpr> for Expr {
    fn from(x: QueryExpr) -> Self {
        Expr::Crud(Box::new(CrudExpr::Query(x)))
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Query::IndexScan(op) => {
                write!(f, "index_scan {:?}", op)
            }
            Query::IndexJoin(op) => {
                write!(f, "index_join {:?}", op)
            }
            Query::Select(q) => {
                write!(f, "select {q}")
            }
            Query::Project(proj) => {
                let q = &proj.cols;
                write!(f, "project")?;
                if !q.is_empty() {
                    write!(f, " ")?;
                }
                for (pos, x) in q.iter().enumerate() {
                    write!(f, "{x}")?;
                    if pos + 1 < q.len() {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            Query::JoinInner(q) => {
                write!(f, "&inner {:?} ON {} = {}", q.rhs, q.col_lhs, q.col_rhs)
            }
        }
    }
}

impl AuthAccess for SourceExpr {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller || self.table_access() == StAccess::Public {
            return Ok(());
        }

        Err(AuthError::TablePrivate {
            named: self.table_name().to_string(),
        })
    }
}

impl AuthAccess for QueryExpr {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        self.source.check_auth(owner, caller)?;
        for q in &self.query {
            q.check_auth(owner, caller)?;
        }

        Ok(())
    }
}

impl AuthAccess for CrudExpr {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        // Anyone may query, so as long as the tables involved are public.
        if let CrudExpr::Query(q) = self {
            return q.check_auth(owner, caller);
        }

        // Mutating operations require `owner == caller`.
        Err(AuthError::OwnerRequired)
    }
}

#[derive(Debug, PartialEq)]
pub enum Code {
    Value(AlgebraicValue),
    Table(MemTable),
    Halt(ErrorLang),
    Block(Vec<Code>),
    Crud(CrudExpr),
    Pass,
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Code::Value(x) => {
                write!(f, "{:?}", &x)
            }
            Code::Block(_) => write!(f, "Block"),
            x => todo!("{:?}", x),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CodeResult {
    Value(AlgebraicValue),
    Table(MemTable),
    Block(Vec<CodeResult>),
    Halt(ErrorLang),
    Pass,
}

impl From<Code> for CodeResult {
    fn from(code: Code) -> Self {
        match code {
            Code::Value(x) => Self::Value(x),
            Code::Table(x) => Self::Table(x),
            Code::Halt(x) => Self::Halt(x),
            Code::Block(x) => {
                if x.is_empty() {
                    Self::Pass
                } else {
                    Self::Block(x.into_iter().map(CodeResult::from).collect())
                }
            }
            Code::Pass => Self::Pass,
            x => Self::Halt(ErrorLang::new(
                ErrorKind::Compiler,
                Some(&format!("Invalid result: {x}")),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::relation::Column;
    use spacetimedb_sats::{product, AlgebraicType, ProductType};
    use typed_arena::Arena;

    const ALICE: Identity = Identity::from_byte_array([1; 32]);
    const BOB: Identity = Identity::from_byte_array([2; 32]);

    // TODO(kim): Should better do property testing here, but writing generators
    // on recursive types (ie. `Query` and friends) is tricky.

    fn tables() -> [SourceExpr; 2] {
        [
            SourceExpr::InMemory {
                source_id: SourceId(0),
                header: Arc::new(Header {
                    table_id: 42.into(),
                    table_name: "foo".into(),
                    fields: vec![],
                    constraints: Default::default(),
                }),
                table_type: StTableType::User,
                table_access: StAccess::Private,
            },
            SourceExpr::DbTable(DbTable {
                head: Arc::new(Header {
                    table_id: 42.into(),
                    table_name: "foo".into(),
                    fields: vec![],
                    constraints: vec![(ColId(42).into(), Constraints::indexed())],
                }),
                table_id: 42.into(),
                table_type: StTableType::User,
                table_access: StAccess::Private,
            }),
        ]
    }

    fn queries() -> impl IntoIterator<Item = Query> {
        let [mem_table, db_table] = tables();
        // Skip `Query::Select` and `QueryProject` -- they don't have table
        // information
        [
            Query::IndexScan(IndexScan {
                table: db_table.get_db_table().unwrap().clone(),
                columns: ColList::new(42.into()),
                bounds: (Bound::Included(22.into()), Bound::Unbounded),
            }),
            Query::IndexJoin(IndexJoin {
                probe_side: mem_table.clone().into(),
                probe_col: 0.into(),
                index_side: SourceExpr::DbTable(DbTable {
                    head: Arc::new(Header {
                        table_id: db_table.head().table_id,
                        table_name: db_table.table_name().into(),
                        fields: vec![],
                        constraints: Default::default(),
                    }),
                    table_id: db_table.head().table_id,
                    table_type: StTableType::User,
                    table_access: StAccess::Public,
                }),
                index_select: None,
                index_col: 22.into(),
                return_index_rows: true,
            }),
            Query::JoinInner(JoinExpr {
                col_rhs: 1.into(),
                rhs: mem_table.into(),
                col_lhs: 1.into(),
                inner: None,
            }),
        ]
    }

    fn query_exprs() -> impl IntoIterator<Item = QueryExpr> {
        tables().map(|table| {
            let mut expr = QueryExpr::from(table);
            expr.query = queries().into_iter().collect();
            expr
        })
    }

    fn assert_owner_private<T: AuthAccess>(auth: &T) {
        assert!(auth.check_auth(ALICE, ALICE).is_ok());
        assert!(matches!(
            auth.check_auth(ALICE, BOB),
            Err(AuthError::TablePrivate { .. })
        ));
    }

    fn assert_owner_required<T: AuthAccess>(auth: T) {
        assert!(auth.check_auth(ALICE, ALICE).is_ok());
        assert!(matches!(auth.check_auth(ALICE, BOB), Err(AuthError::OwnerRequired)));
    }

    fn mem_table(id: TableId, name: &str, fields: &[(u32, AlgebraicType, bool)]) -> SourceExpr {
        let table_access = StAccess::Public;
        let head = Header::new(
            id,
            name.into(),
            fields
                .iter()
                .map(|(col, ty, _)| Column::new(FieldName::new(id, (*col).into()), ty.clone()))
                .collect(),
            fields
                .iter()
                .enumerate()
                .filter(|(_, (_, _, indexed))| *indexed)
                .map(|(i, _)| (ColId::from(i).into(), Constraints::indexed()))
                .collect(),
        );
        SourceExpr::InMemory {
            source_id: SourceId(0),
            header: Arc::new(head),
            table_access,
            table_type: StTableType::User,
        }
    }

    #[test]
    fn test_index_to_inner_join() {
        let index_side = mem_table(
            0.into(),
            "index",
            &[(0, AlgebraicType::U8, false), (1, AlgebraicType::U8, true)],
        );
        let probe_side = mem_table(
            1.into(),
            "probe",
            &[(0, AlgebraicType::U8, false), (1, AlgebraicType::U8, true)],
        );

        let index_col = 1.into();
        let probe_col = 1.into();
        let index_select = ColumnOp::cmp(0, OpCmp::Eq, 0u8);
        let join = IndexJoin {
            probe_side: probe_side.clone().into(),
            probe_col,
            index_side: index_side.clone(),
            index_select: Some(index_select.clone()),
            index_col,
            return_index_rows: false,
        };

        let expr = join.to_inner_join();

        assert_eq!(expr.source, probe_side);
        assert_eq!(expr.query.len(), 1);

        let Query::JoinInner(ref join) = expr.query[0] else {
            panic!("expected an inner join, but got {:#?}", expr.query[0]);
        };

        assert_eq!(join.col_lhs, probe_col);
        assert_eq!(join.col_rhs, index_col);
        assert_eq!(
            join.rhs,
            QueryExpr {
                source: index_side,
                query: vec![index_select.into()]
            }
        );
        assert_eq!(join.inner, None);
    }

    fn setup_best_index() -> (Header, [ColId; 5], [AlgebraicValue; 5]) {
        let table_id = 0.into();

        let vals = [1, 2, 3, 4, 5].map(AlgebraicValue::U64);
        let col_ids = [0, 1, 2, 3, 4].map(ColId);
        let [a, b, c, d, _] = col_ids;
        let columns = col_ids.map(|c| Column::new(FieldName::new(table_id, c), AlgebraicType::I8));

        let head1 = Header::new(
            table_id,
            "t1".into(),
            columns.to_vec(),
            vec![
                //Index a
                (a.into(), Constraints::primary_key()),
                //Index b
                (b.into(), Constraints::indexed()),
                //Index b + c
                (col_list![b, c], Constraints::unique()),
                //Index a + b + c + d
                (col_list![a, b, c, d], Constraints::indexed()),
            ],
        );

        (head1, col_ids, vals)
    }

    fn make_field_value<'a>(
        arena: &'a Arena<ColumnOp>,
        (cmp, col, value): (OpCmp, ColId, &'a AlgebraicValue),
    ) -> ColValue<'a> {
        let from_expr = |expr| Box::new(ColumnOp::Col(expr));
        let op = ColumnOp::Cmp {
            op: OpQuery::Cmp(cmp),
            lhs: from_expr(ColExpr::Col(col)),
            rhs: from_expr(ColExpr::Value(value.clone())),
        };
        let parent = arena.alloc(op);
        ColValue::new(parent, col, cmp, value)
    }

    fn scan_eq<'a>(arena: &'a Arena<ColumnOp>, col: ColId, val: &'a AlgebraicValue) -> IndexColumnOp<'a> {
        scan(arena, OpCmp::Eq, col, val)
    }

    fn scan<'a>(arena: &'a Arena<ColumnOp>, cmp: OpCmp, col: ColId, val: &'a AlgebraicValue) -> IndexColumnOp<'a> {
        IndexColumnOp::Scan(make_field_value(arena, (cmp, col, val)).parent)
    }

    #[test]
    fn best_index() {
        let (head1, fields, vals) = setup_best_index();
        let [col_a, col_b, col_c, col_d, col_e] = fields;
        let [val_a, val_b, val_c, val_d, val_e] = vals;

        let arena = Arena::new();
        let select_best_index = |fields: &[_]| {
            let fields = fields
                .iter()
                .copied()
                .map(|(col, val): (ColId, _)| make_field_value(&arena, (OpCmp::Eq, col, val)).parent)
                .collect::<Vec<_>>();
            select_best_index(&mut <_>::default(), &head1, &fields)
        };

        let col_list_arena = Arena::new();
        let idx_eq = |cols, val| make_index_arg(OpCmp::Eq, col_list_arena.alloc(cols), val);

        // Check for simple scan
        assert_eq!(
            select_best_index(&[(col_d, &val_e)]),
            [scan_eq(&arena, col_d, &val_e)].into(),
        );

        assert_eq!(
            select_best_index(&[(col_a, &val_a)]),
            [idx_eq(col_a.into(), val_a.clone())].into(),
        );

        assert_eq!(
            select_best_index(&[(col_b, &val_b)]),
            [idx_eq(col_b.into(), val_b.clone())].into(),
        );

        // Check for permutation
        assert_eq!(
            select_best_index(&[(col_b, &val_b), (col_c, &val_c)]),
            [idx_eq(
                col_list![col_b, col_c],
                product![val_b.clone(), val_c.clone()].into()
            )]
            .into(),
        );

        assert_eq!(
            select_best_index(&[(col_c, &val_c), (col_b, &val_b)]),
            [idx_eq(
                col_list![col_b, col_c],
                product![val_b.clone(), val_c.clone()].into()
            )]
            .into(),
        );

        // Check for permutation
        assert_eq!(
            select_best_index(&[(col_a, &val_a), (col_b, &val_b), (col_c, &val_c), (col_d, &val_d)]),
            [idx_eq(
                col_list![col_a, col_b, col_c, col_d],
                product![val_a.clone(), val_b.clone(), val_c.clone(), val_d.clone()].into(),
            )]
            .into(),
        );

        assert_eq!(
            select_best_index(&[(col_b, &val_b), (col_a, &val_a), (col_d, &val_d), (col_c, &val_c)]),
            [idx_eq(
                col_list![col_a, col_b, col_c, col_d],
                product![val_a.clone(), val_b.clone(), val_c.clone(), val_d.clone()].into(),
            )]
            .into()
        );

        // Check mix scan + index
        assert_eq!(
            select_best_index(&[(col_b, &val_b), (col_a, &val_a), (col_e, &val_e), (col_d, &val_d)]),
            [
                idx_eq(col_a.into(), val_a.clone()),
                idx_eq(col_b.into(), val_b.clone()),
                scan_eq(&arena, col_d, &val_d),
                scan_eq(&arena, col_e, &val_e),
            ]
            .into()
        );

        assert_eq!(
            select_best_index(&[(col_b, &val_b), (col_c, &val_c), (col_d, &val_d)]),
            [
                idx_eq(col_list![col_b, col_c], product![val_b.clone(), val_c.clone()].into(),),
                scan_eq(&arena, col_d, &val_d),
            ]
            .into()
        );
    }

    #[test]
    fn best_index_range() {
        let arena = Arena::new();

        let (head1, cols, vals) = setup_best_index();
        let [col_a, col_b, col_c, col_d, _] = cols;
        let [val_a, val_b, val_c, val_d, _] = vals;

        let select_best_index = |cols: &[_]| {
            let fields = cols
                .iter()
                .map(|x| make_field_value(&arena, *x).parent)
                .collect::<Vec<_>>();
            select_best_index(&mut <_>::default(), &head1, &fields)
        };

        let col_list_arena = Arena::new();
        let idx = |cmp, cols: &[ColId], val: &AlgebraicValue| {
            let columns = cols.iter().copied().collect::<ColListBuilder>().build().unwrap();
            let columns = col_list_arena.alloc(columns);
            make_index_arg(cmp, columns, val.clone())
        };

        // Same field indexed
        assert_eq!(
            select_best_index(&[(OpCmp::Gt, col_a, &val_a), (OpCmp::Lt, col_a, &val_b)]),
            [idx(OpCmp::Lt, &[col_a], &val_b), idx(OpCmp::Gt, &[col_a], &val_a)].into()
        );

        // Same field scan
        assert_eq!(
            select_best_index(&[(OpCmp::Gt, col_d, &val_d), (OpCmp::Lt, col_d, &val_b)]),
            [
                scan(&arena, OpCmp::Lt, col_d, &val_b),
                scan(&arena, OpCmp::Gt, col_d, &val_d)
            ]
            .into()
        );
        // One indexed other scan
        assert_eq!(
            select_best_index(&[(OpCmp::Gt, col_b, &val_b), (OpCmp::Lt, col_c, &val_c)]),
            [idx(OpCmp::Gt, &[col_b], &val_b), scan(&arena, OpCmp::Lt, col_c, &val_c)].into()
        );

        // 1 multi-indexed 1 index
        assert_eq!(
            select_best_index(&[
                (OpCmp::Eq, col_b, &val_b),
                (OpCmp::GtEq, col_a, &val_a),
                (OpCmp::Eq, col_c, &val_c),
            ]),
            [
                idx(
                    OpCmp::Eq,
                    &[col_b, col_c],
                    &product![val_b.clone(), val_c.clone()].into(),
                ),
                idx(OpCmp::GtEq, &[col_a], &val_a),
            ]
            .into()
        );

        // 1 indexed 2 scan
        assert_eq!(
            select_best_index(&[
                (OpCmp::Gt, col_b, &val_b),
                (OpCmp::Eq, col_a, &val_a),
                (OpCmp::Lt, col_c, &val_c),
            ]),
            [
                idx(OpCmp::Eq, &[col_a], &val_a),
                idx(OpCmp::Gt, &[col_b], &val_b),
                scan(&arena, OpCmp::Lt, col_c, &val_c),
            ]
            .into()
        );
    }

    #[test]
    fn test_auth_table() {
        tables().iter().for_each(assert_owner_private)
    }

    #[test]
    fn test_auth_query_code() {
        for code in query_exprs() {
            assert_owner_private(&code)
        }
    }

    #[test]
    fn test_auth_query() {
        for query in queries() {
            assert_owner_private(&query);
        }
    }

    #[test]
    fn test_auth_crud_code_query() {
        for query in query_exprs() {
            let crud = CrudExpr::Query(query);
            assert_owner_private(&crud);
        }
    }

    #[test]
    fn test_auth_crud_code_insert() {
        for table in tables().into_iter().filter_map(|s| s.get_db_table().cloned()) {
            let crud = CrudExpr::Insert { table, rows: vec![] };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_update() {
        for qc in query_exprs() {
            let crud = CrudExpr::Update {
                delete: qc,
                assignments: Default::default(),
            };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_delete() {
        for query in query_exprs() {
            let crud = CrudExpr::Delete { query };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_create_table() {
        let table = TableDef::new("etcpasswd".into(), vec![])
            .with_access(StAccess::Public)
            .with_type(StTableType::System); // hah!

        let crud = CrudExpr::CreateTable { table: Box::new(table) };
        assert_owner_required(crud);
    }

    #[test]
    fn test_auth_crud_code_drop() {
        let crud = CrudExpr::Drop {
            name: "etcpasswd".into(),
            kind: DbType::Table,
            table_access: StAccess::Public,
        };
        assert_owner_required(crud);
    }

    #[test]
    /// Tests that [`QueryExpr::optimize`] can rewrite inner joins followed by projections into semijoins.
    fn optimize_inner_join_to_semijoin() {
        let lhs = TableSchema::from_def(
            TableId(0),
            TableDef::new(
                "lhs".into(),
                ProductType::from_iter([AlgebraicType::I32, AlgebraicType::String]).into(),
            ),
        );
        let rhs = TableSchema::from_def(
            TableId(1),
            TableDef::new(
                "rhs".into(),
                ProductType::from_iter([AlgebraicType::I32, AlgebraicType::I64]).into(),
            ),
        );

        let lhs_source = SourceExpr::from(&lhs);
        let rhs_source = SourceExpr::from(&rhs);

        let q = QueryExpr::new(lhs_source.clone())
            .with_join_inner(rhs_source.clone(), 0.into(), 0.into(), false)
            .with_project(
                [0, 1]
                    .map(|c| FieldExpr::Name(FieldName::new(lhs.table_id, c.into())))
                    .into(),
                Some(TableId(0)),
            )
            .unwrap();
        let q = q.optimize(&|_, _| 0);

        assert_eq!(q.source, lhs_source, "Optimized query should read from lhs");

        assert_eq!(
            q.query.len(),
            1,
            "Optimized query should have a single member, a semijoin"
        );
        match &q.query[0] {
            Query::JoinInner(JoinExpr { rhs, inner: semi, .. }) => {
                assert_eq!(semi, &None, "Optimized query should be a semijoin");
                assert_eq!(rhs.source, rhs_source, "Optimized query should filter with rhs");
                assert!(
                    rhs.query.is_empty(),
                    "Optimized query should not filter rhs before joining"
                );
            }
            wrong => panic!("Expected an inner join, but found {wrong:?}"),
        }
    }

    #[test]
    /// Tests that [`QueryExpr::optimize`] will not rewrite inner joins which are not followed by projections to the LHS table.
    fn optimize_inner_join_no_project() {
        let lhs = TableSchema::from_def(
            TableId(0),
            TableDef::new(
                "lhs".into(),
                ProductType::from_iter([AlgebraicType::I32, AlgebraicType::String]).into(),
            ),
        );
        let rhs = TableSchema::from_def(
            TableId(1),
            TableDef::new(
                "rhs".into(),
                ProductType::from_iter([AlgebraicType::I32, AlgebraicType::I64]).into(),
            ),
        );

        let lhs_source = SourceExpr::from(&lhs);
        let rhs_source = SourceExpr::from(&rhs);

        let q = QueryExpr::new(lhs_source.clone()).with_join_inner(rhs_source.clone(), 0.into(), 0.into(), false);
        let optimized = q.clone().optimize(&|_, _| 0);
        assert_eq!(q, optimized);
    }

    #[test]
    /// Tests that [`QueryExpr::optimize`] will not rewrite inner joins followed by projections to the RHS rather than LHS table.
    fn optimize_inner_join_wrong_project() {
        let lhs = TableSchema::from_def(
            TableId(0),
            TableDef::new(
                "lhs".into(),
                ProductType::from([AlgebraicType::I32, AlgebraicType::String]).into(),
            ),
        );
        let rhs = TableSchema::from_def(
            TableId(1),
            TableDef::new(
                "rhs".into(),
                ProductType::from([AlgebraicType::I32, AlgebraicType::I64]).into(),
            ),
        );

        let lhs_source = SourceExpr::from(&lhs);
        let rhs_source = SourceExpr::from(&rhs);

        let q = QueryExpr::new(lhs_source.clone())
            .with_join_inner(rhs_source.clone(), 0.into(), 0.into(), false)
            .with_project(
                [0, 1]
                    .map(|c| FieldExpr::Name(FieldName::new(rhs.table_id, c.into())))
                    .into(),
                Some(TableId(1)),
            )
            .unwrap();
        let optimized = q.clone().optimize(&|_, _| 0);
        assert_eq!(q, optimized);
    }
}
