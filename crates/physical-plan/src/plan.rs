use std::{
    ops::{Bound, Deref},
    sync::Arc,
};

use spacetimedb_expr::{ty::Symbol, StatementSource};
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColList, IndexId};
use spacetimedb_schema::{def::ConstraintData, schema::TableSchema};
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
    Proj(Box<PhysicalPlan>, Symbol),
}

impl PhysicalPlan {
    /// Applies a transformation to every operation in this plan
    pub fn map(self, mut f: impl FnMut(Self) -> Self) -> Self {
        match self {
            Self::TableScan(..) | Self::IxScan(..) => f(self),
            Self::NLJoin(lhs, rhs) => {
                let lhs = lhs.map(&mut f);
                let rhs = rhs.map(&mut f);
                let lhs = Box::new(lhs);
                let rhs = Box::new(rhs);
                f(Self::NLJoin(lhs, rhs))
            }
            Self::IxJoin(join) => {
                let lhs = join.lhs.map(&mut f);
                let lhs = Box::new(lhs);
                f(Self::IxJoin(IxJoin { lhs, ..join }))
            }
            Self::IxSemiJoin(join) => {
                let lhs = join.lhs.map(&mut f);
                let lhs = Box::new(lhs);
                f(Self::IxSemiJoin(IxSemiJoin { lhs, ..join }))
            }
            Self::Filter(arg, expr) => {
                let arg = arg.map(&mut f);
                let arg = Box::new(arg);
                f(Self::Filter(arg, expr))
            }
            Self::Project(arg, expr) => {
                let arg = arg.map(&mut f);
                let arg = Box::new(arg);
                f(Self::Project(arg, expr))
            }
            Self::Proj(arg, field) => {
                let arg = arg.map(&mut f);
                let arg = Box::new(arg);
                f(Self::Proj(arg, field))
            }
        }
    }

    /// Applies a rewrite rule to this plan.
    /// Updates indicator variable if plan was modified.
    pub fn apply<R: RewriteRule>(self, ok: &mut bool) -> Self {
        if R::matches(&self) {
            *ok = true;
            return R::rewrite(self);
        }
        self
    }

    /// Applies rewrite rules until a fixpoint is reached
    pub fn optimize(self) -> Self {
        let mut plan = self;
        let mut ok = false;

        plan = plan.apply::<PushProjection>(&mut ok);
        plan = plan.apply::<IxSemiJoinRule>(&mut ok);
        plan = plan.apply::<UniqueIxJoinRule>(&mut ok);

        if ok {
            plan.optimize()
        } else {
            plan
        }
    }
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
    pub lhs: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub rhs: Arc<TableSchema>,
    /// The rhs field name
    pub rhs_label: Symbol,
    /// The index id
    pub index_id: IndexId,
    /// The index fields
    pub index_cols: ColList,
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
    pub lhs: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub rhs: Arc<TableSchema>,
    /// The index id
    pub index_id: IndexId,
    /// The index fields
    pub index_cols: ColList,
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

impl IxSemiJoin {
    pub fn from(join: IxJoin, proj: SemiJoinProj) -> Self {
        let IxJoin {
            lhs,
            rhs,
            rhs_label: _,
            index_id,
            index_cols,
            unique,
            index_key_expr,
        } = join;
        Self {
            lhs,
            rhs,
            index_id,
            index_cols,
            unique,
            index_key_expr,
            proj,
        }
    }
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
#[derive(Debug, Clone)]
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

pub trait RewriteRule {
    fn matches(plan: &PhysicalPlan) -> bool;
    fn rewrite(plan: PhysicalPlan) -> PhysicalPlan;
}

/// Index (semi)join => unique index (semi)join
pub struct UniqueIxJoinRule;

impl RewriteRule for UniqueIxJoinRule {
    fn matches(plan: &PhysicalPlan) -> bool {
        match plan {
            PhysicalPlan::IxJoin(IxJoin {
                unique: false,
                rhs,
                index_cols,
                ..
            })
            | PhysicalPlan::IxSemiJoin(IxSemiJoin {
                unique: false,
                rhs,
                index_cols,
                ..
            }) => rhs
                // Does the rhs table have a unique constraint?
                .constraints
                .iter()
                .any(|cs| match &cs.data {
                    ConstraintData::Unique(data) => {
                        // For the index columns?
                        data.columns.deref() == index_cols
                    }
                    _ => false,
                }),
            _ => false,
        }
    }

    fn rewrite(mut plan: PhysicalPlan) -> PhysicalPlan {
        if let PhysicalPlan::IxJoin(IxJoin {
            // If invoking this rewrite,
            // we know unique is false.
            unique,
            ..
        })
        | PhysicalPlan::IxSemiJoin(IxSemiJoin {
            // If invoking this rewrite,
            // we know unique is false.
            unique,
            ..
        }) = &mut plan
        {
            *unique = true;
        }
        plan
    }
}

/// Index join + projection => index semijoin
pub struct IxSemiJoinRule;

impl RewriteRule for IxSemiJoinRule {
    fn matches(plan: &PhysicalPlan) -> bool {
        if let PhysicalPlan::Proj(join, _) = plan {
            return matches!(**join, PhysicalPlan::IxJoin(..));
        }
        false
    }

    fn rewrite(plan: PhysicalPlan) -> PhysicalPlan {
        if let PhysicalPlan::Proj(join, field) = plan {
            if let PhysicalPlan::IxJoin(join @ IxJoin { rhs_label, .. }) = *join {
                return PhysicalPlan::IxSemiJoin(IxSemiJoin::from(
                    join,
                    if field == rhs_label {
                        SemiJoinProj::Rhs
                    } else {
                        SemiJoinProj::Lhs
                    },
                ));
            }
        }
        unreachable!()
    }
}

/// Push projections down towards the leaves.
/// Required for semijoin rewrites.
pub struct PushProjection;

impl RewriteRule for PushProjection {
    fn matches(_plan: &PhysicalPlan) -> bool {
        unimplemented!()
    }

    fn rewrite(_plan: PhysicalPlan) -> PhysicalPlan {
        unimplemented!()
    }
}
