use spacetimedb_expr::ty::TyId;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, IndexId};
use spacetimedb_schema::schema::{ColumnSchema, IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::BinOp;
use std::{ops::Bound, sync::Arc};

pub struct Index<'a> {
    pub index: &'a IndexSchema,
    pub is_unique: bool,
    pub col: ColId,
}

pub enum Tables<'a> {
    Table(&'a Arc<TableSchema>),
    Join(Box<Tables<'a>>, Box<Tables<'a>>),
}

impl<'a> Tables<'a> {
    pub fn lhs(&self) -> &'a Arc<TableSchema> {
        match self {
            Tables::Table(schema) => schema,
            Tables::Join(lhs, _) => lhs.lhs(),
        }
    }

    pub fn rhs(&self) -> &'a Arc<TableSchema> {
        match self {
            Tables::Table(schema) => schema,
            Tables::Join(_, rhs) => rhs.rhs(),
        }
    }

    pub fn column_lhs(&self, col: usize) -> Option<&'a ColumnSchema> {
        self.lhs().get_column(col)
    }

    pub fn column_rhs(&self, col: usize) -> Option<&'a ColumnSchema> {
        self.rhs().get_column(col)
    }

    fn _find_index(&self, table_schema: &'a TableSchema, col: ColId) -> Option<Index<'a>> {
        table_schema
            .indexes
            .iter()
            .find(|idx| {
                let cols = idx.index_algorithm.columns();
                cols.len() == 1 && cols.contains(col)
            })
            .map(|index| Index {
                index,
                col,
                is_unique: table_schema.constraints.iter().any(|ct| {
                    ct.data
                        .unique_columns()
                        .map_or(false, |cols| cols.len() == 1 && cols.contains(col))
                }),
            })
    }

    pub fn index_lhs(&self, col: usize) -> (&'a Arc<TableSchema>, Option<Index<'a>>) {
        match self {
            Tables::Table(schema) => (schema, self._find_index(schema, col.into())),
            Tables::Join(lhs, _) => lhs.index_lhs(col),
        }
    }

    pub fn index_rhs(&self, col: ColId) -> (&'a Arc<TableSchema>, Option<Index<'a>>) {
        match self {
            Tables::Table(schema) => (schema, self._find_index(schema, col)),
            Tables::Join(_, rhs) => rhs.index_rhs(col),
        }
    }
}

/// A physical plan is a concrete query evaluation strategy.
/// As such, we can reason about its energy consumption.
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(Arc<TableSchema>, TyId),
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

#[cfg(test)]
impl PhysicalPlan {
    pub fn as_project(&self) -> Option<&Project> {
        if let PhysicalPlan::Project(p) = self {
            Some(p)
        } else {
            None
        }
    }

    pub fn as_filter(&self) -> Option<&Filter> {
        if let PhysicalPlan::Filter(p) = self {
            Some(p)
        } else {
            None
        }
    }

    pub fn as_index_scan(&self) -> Option<&IndexScan> {
        if let PhysicalPlan::IndexScan(p) = self {
            Some(p)
        } else {
            None
        }
    }

    pub fn as_cross(&self) -> Option<&CrossJoin> {
        if let PhysicalPlan::CrossJoin(p) = self {
            Some(p)
        } else {
            None
        }
    }

    pub fn as_table_scan(&self) -> Option<&Arc<TableSchema>> {
        if let PhysicalPlan::TableScan(p, _) = self {
            Some(p)
        } else {
            None
        }
    }

    pub fn table_schema(&self) -> Tables {
        match self {
            PhysicalPlan::TableScan(schema, _) => Tables::Table(schema),
            PhysicalPlan::IndexScan(scan) => Tables::Table(&scan.table_schema),
            PhysicalPlan::IndexJoin(join) => Tables::Table(&join.table),
            PhysicalPlan::IndexSemiJoin(join) => Tables::Table(&join.table),
            PhysicalPlan::CrossJoin(join) => {
                Tables::Join(Box::new(join.lhs.table_schema()), Box::new(join.rhs.table_schema()))
            }
            PhysicalPlan::Filter(x) => x.input.table_schema(),
            PhysicalPlan::Project(x) => x.input.table_schema(),
        }
    }

    pub fn bin_op(of: Self, op: BinOp, lhs: PhysicalExpr, rhs: PhysicalExpr) -> Self {
        PhysicalPlan::Filter(Filter {
            input: Box::new(of),
            op: PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs)),
        })
    }

    pub fn project(input: PhysicalPlan, op: PhysicalExpr) -> Self {
        PhysicalPlan::Project(Project {
            input: Box::new(input),
            op,
        })
    }
}

/// Fetch and return row ids from a btree index
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum IndexOp {
    Eq(AlgebraicValue, TyId),
    Range(Bound<AlgebraicValue>, Bound<AlgebraicValue>, TyId),
}

/// Join an input relation with a base table using an index.
/// Returns a 2-tuple of its lhs and rhs input rows.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SemiJoinProj {
    Lhs,
    Rhs,
}

/// Returns the cross product of two input relations.
/// Returns a 2-tuple of its lhs and rhs input rows.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// A generic filter always has an input
    pub input: Box<PhysicalPlan>,
    /// The boolean expression for selecting tuples
    pub op: PhysicalExpr,
}

/// A streaming project or map operation
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// A projection always has an input
    pub input: Box<PhysicalPlan>,
    /// The tuple transformation expression.
    /// It will always produce another tuple.
    pub op: PhysicalExpr,
}

/// A physical scalar expression
#[derive(Debug, Clone, PartialEq)]
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

/// A physical context for the result of a query compilation.
pub struct PhysicalCtx<'a> {
    pub plan: PhysicalPlan,
    pub sql: &'a str,
    pub source: StatementSource,
}

impl<'a> PhysicalCtx<'a> {
    pub fn into_parts(self) -> (PhysicalPlan, &'a str, StatementSource) {
        (self.plan, self.sql, self.source)
    }

    pub fn print_plan(&self) {
        println!("{}", self);
    }
}
