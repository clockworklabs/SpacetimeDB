use std::{ops::Bound, sync::Arc};

use derive_more::From;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::{IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

/// Table aliases are replaced with labels in the physical plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, From)]
pub struct Label(pub usize);

/// Physical query plans always terminate with a projection
#[derive(Debug, PartialEq, Eq)]
pub enum PhysicalProject {
    None(PhysicalPlan),
    Relvar(PhysicalPlan, Label),
    Fields(PhysicalPlan, Vec<(Box<str>, ProjectField)>),
}

impl PhysicalProject {
    pub fn optimize(self) -> Self {
        match self {
            Self::None(plan) => Self::None(plan.optimize(vec![])),
            Self::Relvar(plan, var) => Self::None(plan.optimize(vec![var])),
            Self::Fields(plan, fields) => {
                Self::Fields(plan.optimize(fields.iter().map(|(_, proj)| proj.var).collect()), fields)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectField {
    pub var: Label,
    pub pos: usize,
}

/// A physical plan represents a concrete evaluation strategy.
#[derive(Debug, PartialEq, Eq)]
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(Arc<TableSchema>, Label),
    /// Fetch row ids from an index
    IxScan(IxScan, Label),
    /// An index join + projection
    IxJoin(IxJoin, Semi),
    /// An hash join + projection
    HashJoin(HashJoin, Semi),
    /// A nested loop join
    NLJoin(Box<PhysicalPlan>, Box<PhysicalPlan>),
    /// A tuple-at-a-time filter
    Filter(Box<PhysicalPlan>, PhysicalExpr),
}

impl PhysicalPlan {
    /// Walks the plan tree and calls `f` on every op
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) => {
                input.visit(f);
            }
            Self::NLJoin(lhs, rhs) | Self::HashJoin(HashJoin { lhs, rhs, .. }, _) => {
                lhs.visit(f);
                rhs.visit(f);
            }
            _ => {}
        }
    }

    /// Is there any subplan where `f` returns true?
    pub fn any(&self, f: &impl Fn(&Self) -> bool) -> bool {
        let mut ok = false;
        self.visit(&mut |plan| {
            ok = ok || f(plan);
        });
        ok
    }

    /// Applies `f` recursively to all subplans
    pub fn map(self, f: &impl Fn(Self) -> Self) -> Self {
        match f(self) {
            Self::Filter(input, expr) => Self::Filter(Box::new(input.map(f)), expr),
            Self::NLJoin(lhs, rhs) => Self::NLJoin(Box::new(lhs.map(f)), Box::new(rhs.map(f))),
            Self::HashJoin(join, semi) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.map(f)),
                    rhs: Box::new(join.rhs.map(f)),
                    ..join
                },
                semi,
            ),
            Self::IxJoin(join, semi) => Self::IxJoin(
                IxJoin {
                    lhs: Box::new(join.lhs.map(f)),
                    ..join
                },
                semi,
            ),
            plan => plan,
        }
    }

    /// Applies `f` to a subplan if `ok` returns a match.
    /// Recurses until an `ok` match is found.
    pub fn map_if<Info>(self, f: impl FnOnce(Self, Info) -> Self, ok: impl Fn(&Self) -> Option<Info>) -> Self {
        if let Some(info) = ok(&self) {
            return f(self, info);
        }
        let matches = |plan: &PhysicalPlan| {
            // Does `ok` match a subplan?
            plan.any(&|plan| ok(plan).is_some())
        };
        match self {
            Self::NLJoin(lhs, rhs) if matches(&lhs) => {
                // Replace the lhs subtree
                Self::NLJoin(Box::new(lhs.map_if(f, ok)), rhs)
            }
            Self::NLJoin(lhs, rhs) if matches(&rhs) => {
                // Replace the rhs subtree
                Self::NLJoin(lhs, Box::new(rhs.map_if(f, ok)))
            }
            Self::HashJoin(join, semi) if matches(&join.lhs) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::HashJoin(join, semi) if matches(&join.rhs) => Self::HashJoin(
                HashJoin {
                    rhs: Box::new(join.rhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::IxJoin(join, semi) if matches(&join.lhs) => Self::IxJoin(
                IxJoin {
                    lhs: Box::new(join.lhs.map_if(f, ok)),
                    ..join
                },
                semi,
            ),
            Self::Filter(input, expr) if matches(&input) => {
                // Replace the input only if there is a match
                Self::Filter(Box::new(input.map_if(f, ok)), expr)
            }
            _ => self,
        }
    }

    /// Applies a rewrite rule once to this plan.
    /// Updates indicator variable if plan was modified.
    pub fn apply_once<R: RewriteRule<Plan = PhysicalPlan>>(self, ok: &mut bool) -> Self {
        if let Some(info) = R::matches(&self) {
            *ok = true;
            return R::rewrite(self, info);
        }
        self
    }

    /// Recursively apply a rule to all subplans until a fixedpoint is reached.
    pub fn apply_rec<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Self {
        let mut ok = false;
        let plan = self.map_if(
            |plan, info| {
                ok = true;
                R::rewrite(plan, info)
            },
            R::matches,
        );
        if ok {
            return plan.apply_rec::<R>();
        }
        plan
    }

    /// Repeatedly apply a rule until a fixedpoint is reached.
    /// It does not apply rule recursively to subplans.
    pub fn apply_until<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Self {
        let mut ok = false;
        let plan = self.apply_once::<R>(&mut ok);
        if ok {
            return plan.apply_until::<R>();
        }
        plan
    }

    /// Optimize a physical plan by applying rewrite rules.
    ///
    /// First we canonicalize the plan.
    /// Next we push filters to the leaves.
    /// Then we try to turn those filters into index scans.
    /// And finally we deterimine the index joins and semijoins.
    pub fn optimize(self, reqs: Vec<Label>) -> Self {
        self.map(&Self::canonicalize)
            .apply_until::<PushConjunction>()
            .apply_until::<PushEqFilter>()
            .apply_rec::<EqToIxScan>()
            .apply_rec::<ConjunctionToIxScan>()
            .apply_rec::<HashToIxJoin>()
            .apply_rec::<UniqueIxJoinRule>()
            .apply_rec::<UniqueHashJoinRule>()
            .introduce_semijoins(reqs)
    }

    /// The rewriter assumes a canonicalized plan.
    /// And this means:
    ///
    /// 1. Literals are always on the rhs of a sargable predicate.
    /// 2. Nested ANDs and ORs are flattened.
    /// 3. The lhs(rhs) expr corresponds to the lhs(rhs) of an equijoin.
    ///
    /// Examples:
    ///
    /// 1. Move values to rhs
    /// ```sql
    /// select * from a where 3 = a.x
    /// ```
    ///
    /// ... to ..
    ///
    /// ```sql
    /// select * from a where a.x = 3
    /// ```
    ///
    /// 2. Flatten ANDs and ORs
    /// ```sql
    /// select * from a where (a.x = 3 and a.y = 4) and a.z = 5
    /// ```
    ///
    /// ... to ..
    ///
    /// ```sql
    /// select * from a where a.x = 3 and a.y = 4 and a.z = 5
    /// ```
    ///
    /// 3. Canonicalize equijoin
    /// ```sql
    /// select a.* from a join b on b.id = a.id
    /// ```
    ///
    /// ... to ...
    ///
    /// ```sql
    /// select a.* from a join b on a.id = b.id
    /// ```
    fn canonicalize(self) -> Self {
        match self {
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field,
                    rhs_field,
                    unique,
                },
                semi,
            ) if rhs.has_label(&lhs_field.var) || lhs.has_label(&rhs_field.var) => Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: rhs_field,
                    rhs_field: lhs_field,
                    unique,
                },
                semi,
            ),
            Self::Filter(input, expr) => {
                let move_value_to_rhs = |expr| match expr {
                    PhysicalExpr::BinOp(op, value, expr)
                        if matches!(&*value, PhysicalExpr::Value(_)) && matches!(&*expr, PhysicalExpr::Field(..)) =>
                    {
                        PhysicalExpr::BinOp(op, expr, value)
                    }
                    _ => expr,
                };
                // Flatten ANDs and ORs, and move values to rhs
                Self::Filter(input, expr.flatten().map(&move_value_to_rhs))
            }
            _ => self,
        }
    }

    /// Introduce semijoins in the plan.
    ///
    /// Example:
    ///
    /// p:  project
    /// x:  join
    /// sx: semijoin
    ///
    /// ```text
    ///    p(c)
    ///     |
    ///     x
    ///    / \
    ///   x   c
    ///  / \
    /// a   b
    ///
    /// ... to ...
    ///
    ///    p(c)
    ///     |
    ///     x
    ///    / \
    ///  p(b) c
    ///   |
    ///   x
    ///  / \
    /// a   b
    ///
    /// ... to ..
    ///
    ///     sx
    ///    /  \
    ///   sx   c
    ///  /  \
    /// a    b
    /// ```
    ///
    /// ```sql
    /// select c.*
    /// from (select * from a where a.x = 3) a
    /// join b on a.id = b.id
    /// join c on b.id = c.id
    /// ```
    ///
    /// ... to ...
    ///
    /// ```sql
    /// select c.*
    /// from (
    ///   select b.*
    ///   from (select * from a where a.x = 3) a
    ///   join b on a.id = b.id
    /// ) b
    /// join c on b.id = c.id
    /// ```
    fn introduce_semijoins(self, mut reqs: Vec<Label>) -> Self {
        impl PhysicalPlan {
            fn append_required_label(&self, reqs: &mut Vec<Label>, label: Label) {
                if !reqs.contains(&label) && self.has_label(&label) {
                    reqs.push(label);
                }
            }
        }
        match self {
            Self::Filter(input, expr) => {
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(ProjectField { var, .. }) = expr {
                        if !reqs.contains(var) {
                            reqs.push(*var);
                        }
                    }
                });
                Self::Filter(Box::new(input.introduce_semijoins(reqs)), expr)
            }
            Self::NLJoin(lhs, rhs) => {
                let mut lhs_reqs = vec![];
                let mut rhs_reqs = vec![];

                for var in reqs {
                    lhs.append_required_label(&mut lhs_reqs, var);
                    rhs.append_required_label(&mut rhs_reqs, var);
                }
                let lhs = lhs.introduce_semijoins(lhs_reqs);
                let rhs = rhs.introduce_semijoins(rhs_reqs);
                let lhs = Box::new(lhs);
                let rhs = Box::new(rhs);
                Self::NLJoin(lhs, rhs)
            }
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: lhs_field @ ProjectField { var: u, .. },
                    rhs_field: rhs_field @ ProjectField { var: v, .. },
                    unique,
                },
                Semi::All,
            ) => {
                let semi = reqs
                    .iter()
                    .all(|label| lhs.has_label(label))
                    .then_some(Semi::Lhs)
                    .or_else(|| reqs.iter().all(|label| rhs.has_label(label)).then_some(Semi::Rhs))
                    .unwrap_or(Semi::All);
                let mut lhs_reqs = vec![u];
                let mut rhs_reqs = vec![v];
                for var in reqs {
                    lhs.append_required_label(&mut lhs_reqs, var);
                    rhs.append_required_label(&mut rhs_reqs, var);
                }
                let lhs = lhs.introduce_semijoins(lhs_reqs);
                let rhs = rhs.introduce_semijoins(rhs_reqs);
                let lhs = Box::new(lhs);
                let rhs = Box::new(rhs);
                Self::HashJoin(
                    HashJoin {
                        lhs,
                        rhs,
                        lhs_field,
                        rhs_field,
                        unique,
                    },
                    semi,
                )
            }
            Self::IxJoin(join, Semi::All) if reqs.len() == 1 && join.rhs_label == reqs[0] => {
                let lhs = join.lhs.introduce_semijoins(vec![join.lhs_probe_expr.var]);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Rhs)
            }
            Self::IxJoin(join, Semi::All) if reqs.iter().all(|var| *var != join.rhs_label) => {
                if !reqs.contains(&join.lhs_probe_expr.var) {
                    reqs.push(join.lhs_probe_expr.var);
                }
                let lhs = join.lhs.introduce_semijoins(reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Lhs)
            }
            Self::IxJoin(join, Semi::All) => {
                let reqs = reqs.into_iter().filter(|label| label != &join.rhs_label).collect();
                let lhs = join.lhs.introduce_semijoins(reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::All)
            }
            _ => self,
        }
    }

    // Does this plan return distinct values for these columns?
    fn returns_distinct_values(&self, label: &Label, cols: &ColSet) -> bool {
        match self {
            // Is there a unique constraint for these cols?
            Self::TableScan(schema, var) => var == label && schema.is_unique(cols),
            // Is there a unique constraint for these cols + the index cols?
            Self::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(col, _),
                    ..
                },
                var,
            ) => {
                var == label
                    && schema.is_unique(&ColSet::from_iter(
                        cols.iter()
                            .chain(prefix.iter().map(|(col_id, _)| *col_id))
                            .chain(vec![*col]),
                    ))
            }
            // If the table in question is on the lhs,
            // and if the lhs returns distinct values,
            // we need the rhs to return at most one row when probed.
            // But this is a unique index join,
            // so by definition this requirement is satisfied.
            Self::IxJoin(IxJoin { lhs, unique: true, .. }, _) if lhs.has_label(label) => {
                lhs.returns_distinct_values(label, cols)
            }
            // If the table in question is on the rhs,
            // and if the rhs returns distinct values,
            // we must not probe the rhs for the same value more than once.
            // Hence the lhs must be distinct w.r.t the probe field.
            Self::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    lhs_probe_expr:
                        ProjectField {
                            var: lhs_label,
                            pos: lhs_field_pos,
                        },
                    ..
                },
                _,
            ) => {
                lhs.returns_distinct_values(lhs_label, &ColSet::from(ColId(*lhs_field_pos as u16)))
                    && rhs.is_unique(cols)
            }
            // If the table in question is on the lhs,
            // and if the lhs returns distinct values,
            // we need the rhs to return at most one row when probed.
            // Hence the rhs must be distinct w.r.t the probe field.
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    rhs_field:
                        ProjectField {
                            var: rhs_label,
                            pos: rhs_field_pos,
                        },
                    ..
                },
                _,
            ) if lhs.has_label(label) => {
                lhs.returns_distinct_values(label, cols)
                    && rhs.returns_distinct_values(rhs_label, &ColSet::from(ColId(*rhs_field_pos as u16)))
            }
            // If the table in question is on the rhs,
            // and if the rhs returns distinct values,
            // we must not probe the rhs for the same value more than once.
            // Hence the lhs must be distinct w.r.t the probe field.
            Self::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field:
                        ProjectField {
                            var: lhs_label,
                            pos: lhs_field_pos,
                        },
                    ..
                },
                _,
            ) => {
                rhs.returns_distinct_values(label, cols)
                    && lhs.returns_distinct_values(lhs_label, &ColSet::from(ColId(*lhs_field_pos as u16)))
            }
            // For the columns in question,
            // the base table may not return distinct values,
            // but given the necessary equality conditions,
            // the filter can return distinct values for them.
            Self::Filter(input, expr) => {
                let mut cols: Vec<_> = cols.iter().collect();
                expr.visit(&mut |plan| {
                    if let PhysicalExpr::BinOp(BinOp::Eq, expr, value) = plan {
                        if let (PhysicalExpr::Field(proj), PhysicalExpr::Value(..)) = (&**expr, &**value) {
                            if proj.var == *label {
                                cols.push(proj.pos.into());
                            }
                        }
                    }
                });
                input.returns_distinct_values(label, &ColSet::from_iter(cols))
            }
            _ => false,
        }
    }

    /// Does this plan introduce this label?
    fn has_label(&self, label: &Label) -> bool {
        self.any(&|plan| match plan {
            Self::TableScan(_, var) | Self::IxScan(_, var) | Self::IxJoin(IxJoin { rhs_label: var, .. }, _) => {
                var == label
            }
            _ => false,
        })
    }
}

/// Fetch and return row ids from a btree index
#[derive(Debug, PartialEq, Eq)]
pub struct IxScan {
    /// The table on which this index is defined
    pub schema: Arc<TableSchema>,
    /// The index id
    pub index_id: IndexId,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<(ColId, AlgebraicValue)>,
    /// The index argument
    pub arg: Sarg,
}

/// An index [S]earch [arg]ument
#[derive(Debug, PartialEq, Eq)]
pub enum Sarg {
    Eq(ColId, AlgebraicValue),
    Range(ColId, Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

/// A hash join is potentially a bushy join.
///
/// ```text
///      x
///     / \
///    /   \
///   x     x
///  / \   / \
/// a   b c   d
/// ```
///
/// It joins two relations by a single equality condition.
/// It builds a hash table for the rhs and streams the lhs.
#[derive(Debug, PartialEq, Eq)]
pub struct HashJoin {
    pub lhs: Box<PhysicalPlan>,
    pub rhs: Box<PhysicalPlan>,
    pub lhs_field: ProjectField,
    pub rhs_field: ProjectField,
    pub unique: bool,
}

/// An index join is a left deep join tree,
/// where the lhs is a relation,
/// and the rhs is a relvar or base table,
/// whose rows are fetched using an index.
#[derive(Debug, PartialEq, Eq)]
pub struct IxJoin {
    /// The lhs input used to probe the index
    pub lhs: Box<PhysicalPlan>,
    /// The rhs indexed table
    pub rhs: Arc<TableSchema>,
    /// The rhs relvar label
    pub rhs_label: Label,
    /// The index id
    pub rhs_index: IndexId,
    /// The index field
    pub rhs_field: ColId,
    /// Is the index a unique constraint index?
    pub unique: bool,
    /// The expression for computing probe values.
    /// Values are projected from the lhs,
    /// and used to probe the index on the rhs.
    pub lhs_probe_expr: ProjectField,
}

/// Is this a semijoin?
/// If so, which side is projected?
#[derive(Debug, PartialEq, Eq)]
pub enum Semi {
    Lhs,
    Rhs,
    All,
}

/// A physical scalar expression.
///
/// Types are encoded in the structure of the plan,
/// rather than made explicit as for the logical plan.
#[derive(Debug, PartialEq, Eq)]
pub enum PhysicalExpr {
    /// An n-ary logic expression
    LogOp(LogOp, Vec<PhysicalExpr>),
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A constant algebraic value
    Value(AlgebraicValue),
    /// A field projection expression
    Field(ProjectField),
}

impl PhysicalExpr {
    /// Walks the expression tree and calls `f` on every subexpression
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Self::LogOp(_, exprs) => {
                for expr in exprs {
                    expr.visit(f);
                }
            }
            _ => {}
        }
    }

    /// Applies the transformation `f` to all subplans
    pub fn map(self, f: &impl Fn(Self) -> Self) -> Self {
        match f(self) {
            value @ Self::Value(..) => value,
            field @ Self::Field(..) => field,
            Self::BinOp(op, a, b) => Self::BinOp(op, Box::new(a.map(f)), Box::new(b.map(f))),
            Self::LogOp(op, exprs) => Self::LogOp(op, exprs.into_iter().map(|expr| expr.map(f)).collect()),
        }
    }

    /// Flatten nested ANDs and ORs
    fn flatten(self) -> Self {
        match self {
            Self::LogOp(op, exprs) => Self::LogOp(
                op,
                exprs
                    .into_iter()
                    .map(Self::flatten)
                    .flat_map(|expr| match expr {
                        Self::LogOp(nested, exprs) if nested == op => exprs,
                        _ => vec![expr],
                    })
                    .collect(),
            ),
            Self::BinOp(op, a, b) => Self::BinOp(op, Box::new(a.flatten()), Box::new(b.flatten())),
            Self::Field(..) | Self::Value(..) => self,
        }
    }
}

/// A physical context for the result of a query compilation.
pub struct PhysicalCtx<'a> {
    pub plan: PhysicalProject,
    pub sql: &'a str,
    pub source: StatementSource,
}

pub trait RewriteRule {
    type Plan;
    type Info;

    fn matches(plan: &Self::Plan) -> Option<Self::Info>;
    fn rewrite(plan: Self::Plan, info: Self::Info) -> Self::Plan;
}

/// Push equality conditions down to the leaves.
///
/// Example:
///
/// ```sql
/// select b.*
/// from a join b on a.id = b.id
/// where a.x = 3
/// ```
///
/// ... to ...
///
/// ```sql
/// select b.*
/// from (select * from a where x = 3) a
/// join b on a.id = b.id
/// ```
///
/// Example:
///
/// ```text
///  s(a)
///   |
///   x
///  / \
/// a   b
///
/// ... to ...
///
///    x
///   / \
/// s(a) b
///  |
///  a
/// ```
struct PushEqFilter;

impl RewriteRule for PushEqFilter {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, expr, value)) = plan {
            if let (PhysicalExpr::Field(ProjectField { var, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value) {
                return match &**input {
                    PhysicalPlan::TableScan(..) => None,
                    input => input
                        .any(&|plan| match plan {
                            PhysicalPlan::TableScan(_, label) => label == var,
                            _ => false,
                        })
                        .then_some(*var),
                };
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, relvar: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, expr) = plan {
            return input.map_if(
                |scan, _| PhysicalPlan::Filter(Box::new(scan), expr),
                |plan| match plan {
                    PhysicalPlan::TableScan(_, var) if var == &relvar => Some(()),
                    _ => None,
                },
            );
        }
        unreachable!()
    }
}

/// Push conjunctions down to the leaves.
///
/// Example:
///
/// ```sql
/// select b.*
/// from a join b on a.id = b.id
/// where a.x = 3 and a.y = 5
/// ```
///
/// ... to ...
///
/// ```sql
/// select b.*
/// from (select * from a where x = 3 and y = 5) a
/// join b on a.id = b.id
/// ```
///
/// Example:
///
/// ```text
///  s(a)
///   |
///   x
///  / \
/// a   b
///
/// ... to ...
///
///    x
///   / \
/// s(a) b
///  |
///  a
/// ```
struct PushConjunction;

impl RewriteRule for PushConjunction {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            return exprs.iter().find_map(|expr| {
                if let PhysicalExpr::BinOp(BinOp::Eq, expr, value) = expr {
                    if let (PhysicalExpr::Field(ProjectField { var, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value)
                    {
                        return match &**input {
                            PhysicalPlan::TableScan(..) => None,
                            input => input
                                .any(&|plan| match plan {
                                    PhysicalPlan::TableScan(_, label) => label == var,
                                    _ => false,
                                })
                                .then_some(*var),
                        };
                    }
                }
                None
            });
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, relvar: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            let mut leaf_exprs = vec![];
            let mut root_exprs = vec![];
            for expr in exprs {
                if let PhysicalExpr::BinOp(BinOp::Eq, lhs, value) = &expr {
                    if let (PhysicalExpr::Field(ProjectField { var, .. }), PhysicalExpr::Value(_)) = (&**lhs, &**value)
                    {
                        if var == &relvar {
                            leaf_exprs.push(expr);
                            continue;
                        }
                    }
                }
                root_exprs.push(expr);
            }
            // Match on table scan
            let ok = |plan: &PhysicalPlan| match plan {
                PhysicalPlan::TableScan(_, var) if var == &relvar => Some(()),
                _ => None,
            };
            // Map scan to scan + filter
            let f = |scan, _| {
                PhysicalPlan::Filter(
                    Box::new(scan),
                    match leaf_exprs.len() {
                        1 => leaf_exprs.swap_remove(0),
                        _ => PhysicalExpr::LogOp(LogOp::And, leaf_exprs),
                    },
                )
            };
            // Remove top level filter if all conditions were pushable
            if root_exprs.is_empty() {
                return input.map_if(f, ok);
            }
            // Otherwise remove exprs from top level filter and push
            return PhysicalPlan::Filter(
                Box::new(input.map_if(f, ok)),
                match root_exprs.len() {
                    1 => root_exprs.swap_remove(0),
                    _ => PhysicalExpr::LogOp(LogOp::And, root_exprs),
                },
            );
        }
        unreachable!()
    }
}

/// Turn an equality predicate into a single column index scan
struct EqToIxScan;

struct IxScanInfo {
    index_id: IndexId,
    col_id: ColId,
}

impl RewriteRule for EqToIxScan {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, expr, value)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
                if let (PhysicalExpr::Field(ProjectField { pos, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value) {
                    return schema
                        .indexes
                        .iter()
                        .find_map(
                            |IndexSchema {
                                 index_id,
                                 index_algorithm,
                                 ..
                             }| {
                                index_algorithm
                                    .columns()
                                    .as_singleton()
                                    .filter(|col_id| col_id.idx() == *pos)
                                    .map(|col_id| IxScanInfo {
                                        index_id: *index_id,
                                        col_id,
                                    })
                            },
                        )
                        .or_else(|| {
                            schema.indexes.iter().find_map(
                                |IndexSchema {
                                     index_id,
                                     index_algorithm,
                                     ..
                                 }| {
                                    index_algorithm
                                        .columns()
                                        .head()
                                        .filter(|col_id| col_id.idx() == *pos)
                                        .map(|col_id| IxScanInfo {
                                            index_id: *index_id,
                                            col_id,
                                        })
                                },
                            )
                        });
                }
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, _, value)) = plan {
            if let PhysicalPlan::TableScan(schema, var) = *input {
                if let PhysicalExpr::Value(v) = *value {
                    return PhysicalPlan::IxScan(
                        IxScan {
                            schema,
                            index_id: info.index_id,
                            prefix: vec![],
                            arg: Sarg::Eq(info.col_id, v),
                        },
                        var,
                    );
                }
            }
        }
        unreachable!()
    }
}

/// Turn a conjunction into a single column index scan
struct ConjunctionToIxScan;

impl RewriteRule for ConjunctionToIxScan {
    type Plan = PhysicalPlan;
    type Info = (usize, IxScanInfo);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
                return exprs.iter().enumerate().find_map(|(i, expr)| {
                    if let PhysicalExpr::BinOp(BinOp::Eq, lhs, value) = expr {
                        if let (PhysicalExpr::Field(ProjectField { pos, .. }), PhysicalExpr::Value(_)) =
                            (&**lhs, &**value)
                        {
                            return schema
                                .indexes
                                .iter()
                                .find_map(
                                    |IndexSchema {
                                         index_id,
                                         index_algorithm,
                                         ..
                                     }| {
                                        index_algorithm
                                            .columns()
                                            .as_singleton()
                                            .filter(|col_id| col_id.idx() == *pos)
                                            .map(|col_id| {
                                                (
                                                    i,
                                                    IxScanInfo {
                                                        index_id: *index_id,
                                                        col_id,
                                                    },
                                                )
                                            })
                                    },
                                )
                                .or_else(|| {
                                    schema.indexes.iter().find_map(
                                        |IndexSchema {
                                             index_id,
                                             index_algorithm,
                                             ..
                                         }| {
                                            index_algorithm
                                                .columns()
                                                .head()
                                                .filter(|col_id| col_id.idx() == *pos)
                                                .map(|col_id| {
                                                    (
                                                        i,
                                                        IxScanInfo {
                                                            index_id: *index_id,
                                                            col_id,
                                                        },
                                                    )
                                                })
                                        },
                                    )
                                });
                        }
                    }
                    None
                });
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, mut exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, label) = *input {
                let (i, IxScanInfo { index_id, col_id }) = info;
                if let PhysicalExpr::BinOp(BinOp::Eq, _, value) = exprs.swap_remove(i) {
                    if let PhysicalExpr::Value(v) = *value {
                        return PhysicalPlan::Filter(
                            Box::new(PhysicalPlan::IxScan(
                                IxScan {
                                    schema,
                                    index_id,
                                    prefix: vec![],
                                    arg: Sarg::Eq(col_id, v),
                                },
                                label,
                            )),
                            match exprs.len() {
                                1 => exprs.swap_remove(0),
                                _ => PhysicalExpr::LogOp(LogOp::And, exprs),
                            },
                        );
                    }
                }
            }
        }
        unreachable!()
    }
}

/// Is this hash join a left deep join tree that can use an index?
///
/// ```text
/// Notation:
///
/// x: index join
/// hx: hash join
///
///     hx
///    /  \
///   x    c
///  / \
/// a   b
///
/// ... to ...
///
///     x
///    / \
///   x   c
///  / \
/// a   b
/// ```
struct HashToIxJoin;

impl RewriteRule for HashToIxJoin {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                rhs,
                rhs_field:
                    ProjectField {
                        var: rhs_var,
                        pos: field_pos,
                    },
                ..
            },
            _,
        ) = plan
        {
            return match &**rhs {
                PhysicalPlan::TableScan(schema, var) if var == rhs_var => {
                    // Is there a single column index on this field?
                    schema.indexes.iter().find_map(|ix| {
                        ix.index_algorithm
                            .columns()
                            .as_singleton()
                            .filter(|col_id| col_id.idx() == *field_pos)
                            .map(|col_id| IxScanInfo {
                                index_id: ix.index_id,
                                col_id,
                            })
                    })
                }
                _ => None,
            };
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::HashJoin(join, semi) = plan {
            if let PhysicalPlan::TableScan(rhs, rhs_label) = *join.rhs {
                return PhysicalPlan::IxJoin(
                    IxJoin {
                        lhs: join.lhs,
                        rhs,
                        rhs_label,
                        rhs_index: info.index_id,
                        rhs_field: info.col_id,
                        unique: false,
                        lhs_probe_expr: join.lhs_field,
                    },
                    semi,
                );
            }
        }
        unreachable!()
    }
}

/// Does this index join use a unique index?
struct UniqueIxJoinRule;

impl RewriteRule for UniqueIxJoinRule {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::IxJoin(
            IxJoin {
                unique: false,
                rhs,
                rhs_field,
                ..
            },
            _,
        ) = plan
        {
            return rhs
                .constraints
                .iter()
                .filter_map(|cs| cs.data.unique_columns())
                .filter_map(|cols| cols.as_singleton())
                .find(|col_id| col_id == rhs_field)
                .map(|_| ());
        }
        None
    }

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::IxJoin(IxJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        plan
    }
}

/// Does probing the hash table return at most one element?
struct UniqueHashJoinRule;

impl RewriteRule for UniqueHashJoinRule {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                unique: false,
                rhs,
                rhs_field:
                    ProjectField {
                        var: rhs_label,
                        pos: rhs_field_pos,
                    },
                ..
            },
            _,
        ) = plan
        {
            return rhs
                .returns_distinct_values(rhs_label, &ColSet::from(ColId(*rhs_field_pos as u16)))
                .then_some(());
        }
        None
    }

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::HashJoin(HashJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use spacetimedb_expr::check::{compile_sql_sub, SchemaView};
    use spacetimedb_lib::{
        db::auth::{StAccess, StTableType},
        AlgebraicType, AlgebraicValue,
    };
    use spacetimedb_primitives::{ColId, ColList, ColSet, TableId};
    use spacetimedb_schema::{
        def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData},
        schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    };
    use spacetimedb_sql_parser::ast::BinOp;

    use crate::{
        compile::compile,
        plan::{HashJoin, IxJoin, IxScan, PhysicalPlan, PhysicalProject, ProjectField, Sarg, Semi},
    };

    use super::PhysicalExpr;

    struct SchemaViewer {
        schemas: Vec<Arc<TableSchema>>,
    }

    impl SchemaView for SchemaViewer {
        fn schema(&self, name: &str) -> Option<Arc<TableSchema>> {
            self.schemas
                .iter()
                .find(|schema| schema.table_name.as_ref() == name)
                .cloned()
        }
    }

    fn schema(
        table_id: TableId,
        table_name: &str,
        columns: &[(&str, AlgebraicType)],
        indexes: &[&[usize]],
        unique: &[&[usize]],
        primary_key: Option<usize>,
    ) -> TableSchema {
        TableSchema::new(
            table_id,
            table_name.to_owned().into_boxed_str(),
            columns
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| ColumnSchema {
                    table_id,
                    col_name: (*name).to_owned().into_boxed_str(),
                    col_pos: i.into(),
                    col_type: ty.clone(),
                })
                .collect(),
            indexes
                .iter()
                .enumerate()
                .map(|(i, cols)| IndexSchema {
                    table_id,
                    index_id: i.into(),
                    index_name: "".to_owned().into_boxed_str(),
                    index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                        columns: ColList::from_iter(cols.iter().copied()),
                    }),
                })
                .collect(),
            unique
                .iter()
                .enumerate()
                .map(|(i, cols)| ConstraintSchema {
                    table_id,
                    constraint_id: i.into(),
                    constraint_name: "".to_owned().into_boxed_str(),
                    data: ConstraintData::Unique(UniqueConstraintData {
                        columns: ColSet::from_iter(cols.iter().copied()),
                    }),
                })
                .collect(),
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            primary_key.map(ColId::from),
        )
    }

    /// No rewrites applied to a simple table scan
    #[test]
    fn table_scan_noop() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[("id", AlgebraicType::U64), ("x", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer {
            schemas: vec![t.clone()],
        };

        let sql = "select * from t";

        let lp = compile_sql_sub(sql, &db).unwrap();
        let pp = compile(lp).plan.optimize();

        match pp {
            PhysicalProject::None(PhysicalPlan::TableScan(schema, _)) => {
                assert_eq!(schema.table_id, t_id);
            }
            proj => panic!("unexpected project: {:#?}", proj),
        };
    }

    /// No rewrites applied to a table scan + filter
    #[test]
    fn filter_noop() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[("id", AlgebraicType::U64), ("x", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer {
            schemas: vec![t.clone()],
        };

        let sql = "select * from t where x = 5";

        let lp = compile_sql_sub(sql, &db).unwrap();
        let pp = compile(lp).plan.optimize();

        match pp {
            PhysicalProject::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(ProjectField { pos: 1, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U64(5))));

                match *input {
                    PhysicalPlan::TableScan(schema, _) => {
                        assert_eq!(schema.table_id, t_id);
                    }
                    plan => panic!("unexpected plan: {:#?}", plan),
                }
            }
            proj => panic!("unexpected project: {:#?}", proj),
        };
    }

    /// Given the following operator notation:
    ///
    /// x:  join  
    /// p:  project  
    /// s:  select  
    /// ix: index scan  
    /// rx: right index semijoin  
    ///
    /// This test takes the following logical plan:
    ///
    /// ```text
    ///       p(b)
    ///        |
    ///        x
    ///       / \
    ///      x   b
    ///     / \
    ///    x   l
    ///   / \
    /// s(u) l
    ///  |
    ///  u
    /// ```
    ///
    /// And turns it into the following physical plan:
    ///
    /// ```text
    ///         rx
    ///        /  \
    ///       rx   b
    ///      /  \
    ///     rx   l
    ///    /  \
    /// ix(u)  l
    /// ```
    #[test]
    fn index_semijoins_1() {
        let u_id = TableId(1);
        let l_id = TableId(2);
        let b_id = TableId(3);

        let u = Arc::new(schema(
            u_id,
            "u",
            &[("identity", AlgebraicType::U64), ("entity_id", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0], &[1]],
            Some(0),
        ));

        let l = Arc::new(schema(
            l_id,
            "l",
            &[("entity_id", AlgebraicType::U64), ("chunk", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        ));

        let b = Arc::new(schema(
            b_id,
            "b",
            &[("entity_id", AlgebraicType::U64), ("misc", AlgebraicType::U64)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer {
            schemas: vec![u.clone(), l.clone(), b.clone()],
        };

        let sql = "
            select b.*
            from u
            join l as p on u.entity_id = p.entity_id
            join l as q on p.chunk = q.chunk
            join b on q.entity_id = b.entity_id
            where u.identity = 5
        ";
        let lp = compile_sql_sub(sql, &db).unwrap();
        let pp = compile(lp).plan.optimize();

        // Plan:
        //         rx
        //        /  \
        //       rx   b
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match pp {
            PhysicalProject::None(plan) => plan,
            proj => panic!("unexpected project: {:#?}", proj),
        };

        // Plan:
        //         rx
        //        /  \
        //       rx   b
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_probe_expr: ProjectField { pos: 0, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, b_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //       rx
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(1),
                    unique: false,
                    lhs_probe_expr: ProjectField { pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, l_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_probe_expr: ProjectField { pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, l_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(u)
        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, u_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }
    }

    /// Given the following operator notation:
    ///
    /// x:  join  
    /// p:  project  
    /// s:  select  
    /// ix: index scan  
    /// rx: right index semijoin  
    /// rj: right hash semijoin  
    ///
    /// This test takes the following logical plan:
    ///
    /// ```text
    ///         p(p)
    ///          |
    ///          x
    ///         / \
    ///        x   p
    ///       / \
    ///      x   s(w)
    ///     / \   |
    ///    x   w  w
    ///   / \
    /// s(m) m
    ///  |
    ///  m
    /// ```
    ///
    /// And turns it into the following physical plan:
    ///
    /// ```text
    ///           rx
    ///          /  \
    ///         rj   p
    ///        /  \
    ///       rx  ix(w)
    ///      /  \
    ///     rx   w
    ///    /  \
    /// ix(m)  m
    /// ```
    #[test]
    fn index_semijoins_2() {
        let m_id = TableId(1);
        let w_id = TableId(2);
        let p_id = TableId(3);

        let m = Arc::new(schema(
            m_id,
            "m",
            &[("employee", AlgebraicType::U64), ("manager", AlgebraicType::U64)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        ));

        let w = Arc::new(schema(
            w_id,
            "w",
            &[("employee", AlgebraicType::U64), ("project", AlgebraicType::U64)],
            &[&[0], &[1], &[0, 1]],
            &[&[0, 1]],
            None,
        ));

        let p = Arc::new(schema(
            p_id,
            "p",
            &[("id", AlgebraicType::U64), ("name", AlgebraicType::String)],
            &[&[0]],
            &[&[0]],
            Some(0),
        ));

        let db = SchemaViewer {
            schemas: vec![m.clone(), w.clone(), p.clone()],
        };

        let sql = "
            select p.*
            from m
            join m as n on m.manager = n.manager
            join w as u on n.employee = u.employee
            join w as v on u.project = v.project
            join p on p.id = v.project
            where 5 = m.employee and 5 = v.employee
        ";
        let lp = compile_sql_sub(sql, &db).unwrap();
        let pp = compile(lp).plan.optimize();

        // Plan:
        //           rx
        //          /  \
        //         rj   p
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match pp {
            PhysicalProject::None(plan) => plan,
            proj => panic!("unexpected project: {:#?}", proj),
        };

        // Plan:
        //           rx
        //          /  \
        //         rj   p
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: true,
                    lhs_probe_expr: ProjectField { pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, p_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //         rj
        //        /  \
        //       rx  ix(w)
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let (rhs, lhs) = match plan {
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs,
                    rhs,
                    lhs_field: ProjectField { pos: 1, .. },
                    rhs_field: ProjectField { pos: 1, .. },
                    unique: true,
                },
                Semi::Rhs,
            ) => (*rhs, *lhs),
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(w)
        match rhs {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, w_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }

        // Plan:
        //       rx
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match lhs {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(0),
                    unique: false,
                    lhs_probe_expr: ProjectField { pos: 0, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, w_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs,
                    rhs,
                    rhs_field: ColId(1),
                    unique: false,
                    lhs_probe_expr: ProjectField { pos: 1, .. },
                    ..
                },
                Semi::Rhs,
            ) => {
                assert_eq!(rhs.table_id, m_id);
                *lhs
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        // Plan: ix(m)
        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema,
                    prefix,
                    arg: Sarg::Eq(ColId(0), AlgebraicValue::U64(5)),
                    ..
                },
                _,
            ) => {
                assert!(prefix.is_empty());
                assert_eq!(schema.table_id, m_id);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        }
    }
}
