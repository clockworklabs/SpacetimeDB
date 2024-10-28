use std::{ops::Bound, sync::Arc};

use spacetimedb_expr::ty::TyId;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, IndexId};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;

/// A physical plan is a concrete query evaluation strategy.
/// As such, we can reason about its energy consumption.
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(Arc<TableSchema>),
    /// Fetch and return row ids from an index
    IndexScan(IndexScan),
    /// Join an input relation with a base table using an index
    IndexJoin(IndexJoin),
    /// An index join + projection
    IndexSemiJoin(IndexSemiJoin),
    /// Return the cross product of two input relations
    CrossJoin(CrossJoin),
    /// Filter an input relation row by row
    Filter(Filter),
    /// Transform an input relation row by row
    Project(Project),
}

/// Fetch and return row ids from a btree index
pub struct IndexScan {
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
pub enum IndexOp {
    Eq(AlgebraicValue, TyId),
    Range(Bound<AlgebraicValue>, Bound<AlgebraicValue>, TyId),
}

/// Join an input relation with a base table using an index.
/// Returns a 2-tuple of its lhs and rhs input rows.
pub struct IndexJoin {
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
    /// The return type of this index join.
    /// Always a 2-tuple of its input types.
    pub ty: TyId,
}

/// An index join + projection.
/// Returns tuples from the lhs (or rhs) exclusively.
pub struct IndexSemiJoin {
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
pub enum SemiJoinProj {
    Lhs,
    Rhs,
}

/// Returns the cross product of two input relations.
/// Returns a 2-tuple of its lhs and rhs input rows.
pub struct CrossJoin {
    /// The lhs input relation
    pub lhs: Box<PhysicalPlan>,
    /// The rhs input relation
    pub rhs: Box<PhysicalPlan>,
    /// The type of this cross product.
    /// Always a 2-tuple of its input types.
    pub ty: TyId,
}

/// A streaming or non-leaf filter operation
pub struct Filter {
    /// A generic filter always has an input
    pub input: Box<PhysicalPlan>,
    /// The boolean expression for selecting tuples
    pub op: PhysicalExpr,
}

/// A streaming project or map operation
pub struct Project {
    /// A projection always has an input
    pub input: Box<PhysicalPlan>,
    /// The tuple transformation expression.
    /// It will always produce another tuple.
    pub op: PhysicalExpr,
}

/// A physical scalar expression
pub enum PhysicalExpr {
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A tuple expression
    Tuple(Vec<PhysicalExpr>, TyId),
    /// A constant algebraic value
    Value(AlgebraicValue, TyId),
    /// A field projection expression
    Field(Box<PhysicalExpr>, usize, TyId),
    /// The input tuple to a relop
    Input(TyId),
}
