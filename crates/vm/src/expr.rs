use crate::errors::{ErrorKind, ErrorLang, ErrorType, ErrorVm};
use crate::operator::{OpCmp, OpLogic, OpQuery};
use crate::relation::{MemTable, RelValue, Table};
use derive_more::From;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::*;
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::{TableDef, TableSchema};
use spacetimedb_sats::db::error::AuthError;
use spacetimedb_sats::relation::{Column, DbTable, FieldExpr, FieldName, Header, Relation, RowCount};
use spacetimedb_sats::ProductValue;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::ops::Bound;
use std::sync::Arc;

/// Trait for checking if the `caller` have access to `Self`
pub trait AuthAccess {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From)]
pub enum ColumnOp {
    #[from]
    Field(FieldExpr),
    Cmp {
        op: OpQuery,
        lhs: Box<ColumnOp>,
        rhs: Box<ColumnOp>,
    },
}

impl ColumnOp {
    pub fn new(op: OpQuery, lhs: ColumnOp, rhs: ColumnOp) -> Self {
        Self::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn cmp(field: FieldName, op: OpCmp, value: AlgebraicValue) -> Self {
        Self::Cmp {
            op: OpQuery::Cmp(op),
            lhs: Box::new(ColumnOp::Field(FieldExpr::Name(field))),
            rhs: Box::new(ColumnOp::Field(FieldExpr::Value(value))),
        }
    }

    fn reduce(&self, row: &RelValue<'_>, value: &ColumnOp, header: &Header) -> Result<AlgebraicValue, ErrorLang> {
        match value {
            ColumnOp::Field(field) => Ok(row.get(field, header)?.into_owned()),
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs, header)?.into()),
        }
    }

    fn reduce_bool(&self, row: &RelValue<'_>, value: &ColumnOp, header: &Header) -> Result<bool, ErrorLang> {
        match value {
            ColumnOp::Field(field) => {
                let field = row.get(field, header)?;

                match field.as_bool() {
                    Some(b) => Ok(*b),
                    None => Err(ErrorType::FieldBool(field.into_owned()).into()),
                }
            }
            ColumnOp::Cmp { op, lhs, rhs } => Ok(self.compare_bin_op(row, *op, lhs, rhs, header)?),
        }
    }

    fn compare_bin_op(
        &self,
        row: &RelValue<'_>,
        op: OpQuery,
        lhs: &ColumnOp,
        rhs: &ColumnOp,
        header: &Header,
    ) -> Result<bool, ErrorVm> {
        match op {
            OpQuery::Cmp(op) => {
                let lhs = self.reduce(row, lhs, header)?;
                let rhs = self.reduce(row, rhs, header)?;

                Ok(match op {
                    OpCmp::Eq => lhs == rhs,
                    OpCmp::NotEq => lhs != rhs,
                    OpCmp::Lt => lhs < rhs,
                    OpCmp::LtEq => lhs <= rhs,
                    OpCmp::Gt => lhs > rhs,
                    OpCmp::GtEq => lhs >= rhs,
                })
            }
            OpQuery::Logic(op) => {
                let lhs = self.reduce_bool(row, lhs, header)?;
                let rhs = self.reduce_bool(row, rhs, header)?;

                Ok(match op {
                    OpLogic::And => lhs && rhs,
                    OpLogic::Or => lhs || rhs,
                })
            }
        }
    }

    pub fn compare(&self, row: &RelValue<'_>, header: &Header) -> Result<bool, ErrorVm> {
        match self {
            ColumnOp::Field(field) => {
                let lhs = row.get(field, header)?;
                Ok(*lhs.as_bool().unwrap())
            }
            ColumnOp::Cmp { op, lhs, rhs } => self.compare_bin_op(row, *op, lhs, rhs, header),
        }
    }

    // Flattens a nested conjunction of AND expressions.
    pub fn to_vec(self) -> Vec<ColumnOp> {
        match self {
            ColumnOp::Cmp {
                op: OpQuery::Logic(OpLogic::And),
                lhs,
                rhs,
            } => {
                let mut lhs = lhs.to_vec();
                let mut rhs = rhs.to_vec();
                lhs.append(&mut rhs);
                lhs
            }
            op => vec![op],
        }
    }
}

impl fmt::Display for ColumnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColumnOp::Field(x) => {
                write!(f, "{}", x)
            }
            ColumnOp::Cmp { op, lhs, rhs } => {
                write!(f, "{} {} {}", lhs, op, rhs)
            }
        }
    }
}

impl From<FieldName> for ColumnOp {
    fn from(value: FieldName) -> Self {
        ColumnOp::Field(value.into())
    }
}

impl From<FieldName> for Box<ColumnOp> {
    fn from(value: FieldName) -> Self {
        Box::new(ColumnOp::Field(value.into()))
    }
}

impl From<AlgebraicValue> for ColumnOp {
    fn from(value: AlgebraicValue) -> Self {
        ColumnOp::Field(value.into())
    }
}

impl From<AlgebraicValue> for Box<ColumnOp> {
    fn from(value: AlgebraicValue) -> Self {
        Box::new(ColumnOp::Field(value.into()))
    }
}

impl From<IndexScan> for Box<ColumnOp> {
    fn from(value: IndexScan) -> Self {
        Box::new(value.into())
    }
}

impl From<IndexScan> for ColumnOp {
    fn from(value: IndexScan) -> Self {
        let table = value.table;
        let columns = value.columns;
        assert_eq!(columns.len(), 1, "multi-column predicates are not yet supported");

        let field = table.head().fields[usize::from(columns.head())].field.clone();
        match (value.lower_bound, value.upper_bound) {
            // Inclusive lower bound => field >= value
            (Bound::Included(value), Bound::Unbounded) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::GtEq),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Exclusive lower bound => field > value
            (Bound::Excluded(value), Bound::Unbounded) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::Gt),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Inclusive upper bound => field <= value
            (Bound::Unbounded, Bound::Included(value)) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::LtEq),
                lhs: field.into(),
                rhs: value.into(),
            },
            // Exclusive upper bound => field < value
            (Bound::Unbounded, Bound::Excluded(value)) => ColumnOp::Cmp {
                op: OpQuery::Cmp(OpCmp::Lt),
                lhs: field.into(),
                rhs: value.into(),
            },
            (Bound::Unbounded, Bound::Unbounded) => unreachable!(),
            (lower_bound, upper_bound) => {
                let lhs = IndexScan {
                    table: table.clone(),
                    columns: columns.clone(),
                    lower_bound,
                    upper_bound: Bound::Unbounded,
                };
                let rhs = IndexScan {
                    table,
                    columns,
                    lower_bound: Bound::Unbounded,
                    upper_bound,
                };
                ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: lhs.into(),
                    rhs: rhs.into(),
                }
            }
        }
    }
}

impl From<Query> for Option<ColumnOp> {
    fn from(value: Query) -> Self {
        match value {
            Query::IndexScan(op) => Some(op.into()),
            Query::Select(op) => Some(op),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[repr(transparent)]
/// A set of [`MemTable`]s referenced by a query plan by their [`SourceId`].
///
/// Rather than embedding [`MemTable`]s in query plans, we store a `SourceExpr::MemTable`,
/// which contains the information necessary for optimization along with a [`SourceId`].
/// Query execution then executes the plan, and when it encounters a `SourceExpr::MemTable`,
/// retrieves the [`MemTable`] from the corresponding `SourceSet`.
/// This allows query plans to be re-used, though each execution requires a new `SourceSet`.
///
/// Internally, the `SourceSet` stores an `Option<MemTable>` for each planned [`SourceId`].
/// During execution, the VM will [`Option::take`] the [`MemTable`] to consume them.
/// This means that a query plan may not include multiple references to the same [`SourceId`].
pub struct SourceSet(Vec<Option<MemTable>>);

impl SourceSet {
    /// Get a fresh `SourceId` which can be used as the id for a new entry.
    fn next_id(&self) -> SourceId {
        SourceId(self.0.len())
    }

    /// Insert a [`MemTable`] into this `SourceSet` so it can be used in a query plan,
    /// and return a [`SourceExpr`] which can be embedded in that plan.
    pub fn add_mem_table(&mut self, table: MemTable) -> SourceExpr {
        let source_id = self.next_id();
        let expr = SourceExpr::from_mem_table(&table, source_id);
        self.0.push(Some(table));
        expr
    }

    /// Extract the [`MemTable`] referred to by `id` from this `SourceSet`,
    /// leaving a "gap" in its place.
    ///
    /// Subsequent calls to `take_mem_table` on the same `id` will return `None`.
    pub fn take_mem_table(&mut self, id: SourceId) -> Option<MemTable> {
        self.0.get_mut(id.0)?.take()
    }

    /// Resolve `source` to a `Table` for use in query execution.
    ///
    /// If the `source` is a [`SourceExpr::DbTable`], this simply clones the [`DbTable`] and returns it.
    /// ([`DbTable::clone`] is inexpensive.)
    /// In this case, `self` is not modified.
    ///
    /// If the `source` is a [`SourceExpr::MemTable`], this behaves like [`Self::take_mem_table`].
    /// Subsequent calls to `take_table` or `take_mem_table` with the same `source` will fail.
    pub fn take_table(&mut self, source: &SourceExpr) -> Option<Table> {
        match source {
            SourceExpr::DbTable(db_table) => Some(Table::DbTable(db_table.clone())),
            SourceExpr::MemTable { source_id, .. } => self.take_mem_table(*source_id).map(Table::MemTable),
        }
    }
}

impl std::ops::Index<SourceId> for SourceSet {
    type Output = Option<MemTable>;

    fn index(&self, idx: SourceId) -> &Option<MemTable> {
        &self.0[idx.0]
    }
}

impl std::ops::IndexMut<SourceId> for SourceSet {
    fn index_mut(&mut self, idx: SourceId) -> &mut Option<MemTable> {
        &mut self.0[idx.0]
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

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
/// A reference to a table within a query plan,
/// used as the source for selections, scans, filters and joins.
pub enum SourceExpr {
    /// A plan for a "virtual" or projected table.
    ///
    /// The actual [`MemTable`] is not stored within the query plan;
    /// rather, the `source_id` is an index with a corresponding [`SourceSet`]
    /// which contains the [`MemTable`].
    ///
    /// This allows query plans to be reused by supplying a new [`SourceSet`].
    MemTable {
        source_id: SourceId,
        header: Arc<Header>,
        table_type: StTableType,
        table_access: StAccess,
        row_count: RowCount,
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
        if let SourceExpr::MemTable { source_id, .. } = self {
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
            SourceExpr::MemTable { table_type, .. } => *table_type,
            SourceExpr::DbTable(db_table) => db_table.table_type,
        }
    }

    pub fn table_access(&self) -> StAccess {
        match self {
            SourceExpr::MemTable { table_access, .. } => *table_access,
            SourceExpr::DbTable(db_table) => db_table.table_access,
        }
    }

    pub fn head(&self) -> &Arc<Header> {
        match self {
            SourceExpr::MemTable { header, .. } => header,
            SourceExpr::DbTable(db_table) => &db_table.head,
        }
    }

    /// Check if the `name` of the [FieldName] exist on this [SourceExpr]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_field<'a>(&'a self, field: &'a FieldName) -> Option<&Column> {
        self.head().column(field)
    }

    pub fn is_mem_table(&self) -> bool {
        matches!(self, SourceExpr::MemTable { .. })
    }

    pub fn is_db_table(&self) -> bool {
        matches!(self, SourceExpr::DbTable(_))
    }

    pub fn from_mem_table(mem_table: &MemTable, id: SourceId) -> Self {
        SourceExpr::MemTable {
            source_id: id,
            header: mem_table.head.clone(),
            table_type: StTableType::User,
            table_access: mem_table.table_access,
            row_count: RowCount::exact(mem_table.data.len()),
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

impl Relation for SourceExpr {
    fn head(&self) -> &Arc<Header> {
        self.head()
    }

    fn row_count(&self) -> RowCount {
        match self {
            SourceExpr::MemTable { row_count, .. } => *row_count,
            SourceExpr::DbTable(_) => RowCount::unknown(),
        }
    }
}

impl From<&TableSchema> for SourceExpr {
    fn from(value: &TableSchema) -> Self {
        SourceExpr::DbTable(DbTable::new(
            Arc::new(value.into()),
            value.table_id,
            value.table_type,
            value.table_access,
        ))
    }
}

// A descriptor for an index join operation.
// The semantics are those of a semijoin with rows from the index or the probe side being returned.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IndexJoin {
    pub probe_side: QueryExpr,
    pub probe_field: FieldName,
    pub index_side: SourceExpr,
    pub index_select: Option<ColumnOp>,
    pub index_col: ColId,
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
            .has_constraint(&self.probe_field, Constraints::indexed())
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
        // The compiler ensures the following unwrap is safe.
        // The existence of this column has already been verified,
        // during construction of the index join.
        let probe_column = self.probe_side.source.head().column(&self.probe_field).unwrap().col_id;
        match self.index_side.get_db_table() {
            // If the size of the indexed table is sufficiently large,
            // do not reorder.
            //
            // TODO: This determination is quite arbitrary.
            // Ultimately we should be using cardinality estimation.
            Some(DbTable { head, table_id, .. }) if row_count(*table_id, &head.table_name) > 3000 => self,
            // If this is a delta table, we must reorder.
            // If this is a sufficiently small physical table, we should reorder.
            _ => {
                // For the same reason the compiler also ensures this unwrap is safe.
                let index_field = self
                    .index_side
                    .head()
                    .fields
                    .iter()
                    .find(|col| col.col_id == self.index_col)
                    .unwrap()
                    .field
                    .clone();
                // Merge all selections from the original probe side into a single predicate.
                // This includes an index scan if present.
                let predicate = self.probe_side.query.into_iter().fold(None, |acc, op| {
                    <Query as Into<Option<ColumnOp>>>::into(op).map(|op| {
                        if let Some(predicate) = acc {
                            ColumnOp::new(OpQuery::Logic(OpLogic::And), predicate, op)
                        } else {
                            op
                        }
                    })
                });
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
                    probe_field: index_field,
                    // The original probe table is now the table that is being probed.
                    index_side: self.probe_side.source,
                    // Any selections from the original probe side are pulled above the index lookup.
                    index_select: predicate,
                    // The new index field is the previous probe field.
                    index_col: probe_column,
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
            let col_lhs = self.index_side.head().fields[usize::from(self.index_col)].field.clone();
            let col_rhs = self.probe_field;
            let rhs = self.probe_side;

            let fields = self
                .index_side
                .head()
                .fields
                .iter()
                .cloned()
                .map(|Column { field, .. }| field.into())
                .collect();

            let table = self.index_side.table_id();
            let source = self.index_side;
            let inner_join = Query::JoinInner(JoinExpr::new(rhs, col_lhs, col_rhs));
            let project = Query::Project(fields, table);
            let query = if let Some(predicate) = self.index_select {
                vec![predicate.into(), inner_join, project]
            } else {
                vec![inner_join, project]
            };
            QueryExpr { source, query }
        } else {
            let col_rhs = self.index_side.head().fields[usize::from(self.index_col)].field.clone();
            let col_lhs = self.probe_field;
            let mut rhs: QueryExpr = self.index_side.into();

            if let Some(predicate) = self.index_select {
                rhs.query.push(predicate.into());
            }

            let fields = self
                .probe_side
                .source
                .head()
                .fields
                .iter()
                .cloned()
                .map(|Column { field, .. }| field.into())
                .collect();

            let table = self.probe_side.source.table_id();
            let source = self.probe_side.source;
            let inner_join = Query::JoinInner(JoinExpr::new(rhs, col_lhs, col_rhs));
            let project = Query::Project(fields, table);
            let query = vec![inner_join, project];
            QueryExpr { source, query }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct JoinExpr {
    pub rhs: QueryExpr,
    pub col_lhs: FieldName,
    pub col_rhs: FieldName,
}

impl JoinExpr {
    pub fn new(rhs: QueryExpr, col_lhs: FieldName, col_rhs: FieldName) -> Self {
        Self { rhs, col_lhs, col_rhs }
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
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CrudExpr {
    Query(QueryExpr),
    Insert {
        source: SourceExpr,
        rows: Vec<ProductValue>,
    },
    Update {
        delete: QueryExpr,
        assignments: HashMap<FieldName, FieldExpr>,
    },
    Delete {
        query: QueryExpr,
    },
    CreateTable {
        table: TableDef,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
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
        exprs.into_iter().all(|expr| matches!(expr, CrudExpr::Query(_)))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IndexScan {
    pub table: DbTable,
    pub columns: ColList,
    pub lower_bound: Bound<AlgebraicValue>,
    pub upper_bound: Bound<AlgebraicValue>,
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
    // The second argument is the table id for a qualified wildcard project.
    // If present, further optimzations are possible.
    Project(Vec<FieldExpr>, Option<TableId>),
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
pub enum IndexArgument {
    Eq {
        col_id: ColId,
        value: AlgebraicValue,
    },
    LowerBound {
        col_id: ColId,
        value: AlgebraicValue,
        inclusive: bool,
    },
    UpperBound {
        col_id: ColId,
        value: AlgebraicValue,
        inclusive: bool,
    },
}

// Sargable stands for Search ARGument ABLE.
// A sargable predicate is one that can be answered using an index.
fn is_sargable(table: &SourceExpr, op: &ColumnOp) -> Option<IndexArgument> {
    if let ColumnOp::Cmp {
        op: OpQuery::Cmp(op),
        lhs,
        rhs,
    } = op
    {
        // lhs must be a field
        let ColumnOp::Field(FieldExpr::Name(ref name)) = **lhs else {
            return None;
        };
        // rhs must be a value
        let ColumnOp::Field(FieldExpr::Value(ref value)) = **rhs else {
            return None;
        };
        // lhs field must exist
        let column = table.get_column_by_field(name)?;
        // lhs field must have an index
        if !table.head().has_constraint(&column.field, Constraints::indexed()) {
            return None;
        }

        match op {
            OpCmp::Eq => Some(IndexArgument::Eq {
                col_id: column.col_id,
                value: value.clone(),
            }),
            // a < 5 => exclusive upper bound
            OpCmp::Lt => Some(IndexArgument::UpperBound {
                col_id: column.col_id,
                value: value.clone(),
                inclusive: false,
            }),
            // a > 5 => exclusive lower bound
            OpCmp::Gt => Some(IndexArgument::LowerBound {
                col_id: column.col_id,
                value: value.clone(),
                inclusive: false,
            }),
            // a <= 5 => inclusive upper bound
            OpCmp::LtEq => Some(IndexArgument::UpperBound {
                col_id: column.col_id,
                value: value.clone(),
                inclusive: true,
            }),
            // a >= 5 => inclusive lower bound
            OpCmp::GtEq => Some(IndexArgument::LowerBound {
                col_id: column.col_id,
                value: value.clone(),
                inclusive: true,
            }),
            OpCmp::NotEq => None,
        }
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

    /// Does this query read from a given table?
    pub fn reads_from_table(&self, id: &TableId) -> bool {
        self.source.table_id() == Some(*id)
            || self.query.iter().any(|q| match q {
                Query::Select(_) | Query::Project(_, _) => false,
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
        // if this is the first operator in the list, generate index scan
        let Some(query) = self.query.pop() else {
            self.query.push(Query::IndexScan(IndexScan {
                table,
                columns,
                lower_bound: Bound::Included(value.clone()),
                upper_bound: Bound::Included(value),
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
                self = self.with_index_eq(db_table.clone(), columns, value);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_eq(table, columns, value),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        columns,
                        lower_bound: Bound::Included(value.clone()),
                        upper_bound: Bound::Included(value),
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        columns,
                        lower_bound: Bound::Included(value.clone()),
                        upper_bound: Bound::Included(value),
                    }
                    .into(),
                ));
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
            self.query.push(Query::IndexScan(IndexScan {
                table,
                columns,
                lower_bound: Self::bound(value, inclusive),
                upper_bound: Bound::Unbounded,
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
                self = self.with_index_lower_bound(table, columns, value, inclusive);
                self.query.push(query);
                self
            }
            // try to push below join's rhs
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_lower_bound(table, columns, value, inclusive),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding upper bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                lower_bound: Bound::Unbounded,
                upper_bound: Bound::Included(upper),
                ..
            }) if columns == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    columns,
                    lower_bound: Self::bound(value, inclusive),
                    upper_bound: Bound::Included(upper),
                }));
                self
            }
            // merge with a preceding upper bounded index scan (exclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                lower_bound: Bound::Unbounded,
                upper_bound: Bound::Excluded(upper),
                ..
            }) if columns == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    columns,
                    lower_bound: Self::bound(value, inclusive),
                    upper_bound: Bound::Excluded(upper),
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        columns,
                        lower_bound: Self::bound(value, inclusive),
                        upper_bound: Bound::Unbounded,
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        columns,
                        lower_bound: Self::bound(value, inclusive),
                        upper_bound: Bound::Unbounded,
                    }
                    .into(),
                ));
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
                lower_bound: Bound::Unbounded,
                upper_bound: Self::bound(value, inclusive),
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
            Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }) => {
                self.query.push(Query::JoinInner(JoinExpr {
                    rhs: rhs.with_index_upper_bound(table, columns, value, inclusive),
                    col_lhs,
                    col_rhs,
                }));
                self
            }
            // merge with a preceding lower bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                lower_bound: Bound::Included(lower),
                upper_bound: Bound::Unbounded,
                ..
            }) if columns == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    columns,
                    lower_bound: Bound::Included(lower),
                    upper_bound: Self::bound(value, inclusive),
                }));
                self
            }
            // merge with a preceding lower bounded index scan (inclusive)
            Query::IndexScan(IndexScan {
                columns: lhs_col_id,
                lower_bound: Bound::Excluded(lower),
                upper_bound: Bound::Unbounded,
                ..
            }) if columns == lhs_col_id => {
                self.query.push(Query::IndexScan(IndexScan {
                    table,
                    columns,
                    lower_bound: Bound::Excluded(lower),
                    upper_bound: Self::bound(value, inclusive),
                }));
                self
            }
            // merge with a preceding select
            Query::Select(filter) => {
                self.query.push(Query::Select(ColumnOp::Cmp {
                    op: OpQuery::Logic(OpLogic::And),
                    lhs: filter.into(),
                    rhs: IndexScan {
                        table,
                        columns,
                        lower_bound: Bound::Unbounded,
                        upper_bound: Self::bound(value, inclusive),
                    }
                    .into(),
                }));
                self
            }
            // else generate a new select
            query => {
                self.query.push(query);
                self.query.push(Query::Select(
                    IndexScan {
                        table,
                        columns,
                        lower_bound: Bound::Unbounded,
                        upper_bound: Self::bound(value, inclusive),
                    }
                    .into(),
                ));
                self
            }
        }
    }

    pub fn with_select<O>(mut self, op: O) -> Self
    where
        O: Into<ColumnOp>,
    {
        let Some(query) = self.query.pop() else {
            self.query.push(Query::Select(op.into()));
            return self;
        };

        match (query, op.into()) {
            (
                Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }),
                ColumnOp::Cmp {
                    op: OpQuery::Cmp(cmp),
                    lhs: field,
                    rhs: value,
                },
            ) => match (*field, *value) {
                (ColumnOp::Field(FieldExpr::Name(field)), ColumnOp::Field(FieldExpr::Value(value)))
                    // Field is from lhs, so push onto join's left arg
                    if self.source.head().column(&field).is_some() =>
                {
                    self = self.with_select(ColumnOp::cmp(field, cmp, value));
                    self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }));
                    self
                }
                (ColumnOp::Field(FieldExpr::Name(field)), ColumnOp::Field(FieldExpr::Value(value)))
                    // Field is from rhs, so push onto join's right arg
                    if rhs.source.head().column(&field).is_some() =>
                {
                    self.query.push(Query::JoinInner(JoinExpr {
                        rhs: rhs.with_select(ColumnOp::cmp(field, cmp, value)),
                        col_lhs,
                        col_rhs,
                    }));
                    self
                }
                (field, value) => {
                    self.query.push(Query::JoinInner(JoinExpr { rhs, col_lhs, col_rhs }));
                    self.query.push(Query::Select(ColumnOp::new(OpQuery::Cmp(cmp), field, value)));
                    self
                }
            },
            (Query::Select(filter), op) => {
                self.query
                    .push(Query::Select(ColumnOp::new(OpQuery::Logic(OpLogic::And), filter, op)));
                self
            }
            (query, op) => {
                self.query.push(query);
                self.query.push(Query::Select(op));
                self
            }
        }
    }

    pub fn with_select_cmp<LHS, RHS, O>(self, op: O, lhs: LHS, rhs: RHS) -> Self
    where
        LHS: Into<FieldExpr>,
        RHS: Into<FieldExpr>,
        O: Into<OpQuery>,
    {
        let op = ColumnOp::new(op.into(), ColumnOp::Field(lhs.into()), ColumnOp::Field(rhs.into()));
        self.with_select(op)
    }

    // Appends a project operation to the query operator pipeline.
    // The `wildcard_table_id` represents a projection of the form `table.*`.
    // This is used to determine if an inner join can be rewritten as an index join.
    pub fn with_project(self, cols: &[FieldExpr], wildcard_table_id: Option<TableId>) -> Self {
        let mut x = self;
        if !cols.is_empty() {
            x.query.push(Query::Project(cols.into(), wildcard_table_id));
        }
        x
    }

    pub fn with_join_inner(self, with: impl Into<QueryExpr>, lhs: FieldName, rhs: FieldName) -> Self {
        let mut x = self;
        x.query.push(Query::JoinInner(JoinExpr::new(with.into(), lhs, rhs)));
        x
    }

    fn bound(value: AlgebraicValue, inclusive: bool) -> Bound<AlgebraicValue> {
        if inclusive {
            Bound::Included(value)
        } else {
            Bound::Excluded(value)
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
        // We expect 2 and only 2 operations - a join followed by a wildcard projection.
        if query.query.len() != 2 {
            return query;
        }

        // If the source is a `MemTable`, it doesn't have any indexes,
        // so we can't plan an index join.
        let Some(source_table_id) = query.source.table_id() else {
            return query;
        };
        let source = query.source;
        let second = query.query.pop().unwrap();
        let first = query.query.pop().unwrap();

        // An applicable join must be followed by a wildcard projection.
        let Query::Project(_, Some(wildcard_table_id)) = second else {
            return QueryExpr {
                source,
                query: vec![first, second],
            };
        };

        match first {
            Query::JoinInner(JoinExpr {
                rhs: probe_side,
                col_lhs: index_field,
                col_rhs: probe_field,
            }) => {
                if !probe_side.query.is_empty() && wildcard_table_id == source_table_id {
                    // An applicable join must have an index defined on the correct field.
                    if let Some(col) = source.head().column(&index_field) {
                        let index_col = col.col_id;
                        if source.head().has_constraint(&index_field, Constraints::indexed()) {
                            let index_join = IndexJoin {
                                probe_side,
                                probe_field,
                                index_side: source.clone(),
                                index_select: None,
                                index_col,
                                return_index_rows: true,
                            };
                            return QueryExpr {
                                source,
                                query: vec![Query::IndexJoin(index_join)],
                            };
                        }
                    }
                }
                let first = Query::JoinInner(JoinExpr {
                    rhs: probe_side,
                    col_lhs: index_field,
                    col_rhs: probe_field,
                });
                QueryExpr {
                    source,
                    query: vec![first, second],
                }
            }
            first => QueryExpr {
                source,
                query: vec![first, second],
            },
        }
    }

    /// Look for filters that could use indexes
    fn optimize_select(mut q: QueryExpr, op: ColumnOp, tables: &[SourceExpr]) -> QueryExpr {
        'outer: for ref op in op.to_vec() {
            // Go through each table schema referenced in the query.
            // Find the first sargable condition and short-circuit.
            for schema in tables {
                match is_sargable(schema, op) {
                    // found sargable equality condition for one of the table schemas
                    Some(IndexArgument::Eq { col_id, value }) => {
                        // `unwrap`  here is infallible because `is_sargable(schema, op)` implies `schema.is_db_table`
                        // for any `op`.
                        q = q.with_index_eq(schema.get_db_table().unwrap().clone(), col_id.into(), value);
                        continue 'outer;
                    }
                    // found sargable range condition for one of the table schemas
                    Some(IndexArgument::LowerBound {
                        col_id,
                        value,
                        inclusive,
                    }) => {
                        q = q.with_index_lower_bound(
                            // `unwrap`  here is infallible because `is_sargable(schema, op)` implies `schema.is_db_table`
                            // for any `op`.
                            schema.get_db_table().unwrap().clone(),
                            col_id.into(),
                            value,
                            inclusive,
                        );
                        continue 'outer;
                    }
                    // found sargable range condition for one of the table schemas
                    Some(IndexArgument::UpperBound {
                        col_id,
                        value,
                        inclusive,
                    }) => {
                        q = q.with_index_upper_bound(
                            // `unwrap`  here is infallible because `is_sargable(schema, op)` implies `schema.is_db_table`
                            // for any `op`.
                            schema.get_db_table().unwrap().clone(),
                            col_id.into(),
                            value,
                            inclusive,
                        );
                        continue 'outer;
                    }
                    None => {}
                }
            }
            // filter condition cannot be answered using an index
            q = q.with_select(op.clone());
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

        if self.query.len() == 1 && matches!(self.query[0], Query::IndexJoin(_)) {
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
                    q = q.with_join_inner(join.rhs.optimize(row_count), join.col_lhs, join.col_rhs)
                }
                _ => q.query.push(query),
            };
        }

        let q = q.try_index_join();
        if q.query.len() == 1 && matches!(q.query[0], Query::IndexJoin(_)) {
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

#[derive(Debug, Clone, Eq, PartialEq, From)]
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

impl fmt::Display for SourceExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceExpr::MemTable { header, source_id, .. } => {
                let ty = AlgebraicType::Product(header.ty());
                write!(f, "SourceExpr({source_id:?} => virtual {ty:?})")
            }
            SourceExpr::DbTable(x) => {
                write!(f, "DbTable({})", x.table_id)
            }
        }
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
            Query::Project(q, _) => {
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

#[derive(Debug, Clone, PartialEq, Eq)]
// TODO(bikeshedding): Refactor this struct so that `IndexJoin`s replace the `table`,
// rather than appearing as the first element of the `query`.
//
// `IndexJoin`s do not behave like filters; in fact they behave more like data sources.
// A query conceptually starts with either a single table or an `IndexJoin`,
// and then stacks a set of filters on top of that.
pub struct QueryCode {
    pub table: SourceExpr,
    pub query: Vec<Query>,
}

impl From<QueryExpr> for QueryCode {
    fn from(value: QueryExpr) -> Self {
        QueryCode {
            table: value.source,
            query: value.query,
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

impl AuthAccess for Table {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller || self.table_access() == StAccess::Public {
            return Ok(());
        }

        Err(AuthError::TablePrivate {
            named: self.table_name().to_string(),
        })
    }
}

impl AuthAccess for QueryCode {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        self.table.check_auth(owner, caller)?;
        for q in &self.query {
            q.check_auth(owner, caller)?;
        }

        Ok(())
    }
}

impl Relation for QueryCode {
    fn head(&self) -> &Arc<Header> {
        self.table.head()
    }

    fn row_count(&self) -> RowCount {
        self.table.row_count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrudCode {
    Query(QueryCode),
    Insert {
        table: SourceExpr,
        rows: Vec<ProductValue>,
    },
    Update {
        delete: QueryCode,
        assignments: HashMap<FieldName, FieldExpr>,
    },
    Delete {
        query: QueryCode,
    },
    CreateTable {
        table: TableDef,
    },
    Drop {
        name: String,
        kind: DbType,
        table_access: StAccess,
    },
}

impl AuthAccess for CrudCode {
    fn check_auth(&self, owner: Identity, caller: Identity) -> Result<(), AuthError> {
        if owner == caller {
            return Ok(());
        }
        // Anyone may query, so as long as the tables involved are public.
        if let CrudCode::Query(q) = self {
            return q.check_auth(owner, caller);
        }

        // Mutating operations require `owner == caller`.
        Err(AuthError::OwnerRequired)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Code {
    Value(AlgebraicValue),
    Table(MemTable),
    Halt(ErrorLang),
    Block(Vec<Code>),
    Crud(CrudCode),
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

#[derive(Debug, Clone, PartialEq)]
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

    const ALICE: Identity = Identity::from_byte_array([1; 32]);
    const BOB: Identity = Identity::from_byte_array([2; 32]);

    // TODO(kim): Should better do property testing here, but writing generators
    // on recursive types (ie. `Query` and friends) is tricky.

    fn tables() -> [SourceExpr; 2] {
        [
            SourceExpr::MemTable {
                source_id: SourceId(0),
                header: Arc::new(Header {
                    table_name: "foo".into(),
                    fields: vec![],
                    constraints: Default::default(),
                }),
                row_count: RowCount::unknown(),
                table_type: StTableType::User,
                table_access: StAccess::Private,
            },
            SourceExpr::DbTable(DbTable {
                head: Arc::new(Header {
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
                lower_bound: Bound::Included(22.into()),
                upper_bound: Bound::Unbounded,
            }),
            Query::IndexJoin(IndexJoin {
                probe_side: mem_table.clone().into(),
                probe_field: FieldName::Name {
                    table: "foo".into(),
                    field: "bar".into(),
                },
                index_side: SourceExpr::DbTable(DbTable {
                    head: Arc::new(Header {
                        table_name: "bar".into(),
                        fields: vec![],
                        constraints: Default::default(),
                    }),
                    table_id: 42.into(),
                    table_type: StTableType::User,
                    table_access: StAccess::Public,
                }),
                index_select: None,
                index_col: 22.into(),
                return_index_rows: true,
            }),
            Query::JoinInner(JoinExpr {
                rhs: mem_table.into(),
                col_rhs: FieldName::Name {
                    table: "foo".into(),
                    field: "id".into(),
                },
                col_lhs: FieldName::Name {
                    table: "bar".into(),
                    field: "id".into(),
                },
            }),
        ]
    }

    fn query_codes() -> impl IntoIterator<Item = QueryCode> {
        tables().map(|table| {
            let expr = QueryExpr::from(table);
            let mut code = QueryCode::from(expr);
            code.query = queries().into_iter().collect();
            code
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

    fn mem_table(name: &str, fields: &[(&str, AlgebraicType, bool)]) -> SourceExpr {
        let table_access = StAccess::Public;
        let head = Header::new(
            name.into(),
            fields
                .iter()
                .enumerate()
                .map(|(i, (field, ty, _))| Column::new(FieldName::named(name, field), ty.clone(), i.into()))
                .collect(),
            fields
                .iter()
                .enumerate()
                .filter(|(_, (_, _, indexed))| *indexed)
                .map(|(i, _)| (ColId(i as u32).into(), Constraints::indexed()))
                .collect(),
        );
        SourceExpr::MemTable {
            source_id: SourceId(0),
            header: Arc::new(head),
            row_count: RowCount::unknown(),
            table_access,
            table_type: StTableType::User,
        }
    }

    #[test]
    fn test_index_to_inner_join() {
        let index_side = mem_table(
            "index",
            &[("a", AlgebraicType::U8, false), ("b", AlgebraicType::U8, true)],
        );
        let probe_side = mem_table(
            "probe",
            &[("c", AlgebraicType::U8, false), ("b", AlgebraicType::U8, true)],
        );

        let probe_field = probe_side.head().fields[1].field.clone();
        let select_field = FieldName::Name {
            table: "index".into(),
            field: "a".into(),
        };
        let index_select = ColumnOp::cmp(select_field, OpCmp::Eq, 0.into());
        let join = IndexJoin {
            probe_side: probe_side.clone().into(),
            probe_field,
            index_side: index_side.clone(),
            index_select: Some(index_select.clone()),
            index_col: 1.into(),
            return_index_rows: false,
        };

        let expr = join.to_inner_join();

        assert_eq!(expr.source, probe_side);
        assert_eq!(expr.query.len(), 2);

        let Query::JoinInner(ref join) = expr.query[0] else {
            panic!("expected an inner join, but got {:#?}", expr.query[0]);
        };

        assert_eq!(join.col_lhs, FieldName::named("probe", "b"));
        assert_eq!(join.col_rhs, FieldName::named("index", "b"));
        assert_eq!(
            join.rhs,
            QueryExpr {
                source: index_side,
                query: vec![index_select.into()]
            }
        );

        let Query::Project(ref fields, None) = expr.query[1] else {
            panic!("expected a projection, but got {:#?}", expr.query[1]);
        };

        assert_eq!(
            fields,
            &vec![
                FieldName::named("probe", "c").into(),
                FieldName::named("probe", "b").into()
            ]
        );
    }

    #[test]
    fn test_auth_table() {
        tables().iter().for_each(assert_owner_private)
    }

    #[test]
    fn test_auth_query_code() {
        for code in query_codes() {
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
        for query in query_codes() {
            let crud = CrudCode::Query(query);
            assert_owner_private(&crud);
        }
    }

    #[test]
    fn test_auth_crud_code_insert() {
        for table in tables() {
            let crud = CrudCode::Insert { table, rows: vec![] };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_update() {
        for qc in query_codes() {
            let crud = CrudCode::Update {
                delete: qc,
                assignments: Default::default(),
            };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_delete() {
        for query in query_codes() {
            let crud = CrudCode::Delete { query };
            assert_owner_required(crud);
        }
    }

    #[test]
    fn test_auth_crud_code_create_table() {
        let table = TableDef::new("etcpasswd".into(), vec![])
            .with_access(StAccess::Public)
            .with_type(StTableType::System); // hah!

        let crud = CrudCode::CreateTable { table };
        assert_owner_required(crud);
    }

    #[test]
    fn test_auth_crud_code_drop() {
        let crud = CrudCode::Drop {
            name: "etcpasswd".into(),
            kind: DbType::Table,
            table_access: StAccess::Public,
        };
        assert_owner_required(crud);
    }
}
