use std::{ops::Bound, sync::Arc};

use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, IndexId};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;

/// A physical plan is a concrete evaluation strategy.
/// As such, we can reason about its energy consumption.
///
/// Types are encoded in the structure of the plan,
/// rather than made explicit as for the logical plan.
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(Arc<TableSchema>),
    /// Fetch row ids from an index
    IxScan(IxScan),
    /// Join a relation to a table using an index
    IxJoin(IxJoin),
    /// An index join + projection
    IxSemiJoin(IxSemiJoin),
    /// A Nested Loop Join.
    /// Equivalent to a cross product.
    ///
    /// 1) If the lhs relation has `n` tuples
    /// 2) If the rhs relation has `m` tuples
    ///
    /// Then a nested loop join returns `n * m` tuples,
    /// which is also its asymptotic complexity.
    NLJoin(Box<PhysicalPlan>, Box<PhysicalPlan>),
    /// A tuple-at-a-time filter
    Filter(Box<PhysicalPlan>, PhysicalExpr),
    /// A tuple-at-a-time projection
    Project(Box<PhysicalPlan>, PhysicalExpr),
}

/// Fetch and return row ids from a btree index
pub struct IxScan {
    /// The table on which this index is defined
    pub table_schema: Arc<TableSchema>,
    /// The index id
    pub index_id: IndexId,
    /// Is this index unique?
    /// Does it uniquely identify the rows?
    pub unique: bool,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<(ColId, AlgebraicValue)>,
    /// The range column
    pub col: ColId,
    /// Equality or range scan?
    pub op: IndexOp,
}

/// BTrees support equality and range scans
#[derive(Debug)]
pub enum IndexOp {
    Eq(AlgebraicValue),
    Range(Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

/// An index join.
/// Joins a relation to a base table using an index.
///
/// 1) If the input relation has `n` tuples
/// 2) If the base table has `m` rows
/// 3) If the complexity of an index lookup is f(m)
///
/// Then the complexity of the index join is `n * f(m)`
pub struct IxJoin {
    /// The lhs input used to probe the index
    pub input: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub table: Arc<TableSchema>,
    /// The rhs index
    pub index: IndexId,
    /// Is the index unique?
    /// Does it uniquely identify the rows?
    pub unique: bool,
    /// The expression that derives index keys from the lhs.
    /// It is evaluated over each row from the lhs.
    /// The resulting value is used to probe the index.
    pub index_key_expr: PhysicalExpr,
}

/// An index semijoin.
/// I.e. an index join + projection.
/// Same asymptotic complexity as [IxJoin].
pub struct IxSemiJoin {
    /// The lhs input used to probe the index
    pub input: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub table: Arc<TableSchema>,
    /// The rhs index
    pub index: IndexId,
    /// Is the index unique?
    /// Does it uniquely identify the rows?
    pub unique: bool,
    /// The expression that derives index keys from the lhs.
    /// It is evaluated over each row from the lhs.
    /// The resulting value is used to probe the index.
    pub index_key_expr: PhysicalExpr,
    /// Which side of the semijoin to project
    pub proj: SemiJoinProj,
}

/// Which side of a semijoin to project?
#[derive(Debug)]
pub enum SemiJoinProj {
    Lhs,
    Rhs,
}

/// A physical scalar expression.
///
/// Types are encoded in the structure of the plan,
/// rather than made explicit as for the logical plan.
pub enum PhysicalExpr {
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A constant algebraic value.
    /// Type already encoded in value.
    Value(AlgebraicValue),
    /// A tuple constructor
    Tuple(Vec<PhysicalExpr>),
    /// A field projection expression.
    Field(Box<PhysicalExpr>, usize),
    /// A pointer to a row in a table.
    /// A base element for a field projection.
    Ptr,
    /// A reference to a product value.
    /// A base element for a field projection.
    Ref,
    /// A temporary tuple value.
    /// A base element for a field projection.
    Tup,
}

/// A physical context for the result of a query compilation.
pub struct PhysicalCtx<'a> {
    pub plan: PhysicalPlan,
    pub sql: &'a str,
    pub source: StatementSource,
}
