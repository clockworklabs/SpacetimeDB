use std::{
    borrow::Cow,
    ops::{Bound, Deref, DerefMut},
    sync::Arc,
};

use anyhow::{bail, Result};
use derive_more::From;
use either::Either;
use spacetimedb_expr::{expr::AggType, StatementSource};
use spacetimedb_lib::{query::Delta, sats::size_of::SizeOf, AlgebraicValue, ProductValue};
use spacetimedb_primitives::{ColId, ColSet, IndexId, TableId};
use spacetimedb_schema::schema::{IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use spacetimedb_table::table::RowRef;

use crate::rules::{
    ComputePositions, HashToIxJoin, IxScanAnd, IxScanEq, IxScanEq2Col, IxScanEq3Col, PullFilterAboveHashJoin,
    PushConstAnd, PushConstEq, PushLimit, ReorderDeltaJoinRhs, ReorderHashJoin, RewriteRule, UniqueHashJoinRule,
    UniqueIxJoinRule,
};

/// Table aliases are replaced with labels in the physical plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, From)]
pub struct Label(pub usize);

/// Physical plans always terminate with a projection.
/// This type of projection returns row ids.
///
/// It can represent:
///
/// ```sql
/// select * from t
/// ```
///
/// and
///
/// ```sql
/// select t.* from t join ...
/// ```
///
/// but not
///
/// ```sql
/// select a from t
/// ```
#[derive(Debug, Clone)]
pub enum ProjectPlan {
    None(PhysicalPlan),
    Name(PhysicalPlan, Label, Option<usize>),
}

impl Deref for ProjectPlan {
    type Target = PhysicalPlan;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan,
        }
    }
}

impl DerefMut for ProjectPlan {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan,
        }
    }
}

impl ProjectPlan {
    pub fn optimize(self) -> Result<Self> {
        match self {
            Self::None(plan) => Ok(Self::None(plan.optimize(vec![])?)),
            Self::Name(plan, label, _) => {
                let plan = plan.optimize(vec![label])?;
                let n = plan.nfields();
                let pos = plan.position(&label);
                Ok(match n {
                    1 => Self::None(plan),
                    _ => Self::Name(plan, label, pos),
                })
            }
        }
    }

    /// Unwrap the underlying physical plan
    pub fn physical_plan(&self) -> &PhysicalPlan {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan,
        }
    }
}

/// Physical plans always terminate with a projection.
/// This type can project fields within a table.
///
/// That is, it can represent:
///
/// ```sql
/// select a from t
/// ```
///
/// as well as
///
/// ```sql
/// select t.a, s.b from t join s ...
/// ```
///
/// TODO: LIMIT and COUNT were added rather hastily.
/// We should rethink having separate plan types for projections and selections,
/// as it makes optimization more difficult the more they diverge.
///
/// Note that RLS takes a single expression and produces a list of expressions.
/// Hence why these variants take lists rather than single expressions.
/// See [spacetimedb_expr::ProjectList] for details.
#[derive(Debug)]
pub enum ProjectListPlan {
    /// A plan that returns physical rows
    Name(Vec<ProjectPlan>),
    /// A plan that returns virtual rows
    List(Vec<PhysicalPlan>, Vec<TupleField>),
    /// A plan that limits rows
    Limit(Box<ProjectListPlan>, u64),
    /// An aggregate function
    Agg(Vec<PhysicalPlan>, AggType),
}

impl ProjectListPlan {
    pub fn optimize(self) -> Result<Self> {
        match self {
            Self::Name(plan) => Ok(Self::Name(
                plan.into_iter().map(|plan| plan.optimize()).collect::<Result<_>>()?,
            )),
            Self::Limit(plan, n) => {
                let mut limit = Self::Limit(Box::new(plan.optimize()?), n);
                // Merge a limit with a scan if possible
                if PushLimit::matches(&limit).is_some() {
                    limit = PushLimit::rewrite(limit, ())?;
                }
                Ok(limit)
            }
            Self::Agg(plan, agg_type) => Ok(Self::Agg(
                plan.into_iter()
                    .map(|plan| plan.optimize(vec![]))
                    .collect::<Result<_>>()?,
                agg_type,
            )),
            Self::List(plans, mut fields) => {
                let mut optimized_plans = Vec::with_capacity(plans.len());
                for plan in plans {
                    // Collect the names of the relvars
                    let labels = fields.iter().map(|field| field.label).collect();
                    // Optimize each plan
                    let optimized_plan = plan.optimize(labels)?;
                    // Compute the position of each relvar referenced in the projection
                    for TupleField { label, label_pos, .. } in &mut fields {
                        *label_pos = optimized_plan.position(label);
                    }
                    optimized_plans.push(optimized_plan);
                }
                Ok(Self::List(optimized_plans, fields))
            }
        }
    }

    /// Returns an iterator over the underlying physical plans
    pub fn plan_iter(&self) -> impl Iterator<Item = &PhysicalPlan> + '_ {
        match self {
            Self::List(plans, _) | Self::Agg(plans, _) => Either::Left(plans.iter()),
            Self::Name(plans) => Either::Right(plans.iter().map(|plan| plan.physical_plan())),
            Self::Limit(plan, _) => plan.plan_iter(),
        }
    }
}

/// Query operators return tuples of rows.
/// And this type refers to a field of a row within a tuple.
///
/// Note that from the perspective of the optimizer,
/// tuple elements have names or labels,
/// so as to preserve query semantics across rewrites.
///
/// However from the perspective of the query engine,
/// tuple elements are entirely positional.
/// Hence the need for both `label` and `label_pos`.
///
/// The former is consistent across rewrites.
/// The latter is only computed once after optimization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupleField {
    pub label: Label,
    pub label_pos: Option<usize>,
    pub field_pos: usize,
}

/// A physical plan represents a concrete evaluation strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalPlan {
    /// Scan a table row by row, returning row ids
    TableScan(TableScan, Label),
    /// Fetch row ids from an index
    IxScan(IxScan, Label),
    /// An index join + projection
    IxJoin(IxJoin, Semi),
    /// A hash join + projection
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
            Self::TableScan(..) | Self::IxScan(..) => {}
        }
    }

    /// Walks the plan tree and calls `f` on every op
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) => {
                input.visit_mut(f);
            }
            Self::NLJoin(lhs, rhs) | Self::HashJoin(HashJoin { lhs, rhs, .. }, _) => {
                lhs.visit_mut(f);
                rhs.visit_mut(f);
            }
            Self::TableScan(..) | Self::IxScan(..) => {}
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
            plan @ Self::TableScan(..) | plan @ Self::IxScan(..) => plan,
        }
    }

    /// Applies `f` to a subplan if `ok` returns a match.
    /// Recurses until an `ok` match is found.
    pub fn map_if<Info>(
        self,
        f: impl FnOnce(Self, Info) -> Result<Self>,
        ok: impl Fn(&Self) -> Option<Info>,
    ) -> Result<Self> {
        if let Some(info) = ok(&self) {
            return f(self, info);
        }
        let matches = |plan: &PhysicalPlan| {
            // Does `ok` match a subplan?
            plan.any(&|plan| ok(plan).is_some())
        };
        Ok(match self {
            Self::TableScan(..) | Self::IxScan(..) => self,
            Self::NLJoin(lhs, rhs) => {
                if matches(&lhs) {
                    return Ok(Self::NLJoin(Box::new(lhs.map_if(f, ok)?), rhs));
                }
                if matches(&rhs) {
                    return Ok(Self::NLJoin(lhs, Box::new(rhs.map_if(f, ok)?)));
                }
                Self::NLJoin(lhs, rhs)
            }
            Self::HashJoin(join, semi) => {
                if matches(&join.lhs) {
                    return Ok(Self::HashJoin(
                        HashJoin {
                            lhs: Box::new(join.lhs.map_if(f, ok)?),
                            ..join
                        },
                        semi,
                    ));
                }
                if matches(&join.rhs) {
                    return Ok(Self::HashJoin(
                        HashJoin {
                            rhs: Box::new(join.rhs.map_if(f, ok)?),
                            ..join
                        },
                        semi,
                    ));
                }
                Self::HashJoin(join, semi)
            }
            Self::IxJoin(join, semi) => {
                if matches(&join.lhs) {
                    return Ok(Self::IxJoin(
                        IxJoin {
                            lhs: Box::new(join.lhs.map_if(f, ok)?),
                            ..join
                        },
                        semi,
                    ));
                }
                Self::IxJoin(join, semi)
            }
            Self::Filter(input, expr) => {
                if matches(&input) {
                    return Ok(Self::Filter(Box::new(input.map_if(f, ok)?), expr));
                }
                Self::Filter(input, expr)
            }
        })
    }

    /// Applies a rewrite rule once to this plan.
    /// Updates indicator variable if plan was modified.
    pub fn apply_once<R: RewriteRule<Plan = PhysicalPlan>>(self, ok: &mut bool) -> Result<Self> {
        if let Some(info) = R::matches(&self) {
            *ok = true;
            return R::rewrite(self, info);
        }
        Ok(self)
    }

    /// Recursively apply a rule to all subplans until a fixedpoint is reached.
    pub fn apply_rec<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Result<Self> {
        let mut ok = false;
        let plan = self.map_if(
            |plan, info| {
                ok = true;
                R::rewrite(plan, info)
            },
            R::matches,
        )?;
        if ok {
            return plan.apply_rec::<R>();
        }
        Ok(plan)
    }

    /// Repeatedly apply a rule until a fixedpoint is reached.
    /// It does not apply rule recursively to subplans.
    pub fn apply_until<R: RewriteRule<Plan = PhysicalPlan>>(self) -> Result<Self> {
        let mut ok = false;
        let plan = self.apply_once::<R>(&mut ok)?;
        if ok {
            return plan.apply_until::<R>();
        }
        Ok(plan)
    }

    /// Optimize a plan using the following rewrites:
    ///
    /// 1. Canonicalize the plan
    /// 2. Push filters to the leaves
    /// 3. Turn filters into index scans if possible
    /// 4. Determine index and semijoins
    /// 5. Compute positions for tuple labels
    pub fn optimize(self, reqs: Vec<Label>) -> Result<Self> {
        let optimized = self
            .map(&Self::canonicalize)
            .apply_rec::<PushConstAnd>()?
            .apply_rec::<PushConstEq>()?
            .apply_rec::<ReorderDeltaJoinRhs>()?
            .apply_rec::<PullFilterAboveHashJoin>()?
            .apply_rec::<IxScanEq3Col>()?
            .apply_rec::<IxScanEq2Col>()?
            .apply_rec::<IxScanEq>()?
            .apply_rec::<IxScanAnd>()?
            .apply_rec::<ReorderHashJoin>()?
            .apply_rec::<HashToIxJoin>()?
            .apply_rec::<UniqueIxJoinRule>()?
            .apply_rec::<UniqueHashJoinRule>()?
            .introduce_semijoins(reqs)
            .apply_rec::<ComputePositions>()?;

        let mut unresolved_name = false;

        // Check that we've derived positional values for all named arguments
        optimized.visit(&mut |plan| {
            match plan {
                Self::Filter(_, expr) => {
                    expr.visit(&mut |expr| {
                        if let PhysicalExpr::Field(TupleField { label_pos: None, .. }) = expr {
                            unresolved_name = true;
                        }
                    });
                }
                Self::IxJoin(
                    IxJoin {
                        lhs_field: TupleField { label_pos: None, .. },
                        ..
                    },
                    _,
                )
                | Self::HashJoin(
                    HashJoin {
                        lhs_field: TupleField { label_pos: None, .. },
                        ..
                    },
                    _,
                )
                | Self::HashJoin(
                    HashJoin {
                        rhs_field: TupleField { label_pos: None, .. },
                        ..
                    },
                    _,
                ) => {
                    unresolved_name = true;
                }
                _ => {}
            };
        });

        if unresolved_name {
            bail!("Could not compute positional arguments during query planning")
        }

        Ok(optimized)
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
            ) if rhs.has_label(&lhs_field.label) || lhs.has_label(&rhs_field.label) => Self::HashJoin(
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
                        match op {
                            BinOp::Eq => PhysicalExpr::BinOp(BinOp::Eq, expr, value),
                            BinOp::Ne => PhysicalExpr::BinOp(BinOp::Ne, expr, value),
                            BinOp::Lt => PhysicalExpr::BinOp(BinOp::Gt, expr, value),
                            BinOp::Gt => PhysicalExpr::BinOp(BinOp::Lt, expr, value),
                            BinOp::Lte => PhysicalExpr::BinOp(BinOp::Gte, expr, value),
                            BinOp::Gte => PhysicalExpr::BinOp(BinOp::Lte, expr, value),
                        }
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
        let append_required_label = |plan: &PhysicalPlan, reqs: &mut Vec<Label>, label: Label| {
            if !reqs.contains(&label) && plan.has_label(&label) {
                reqs.push(label);
            }
        };
        match self {
            Self::Filter(input, expr) => {
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(TupleField { label: var, .. }) = expr {
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
                    append_required_label(&lhs, &mut lhs_reqs, var);
                    append_required_label(&rhs, &mut rhs_reqs, var);
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
                    lhs_field: lhs_field @ TupleField { label: u, .. },
                    rhs_field: rhs_field @ TupleField { label: v, .. },
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
                    append_required_label(&lhs, &mut lhs_reqs, var);
                    append_required_label(&rhs, &mut rhs_reqs, var);
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
                let lhs = join.lhs.introduce_semijoins(vec![join.lhs_field.label]);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Rhs)
            }
            Self::IxJoin(join, Semi::All) if reqs.iter().all(|var| *var != join.rhs_label) => {
                if !reqs.contains(&join.lhs_field.label) {
                    reqs.push(join.lhs_field.label);
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
    pub(crate) fn returns_distinct_values(&self, label: &Label, cols: &ColSet) -> bool {
        match self {
            // Is there a unique constraint for these cols?
            Self::TableScan(TableScan { schema, .. }, var) => var == label && schema.as_ref().is_unique(cols),
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
                    && schema.as_ref().is_unique(&ColSet::from_iter(
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
                    lhs_field:
                        TupleField {
                            label: lhs_label,
                            field_pos: lhs_field_pos,
                            ..
                        },
                    ..
                },
                _,
            ) => {
                lhs.returns_distinct_values(lhs_label, &ColSet::from(ColId(*lhs_field_pos as u16)))
                    && rhs.as_ref().is_unique(cols)
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
                        TupleField {
                            label: rhs_label,
                            field_pos: rhs_field_pos,
                            ..
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
                        TupleField {
                            label: lhs_label,
                            field_pos: lhs_field_pos,
                            ..
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
                            if proj.label == *label {
                                cols.push(proj.field_pos.into());
                            }
                        }
                    }
                });
                input.returns_distinct_values(label, &ColSet::from_iter(cols))
            }
            _ => false,
        }
    }

    pub fn index_on_field(&self, label: &Label, field: usize) -> bool {
        self.any(&|plan| match plan {
            Self::TableScan(TableScan { schema, .. }, alias)
            | Self::IxScan(IxScan { schema, .. }, alias)
            | Self::IxJoin(
                IxJoin {
                    rhs: schema,
                    rhs_label: alias,
                    ..
                },
                _,
            ) if alias == label => schema.indexes.iter().any(|IndexSchema { index_algorithm, .. }| {
                index_algorithm
                    .columns()
                    .as_singleton()
                    .is_some_and(|col_id| col_id.idx() == field)
            }),
            _ => false,
        })
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

    /// How many fields do the tuples returned by this plan have?
    fn nfields(&self) -> usize {
        match self {
            Self::TableScan(..) | Self::IxScan(..) | Self::IxJoin(_, Semi::Rhs) => 1,
            Self::Filter(input, _) => input.nfields(),
            Self::IxJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::IxJoin(join, Semi::All) => join.lhs.nfields() + 1,
            Self::HashJoin(join, Semi::Rhs) => join.rhs.nfields(),
            Self::HashJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::HashJoin(join, Semi::All) => join.lhs.nfields() + join.rhs.nfields(),
            Self::NLJoin(lhs, rhs) => lhs.nfields() + rhs.nfields(),
        }
    }

    /// What is the position of this label in the return tuple?
    pub(crate) fn position(&self, label: &Label) -> Option<usize> {
        self.labels()
            .into_iter()
            .enumerate()
            .find(|(_, name)| name == label)
            .map(|(i, _)| i)
    }

    /// Returns the names of the relvars that this operation returns
    fn labels(&self) -> Vec<Label> {
        fn find(plan: &PhysicalPlan, labels: &mut Vec<Label>) {
            match plan {
                PhysicalPlan::TableScan(_, alias)
                | PhysicalPlan::IxScan(_, alias)
                | PhysicalPlan::IxJoin(IxJoin { rhs_label: alias, .. }, Semi::Rhs) => {
                    labels.push(*alias);
                }
                PhysicalPlan::Filter(input, _)
                | PhysicalPlan::IxJoin(IxJoin { lhs: input, .. }, Semi::Lhs)
                | PhysicalPlan::HashJoin(HashJoin { lhs: input, .. }, Semi::Lhs)
                | PhysicalPlan::HashJoin(HashJoin { rhs: input, .. }, Semi::Rhs) => {
                    find(input, labels);
                }
                PhysicalPlan::IxJoin(IxJoin { lhs, rhs_label, .. }, Semi::All) => {
                    find(lhs, labels);
                    labels.push(*rhs_label);
                }
                PhysicalPlan::NLJoin(lhs, rhs) | PhysicalPlan::HashJoin(HashJoin { lhs, rhs, .. }, Semi::All) => {
                    find(lhs, labels);
                    find(rhs, labels);
                }
            }
        }
        let mut labels = vec![];
        find(self, &mut labels);
        labels
    }

    /// Is this operator a table scan with optional label?
    pub fn is_table_scan(&self, label: Option<&Label>) -> bool {
        match self {
            Self::TableScan(_, var) => label.map(|label| var == label).unwrap_or(true),
            _ => false,
        }
    }

    /// Does this plan scan a table with optional label?
    pub fn has_table_scan(&self, label: Option<&Label>) -> bool {
        self.any(&|plan| match plan {
            Self::TableScan(_, var) => label.map(|label| var == label).unwrap_or(true),
            _ => false,
        })
    }

    /// Is this operator a filter?
    fn is_filter(&self) -> bool {
        matches!(self, Self::Filter(..))
    }

    /// Does this plan contain a filter?
    pub fn has_filter(&self) -> bool {
        self.any(&|plan| plan.is_filter())
    }

    /// Is this operator a scan, index or otherwise, of a delta table?
    pub fn is_delta_scan(&self) -> bool {
        matches!(
            self,
            Self::TableScan(TableScan { delta: Some(_), .. }, _) | Self::IxScan(IxScan { delta: Some(_), .. }, _)
        )
    }

    /// If this plan has any simple equality filters such as `x = 0`,
    /// this method returns the values along with the appropriate table and column.
    /// Note, this excludes compound equality filters such as `x = 0 and y = 1`.
    /// Also note that it is not valid to call this method on an optimized plan.
    /// This is because we assume that index scans have not yet been generated.
    pub fn search_args(&self) -> Vec<(TableId, ColId, AlgebraicValue)> {
        let mut args = vec![];
        self.visit(&mut |op| {
            if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, a, b)) = op {
                match (&**a, &**b) {
                    (PhysicalExpr::Field(field), PhysicalExpr::Value(value))
                    | (PhysicalExpr::Value(value), PhysicalExpr::Field(field)) => {
                        input.visit(&mut |op| match op {
                            PhysicalPlan::TableScan(scan, name) if *name == field.label => {
                                args.push((scan.schema.table_id, field.field_pos.into(), value.clone()));
                            }
                            _ => {}
                        });
                    }
                    _ => {}
                }
            }
        });
        args
    }
}

/// Scan a table row by row, returning row ids
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableScan {
    /// The table on which this index is defined
    pub schema: Arc<TableSchema>,
    /// Limit the number of rows scanned
    pub limit: Option<u64>,
    /// Is this a delta table?
    pub delta: Option<Delta>,
}

/// Fetch and return row ids from a btree index
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IxScan {
    /// The table on which this index is defined
    pub schema: Arc<TableSchema>,
    /// Limit the number of rows scanned
    pub limit: Option<u64>,
    /// Is this an index scan over a delta table?
    pub delta: Option<Delta>,
    /// The index id
    pub index_id: IndexId,
    /// An equality prefix for multi-column scans
    pub prefix: Vec<(ColId, AlgebraicValue)>,
    /// The index argument
    pub arg: Sarg,
}

/// An index [S]earch [arg]ument
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sarg {
    Eq(ColId, AlgebraicValue),
    Range(ColId, Bound<AlgebraicValue>, Bound<AlgebraicValue>),
}

/// A join of two relations on a single equality condition.
/// It builds a hash table for the rhs and streams the lhs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashJoin {
    pub lhs: Box<PhysicalPlan>,
    pub rhs: Box<PhysicalPlan>,
    pub lhs_field: TupleField,
    pub rhs_field: TupleField,
    pub unique: bool,
}

/// An index join is a left deep join tree,
/// where the lhs is a relation,
/// and the rhs is a relvar or base table,
/// whose rows are fetched using an index.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub lhs_field: TupleField,
    // Is the rhs a delta table?
    pub rhs_delta: Option<Delta>,
}

/// Is this a semijoin?
/// If so, which side is projected?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semi {
    Lhs,
    Rhs,
    All,
}

/// A physical scalar expression
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalExpr {
    /// An n-ary logic expression
    LogOp(LogOp, Vec<PhysicalExpr>),
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A constant algebraic value
    Value(AlgebraicValue),
    /// A field projection expression
    Field(TupleField),
}

/// A trait for projecting values from a tuple.
/// This is needed because not all tuples are created equal.
/// Some operators return [RowRef]s.
/// Some joins return tuples of combined [RowRef]s.
pub trait ProjectField {
    fn project(&self, field: &TupleField) -> AlgebraicValue;
}

impl ProjectField for RowRef<'_> {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        self.read_col(field.field_pos).unwrap()
    }
}

impl ProjectField for &'_ ProductValue {
    fn project(&self, field: &TupleField) -> AlgebraicValue {
        self.elements[field.field_pos].clone()
    }
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

    /// Walks the expression tree and calls `f` on every subexpression
    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        match self {
            Self::BinOp(_, a, b) => {
                a.visit_mut(f);
                b.visit_mut(f);
            }
            Self::LogOp(_, exprs) => {
                for expr in exprs {
                    expr.visit_mut(f);
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

    /// Evaluate this boolean expression over `row`
    pub fn eval_bool(&self, row: &impl ProjectField) -> bool {
        self.eval(row).as_bool().copied().unwrap_or(false)
    }

    /// Evaluate this boolean expression over `row`
    pub fn eval_bool_with_metrics(&self, row: &impl ProjectField, bytes_scanned: &mut usize) -> bool {
        self.eval_with_metrics(row, bytes_scanned)
            .as_bool()
            .copied()
            .unwrap_or(false)
    }

    /// Evaluate this expression over `row`
    fn eval(&self, row: &impl ProjectField) -> Cow<'_, AlgebraicValue> {
        self.eval_with_metrics(row, &mut 0)
    }

    /// Evaluate this expression over `row`
    fn eval_with_metrics(&self, row: &impl ProjectField, bytes_scanned: &mut usize) -> Cow<'_, AlgebraicValue> {
        fn eval_bin_op(op: BinOp, a: &AlgebraicValue, b: &AlgebraicValue) -> bool {
            match op {
                BinOp::Eq => a == b,
                BinOp::Ne => a != b,
                BinOp::Lt => a < b,
                BinOp::Lte => a <= b,
                BinOp::Gt => a > b,
                BinOp::Gte => a >= b,
            }
        }
        let into = |b| Cow::Owned(AlgebraicValue::Bool(b));
        match self {
            Self::BinOp(op, a, b) => into(eval_bin_op(
                *op,
                &a.eval_with_metrics(row, bytes_scanned),
                &b.eval_with_metrics(row, bytes_scanned),
            )),
            Self::LogOp(LogOp::And, exprs) => into(
                exprs
                    .iter()
                    // ALL is equivalent to AND
                    .all(|expr| expr.eval_bool_with_metrics(row, bytes_scanned)),
            ),
            Self::LogOp(LogOp::Or, exprs) => into(
                exprs
                    .iter()
                    // ANY is equivalent to OR
                    .any(|expr| expr.eval_bool_with_metrics(row, bytes_scanned)),
            ),
            Self::Field(field) => {
                let value = row.project(field);
                *bytes_scanned += value.size_of();
                Cow::Owned(value)
            }
            Self::Value(v) => Cow::Borrowed(v),
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
    pub plan: ProjectListPlan,
    pub sql: &'a str,
    pub source: StatementSource,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use spacetimedb_expr::{
        check::{SchemaView, TypingResult},
        expr::ProjectName,
        statement::{parse_and_type_sql, Statement},
    };
    use spacetimedb_lib::{
        db::auth::{StAccess, StTableType},
        identity::AuthCtx,
        AlgebraicType, AlgebraicValue,
    };
    use spacetimedb_primitives::{ColId, ColList, ColSet, TableId};
    use spacetimedb_schema::{
        def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData},
        schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    };
    use spacetimedb_sql_parser::ast::BinOp;

    use crate::{
        compile::{compile_select, compile_select_list},
        plan::{HashJoin, IxJoin, IxScan, PhysicalPlan, ProjectListPlan, Sarg, Semi, TupleField},
    };

    use super::{PhysicalExpr, ProjectPlan, TableScan};

    struct SchemaViewer {
        schemas: Vec<Arc<TableSchema>>,
    }

    impl SchemaView for SchemaViewer {
        fn table_id(&self, name: &str) -> Option<TableId> {
            self.schemas
                .iter()
                .find(|schema| schema.table_name.as_ref() == name)
                .map(|schema| schema.table_id)
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>> {
            self.schemas.iter().find(|schema| schema.table_id == table_id).cloned()
        }

        fn rls_rules_for_table(&self, _: TableId) -> anyhow::Result<Vec<Box<str>>> {
            Ok(vec![])
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

    /// A wrapper around [spacetimedb_expr::check::parse_and_type_sub] that takes a dummy [AuthCtx]
    fn parse_and_type_sub(sql: &str, tx: &impl SchemaView) -> TypingResult<ProjectName> {
        spacetimedb_expr::check::parse_and_type_sub(sql, tx, &AuthCtx::for_testing()).map(|(plan, _)| plan)
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

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::TableScan(TableScan { schema, .. }, _)) => {
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

        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 1, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U64(5))));

                match *input {
                    PhysicalPlan::TableScan(TableScan { schema, .. }, _) => {
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
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        // Plan:
        //         rx
        //        /  \
        //       rx   b
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match pp {
            ProjectPlan::None(plan) => plan,
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
                    lhs_field: TupleField { field_pos: 0, .. },
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
                    lhs_field: TupleField { field_pos: 1, .. },
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
                    lhs_field: TupleField { field_pos: 1, .. },
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
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

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
            ProjectPlan::None(plan) => plan,
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
                    lhs_field: TupleField { field_pos: 1, .. },
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
                    lhs_field: TupleField { field_pos: 1, .. },
                    rhs_field: TupleField { field_pos: 1, .. },
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
                    lhs_field: TupleField { field_pos: 0, .. },
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
                    lhs_field: TupleField { field_pos: 1, .. },
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

    /// Test single and multi-column index selections
    #[test]
    fn index_scans() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[
                ("w", AlgebraicType::U8),
                ("x", AlgebraicType::U8),
                ("y", AlgebraicType::U8),
                ("z", AlgebraicType::U8),
            ],
            &[&[1], &[2, 3], &[1, 2, 3]],
            &[],
            None,
        ));

        let db = SchemaViewer {
            schemas: vec![t.clone()],
        };

        let sql = "select * from t where x = 3 and y = 4 and z = 5";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        // Select index on (x, y, z)
        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            )) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(3), AlgebraicValue::U8(5)));
                assert_eq!(
                    prefix,
                    vec![(ColId(1), AlgebraicValue::U8(3)), (ColId(2), AlgebraicValue::U8(4))]
                );
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        // Test permutations of the same query
        let sql = "select * from t where z = 5 and y = 4 and x = 3";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            )) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(3), AlgebraicValue::U8(5)));
                assert_eq!(
                    prefix,
                    vec![(ColId(1), AlgebraicValue::U8(3)), (ColId(2), AlgebraicValue::U8(4))]
                );
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        let sql = "select * from t where x = 3 and y = 4";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        // Select index on x
        let plan = match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 2, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U8(4))));
                *input
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            ) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(1), AlgebraicValue::U8(3)));
                assert!(prefix.is_empty());
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        let sql = "select * from t where w = 5 and x = 4";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        // Select index on x
        let plan = match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 0, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U8(5))));
                *input
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            ) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(1), AlgebraicValue::U8(4)));
                assert!(prefix.is_empty());
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };

        let sql = "select * from t where y = 1";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        // Do not select index on (y, z)
        match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*input, PhysicalPlan::TableScan(..)));
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 2, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U8(1))));
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        // Select index on [y, z]
        let sql = "select * from t where y = 1 and z = 2";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            )) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(3), AlgebraicValue::U8(2)));
                assert_eq!(prefix, vec![(ColId(2), AlgebraicValue::U8(1))]);
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        // Check permutations of the same query
        let sql = "select * from t where z = 2 and y = 1";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            )) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(3), AlgebraicValue::U8(2)));
                assert_eq!(prefix, vec![(ColId(2), AlgebraicValue::U8(1))]);
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        // Select index on (y, z) and filter on (w)
        let sql = "select * from t where w = 1 and y = 2 and z = 3";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        let plan = match pp {
            ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, field, value))) => {
                assert!(matches!(*field, PhysicalExpr::Field(TupleField { field_pos: 0, .. })));
                assert!(matches!(*value, PhysicalExpr::Value(AlgebraicValue::U8(1))));
                *input
            }
            proj => panic!("unexpected plan: {:#?}", proj),
        };

        match plan {
            PhysicalPlan::IxScan(
                IxScan {
                    schema, prefix, arg, ..
                },
                _,
            ) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(arg, Sarg::Eq(ColId(3), AlgebraicValue::U8(3)));
                assert_eq!(prefix, vec![(ColId(2), AlgebraicValue::U8(2))]);
            }
            plan => panic!("unexpected plan: {:#?}", plan),
        };
    }

    #[test]
    fn limit() {
        let t_id = TableId(1);

        let t = Arc::new(schema(
            t_id,
            "t",
            &[("x", AlgebraicType::U8), ("y", AlgebraicType::U8)],
            &[&[0]],
            &[],
            None,
        ));

        let db = SchemaViewer {
            schemas: vec![t.clone()],
        };

        let compile = |sql| {
            let stmt = parse_and_type_sql(sql, &db, &AuthCtx::for_testing()).unwrap();
            let Statement::Select(select) = stmt else {
                unreachable!()
            };
            compile_select_list(select).optimize().unwrap()
        };

        let plan = compile("select * from t limit 5");

        let ProjectListPlan::Name(mut plans) = plan else {
            panic!("expected a qualified wildcard projection {{table_name}}.*")
        };

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            plans.pop().unwrap(),
            ProjectPlan::None(PhysicalPlan::TableScan(TableScan { limit: Some(5), .. }, _))
        ));

        let plan = compile("select * from t where x = 1 limit 5");

        let ProjectListPlan::Name(mut plans) = plan else {
            panic!("expected a qualified wildcard projection {{table_name}}.*")
        };

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            plans.pop().unwrap(),
            ProjectPlan::None(PhysicalPlan::IxScan(IxScan { limit: Some(5), .. }, _))
        ));

        let plan = compile("select * from t where y = 1 limit 5");

        let ProjectListPlan::Limit(plan, 5) = plan else {
            panic!("expected an outer LIMIT")
        };

        assert!(plan.plan_iter().any(|plan| plan.has_filter()));
        assert!(plan.plan_iter().any(|plan| plan.has_table_scan(None)));
    }
}
