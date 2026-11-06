use anyhow::{bail, Result};
use derive_more::From;
use either::Either;
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_expr::{
    expr::{AggType, CollectViews, Expr, Param, ViewArgs, ViewCall},
    StatementSource,
};
use spacetimedb_lib::{query::Delta, sats::size_of::SizeOf, AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_primitives::{ColId, ColList, ColOrCols, ColSet, IndexId};
use spacetimedb_schema::schema::{IndexSchema, TableSchema, VIEW_ARG_HASH_COL};
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use spacetimedb_table::table::RowRef;
use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    mem,
    ops::{Bound, Deref, DerefMut},
    sync::Arc,
};

use crate::rules::{
    ComputePositions, HashToIxJoin, IxScanFromPredicates, PushConstAnd, PushConstEq, PushLimit, ReorderDeltaJoinRhs,
    ReorderHashJoin, RewriteRule, UniqueHashJoinRule, UniqueIxJoinRule,
};

/// Table aliases are replaced with labels in the physical plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, From)]
pub struct Label(pub usize);

/// Resolves formal parameters during expression evaluation.
///
/// Physical plans store parameter slots.
/// Concrete variable bindings are supplied at runtime
/// and implement this minimal interface.
pub trait ParamResolver {
    fn resolve_param(&self, param: ParamSlot, ty: &AlgebraicType) -> AlgebraicValue;
}

/// A runtime parameter slot in a physical plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSlot(pub u16);

impl ParamSlot {
    /// The slot of the `i`-th binding value
    pub fn binding(i: u16) -> Self {
        Self(PARAM_BINDING_FIRST.0.checked_add(i).expect("binding slot out of range"))
    }

    /// Is this the slot of a binding value?
    pub fn is_binding(self) -> bool {
        self.binding_index().is_some()
    }

    /// The index of the binding value in this slot, if it holds one
    pub fn binding_index(self) -> Option<u16> {
        self.0.checked_sub(PARAM_BINDING_FIRST.0)
    }
}

pub const PARAM_SENDER: ParamSlot = ParamSlot(0);
pub const PARAM_VIEW_ARG_HASH_EMPTY: ParamSlot = ParamSlot(1);
pub const PARAM_VIEW_ARG_HASH_SENDER: ParamSlot = ParamSlot(2);

/// The first slot after the built-ins, so `ExecutionParams` rejects binding slots
pub const PARAM_BINDING_FIRST: ParamSlot = ParamSlot(3);

/// Where the value of a binding param comes from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamSource {
    /// A literal lifted out of the query
    Literal(AlgebraicValue),
    /// A built-in, such as `:sender`, resolved for each subscriber
    Builtin(ParamSlot),
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl CollectViews for ProjectPlan {
    fn collect_views(&self, views: &mut HashSet<ViewCall>) {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan.collect_views(views),
        }
    }
}

impl ProjectPlan {
    /// Puts the plan in the form `optimize` starts from,
    /// in which plans that differ only in the order of their predicates are equal
    pub fn canonicalize(self) -> Self {
        match self {
            Self::None(plan) => Self::None(plan.map(&PhysicalPlan::canonicalize)),
            Self::Name(plan, label, pos) => Self::Name(plan.map(&PhysicalPlan::canonicalize), label, pos),
        }
    }

    /// Feeds `state` the parts of this plan that `Eq` compares, so equal plans hash equally
    pub fn hash_structure(&self, state: &mut impl Hasher) {
        match self {
            Self::None(plan) => {
                0u8.hash(state);
                plan.hash_structure(state);
            }
            Self::Name(plan, label, pos) => {
                1u8.hash(state);
                label.hash(state);
                pos.hash(state);
                plan.hash_structure(state);
            }
        }
    }

    /// Puts a lookup of the matching bindings at the top of a template's plan.
    /// Each predicate that reads a binding moves from its filter to the lookup:
    /// `field = binding` becomes the probe key, and the others read the binding row above it.
    /// `indexes` holds the columns of each binding index of the template, and gains the probed one if it's new.
    /// Run before `optimize`.
    pub fn insert_binding_lookup(self, indexes: &mut Vec<ColList>) -> Self {
        let (plan, returns) = match self {
            Self::Name(plan, label, _) => (plan, Some(label)),
            Self::None(plan) => (plan, None),
        };

        let mut lifted = vec![];
        let plan = plan.lift_binding_predicates(&mut lifted);
        // Binding equalities first, by slot, as in `canonicalize`
        lifted.sort_by_key(|expr| expr.binding_eq_index().map_or(u32::MAX, u32::from));

        // In an unoptimized plan, only scans introduce labels
        let mut labels = vec![];
        let mut deltas = vec![];
        plan.visit(&mut |plan| {
            if let PhysicalPlan::TableScan(scan, label) = plan {
                labels.push(*label);
                deltas.extend(scan.delta.map(|_| *label));
            }
        });
        // The lookup widens each tuple, so name the returned rows
        let returns = returns.unwrap_or_else(|| match labels[..] {
            [label] => label,
            _ => panic!("whole rows need exactly one table scan: {plan:#?}"),
        });
        let label = Label(labels.iter().map(|label| label.0 + 1).max().unwrap_or_default());

        // Binding row column `k + 1` holds binding `k`, after the id
        let column = |k: u16| k as usize + 1;
        let mut key = vec![];
        let mut cols = vec![];
        let mut rest = vec![];
        for expr in lifted {
            match (expr.binding_eq_index(), expr) {
                // A binding used twice is probed once and compared the second time
                (Some(k), PhysicalExpr::BinOp(_, field, _)) if !cols.contains(&column(k)) => {
                    let PhysicalExpr::Field(field) = *field else {
                        unreachable!("a binding equality compares a field");
                    };
                    key.push(field);
                    cols.push(column(k));
                }
                (_, mut expr) => {
                    expr.visit_mut(&mut |expr| {
                        if let PhysicalExpr::Param(slot, _) = expr
                            && let Some(k) = slot.binding_index()
                        {
                            *expr = PhysicalExpr::Field(TupleField {
                                label,
                                label_pos: None,
                                field_pos: column(k),
                            });
                        }
                    });
                    rest.push(expr);
                }
            }
        }

        let candidates = match key.is_empty() {
            true => BindingCandidates::All,
            false => {
                // A template's fragments share its bindings, so they share its indexes
                let cols = ColList::from_iter(cols.into_iter().map(ColId::from));
                let i = match indexes.iter().position(|index| *index == cols) {
                    Some(i) => i,
                    None => {
                        indexes.push(cols);
                        indexes.len() - 1
                    }
                };
                let index = BindingIndexId(u32::try_from(i).expect("too many binding indexes"));
                BindingCandidates::Probe(BindingProbe { index, key })
            }
        };
        // In a join, when the changed table alone gives the key, look up right after its delta scan,
        // so that changed rows no binding matches never reach the join
        let early = match (&candidates, &deltas[..]) {
            (BindingCandidates::Probe(probe), [delta]) if labels.len() > 1 => {
                probe.key.iter().all(|field| field.label == *delta).then_some(*delta)
            }
            _ => None,
        };
        let lookup = BindingLookup { label, candidates };
        let plan = match early {
            Some(delta) => plan.look_up_after_scan(delta, &mut Some(lookup)),
            None => PhysicalPlan::BindingLookup(Box::new(plan), lookup),
        };
        let plan = match rest.len() {
            0 => plan,
            1 => PhysicalPlan::Filter(Box::new(plan), rest.remove(0)),
            _ => PhysicalPlan::Filter(Box::new(plan), PhysicalExpr::LogOp(LogOp::And, rest)),
        };
        Self::Name(plan, returns, None)
    }

    pub fn optimize(self) -> Result<Self> {
        match self {
            Self::None(plan) => {
                let plan = plan.optimize(vec![])?;
                // Only single row tuples can be returned as whole rows
                if plan.nfields() != 1 {
                    bail!("Cannot return whole rows from a plan that returns tuples")
                }
                Ok(Self::None(plan))
            }
            Self::Name(plan, label, _) => {
                // Binding rows hold the binding ids, so the root needs them too
                let mut reqs = vec![label];
                reqs.extend(plan.binding_label());
                let plan = plan.optimize(reqs)?;
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

    /// The position of the binding row in each tuple, if this plan has a binding lookup
    pub fn binding_position(&self) -> Option<usize> {
        let position = |label| self.position(&label).expect("the binding row reaches the root");
        self.binding_label().map(position)
    }

    /// Does this plan select or return whole (unprojected) rows from a single table?
    pub fn return_table(&self) -> Option<Arc<TableSchema>> {
        match self {
            Self::None(plan) => plan.return_table(),
            Self::Name(plan, label, _) => plan.find_table_schema(label),
        }
    }

    /// Does this plan select or return whole (unprojected) rows from a view?
    pub fn returns_view_table(&self) -> bool {
        self.return_table().is_some_and(|schema| schema.is_view())
    }

    /// Does this plan read from an (anonymous) view?
    pub fn reads_from_view(&self, anonymous: bool) -> bool {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan.reads_from_view(anonymous),
        }
    }

    /// Does this plan use an event table as the lookup (rhs) table in a semi-join?
    pub fn reads_from_event_table(&self) -> bool {
        match self {
            Self::None(plan) | Self::Name(plan, ..) => plan.reads_from_event_table(),
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

    /// Does this plan select or return whole (unprojected) rows from a single table?
    pub fn return_table(&self) -> Option<Arc<TableSchema>> {
        match self {
            Self::Name(plans) => plans.first().and_then(ProjectPlan::return_table),
            Self::Limit(plan, _) => plan.return_table(),
            Self::List(..) | Self::Agg(..) => None,
        }
    }

    /// Does this plan select or return whole (unprojected) rows from a view?
    pub fn returns_view_table(&self) -> bool {
        self.return_table().is_some_and(|schema| schema.is_view())
    }

    /// Does this plan read from an (anonymous) view?
    pub fn reads_from_view(&self, anonymous: bool) -> bool {
        match self {
            Self::Limit(plan, _) => plan.reads_from_view(anonymous),
            Self::Name(plans) => plans.iter().any(|plan| plan.reads_from_view(anonymous)),
            Self::List(plans, ..) | Self::Agg(plans, ..) => plans.iter().any(|plan| plan.reads_from_view(anonymous)),
        }
    }

    /// Does this plan use an event table as the lookup (rhs) table in a semi-join?
    pub fn reads_from_event_table(&self) -> bool {
        match self {
            Self::Limit(plan, _) => plan.reads_from_event_table(),
            Self::Name(plans) => plans.iter().any(|plan| plan.reads_from_event_table()),
            Self::List(plans, ..) | Self::Agg(plans, ..) => plans.iter().any(|plan| plan.reads_from_event_table()),
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// A join with the matching subscription bindings
    BindingLookup(Box<PhysicalPlan>, BindingLookup),
}

impl CollectViews for PhysicalPlan {
    fn collect_views(&self, views: &mut HashSet<ViewCall>) {
        self.visit(&mut |plan| {
            let (view_info, view_args) = match plan {
                Self::TableScan(scan, _) => (&scan.schema.view_info, Some(&scan.view_args)),
                Self::IxScan(scan, _) => (&scan.schema.view_info, None),
                Self::IxJoin(join, _) => (&join.rhs.view_info, None),
                _ => return,
            };
            if let Some(info) = view_info {
                // An optimized scan has lost the call's args, so never fill in empty args for them.
                assert!(
                    view_args.is_some() || info.params.elements.is_empty(),
                    "parameterized view calls must be collected before optimization",
                );
                let args = view_args.and_then(Option::as_ref).map(|v| v.args.clone());
                views.insert(ViewCall {
                    view_id: info.view_id,
                    args: args.unwrap_or_default(),
                });
            }
        });
    }
}

impl PhysicalPlan {
    /// Walks the plan tree and calls `f` on every op
    pub fn visit(&self, f: &mut impl FnMut(&Self)) {
        f(self);
        match self {
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) | Self::BindingLookup(input, _) => {
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
            Self::IxJoin(IxJoin { lhs: input, .. }, _) | Self::Filter(input, _) | Self::BindingLookup(input, _) => {
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
            Self::BindingLookup(input, lookup) => Self::BindingLookup(Box::new(input.map(f)), lookup),
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
            Self::BindingLookup(input, lookup) => {
                if matches(&input) {
                    return Ok(Self::BindingLookup(Box::new(input.map_if(f, ok)?), lookup));
                }
                Self::BindingLookup(input, lookup)
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
            .apply_rec::<ReorderHashJoin>()?
            .apply_rec::<HashToIxJoin>()?
            .apply_rec::<IxScanFromPredicates>()?
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
                Self::IxJoin(IxJoin { probe, .. }, _) => {
                    probe.visit(&mut |expr| {
                        if let PhysicalExpr::Field(TupleField { label_pos: None, .. }) = expr {
                            unresolved_name = true;
                        }
                    });
                }
                Self::BindingLookup(_, lookup) => {
                    if lookup.key_fields().any(|field| field.label_pos.is_none()) {
                        unresolved_name = true;
                    }
                }
                Self::HashJoin(
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

    /// A scan of `scan.schema`, scoped to the rows of a view call if it reads a view.
    ///
    /// If a view has private arguments, its backing table has an `arg_hash` column.
    /// This column tracks which rows belong to which argument tuple.
    ///
    /// As a result, queries over views cannot read the entire backing table.
    /// They must only select the rows corresponding to the view arguments used
    /// by each view reference. Hence we must add an implicit selection over
    /// these types of views.
    ///
    /// Ex.
    /// ```sql
    /// SELECT * FROM my_view
    /// ```
    ///
    /// becomes the equivalent of
    /// ```sql
    /// SELECT * FROM my_view WHERE arg_hash = <current view arg hash>
    /// ```
    pub(crate) fn scoped_scan(scan: TableScan, label: Label) -> Self {
        if !scan.schema.is_view() {
            return Self::TableScan(scan, label);
        }
        let arg_hash = match &scan.view_args {
            // A parameterized call's hash was bound at compile time, or lifted into a binding
            Some(view_args) => match &view_args.arg_hash {
                Some(Expr::Value(hash, _)) => PhysicalExpr::Value(hash.clone()),
                Some(Expr::Param(Param::Binding(i), ty)) => PhysicalExpr::Param(ParamSlot::binding(*i), ty.clone()),
                arg_hash => panic!("bind_view_arg_hashes must run before physical compilation: {arg_hash:?}"),
            },
            None => {
                let param = if scan.schema.is_anonymous_view() {
                    PARAM_VIEW_ARG_HASH_EMPTY
                } else {
                    PARAM_VIEW_ARG_HASH_SENDER
                };
                PhysicalExpr::Param(param, AlgebraicType::U256)
            }
        };
        Self::Filter(
            Box::new(Self::TableScan(scan, label)),
            PhysicalExpr::BinOp(
                BinOp::Eq,
                Box::new(arg_hash),
                Box::new(PhysicalExpr::Field(TupleField {
                    label,
                    label_pos: None,
                    field_pos: VIEW_ARG_HASH_COL.idx(),
                })),
            ),
        )
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
                        if matches!(&*value, PhysicalExpr::Value(_) | PhysicalExpr::Param(..))
                            && matches!(&*expr, PhysicalExpr::Field(..)) =>
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
                // Binding equalities first, in slot order, so reordered predicates give the same template.
                // The sort is stable, so the rest keep their order.
                let order_binding_eqs = |expr| match expr {
                    PhysicalExpr::LogOp(LogOp::And, mut exprs) => {
                        exprs.sort_by_key(|expr| expr.binding_eq_index().map_or(u32::MAX, u32::from));
                        PhysicalExpr::LogOp(LogOp::And, exprs)
                    }
                    _ => expr,
                };
                // Flatten ANDs and ORs, move values to rhs, and order binding equalities
                Self::Filter(input, expr.flatten().map(&move_value_to_rhs).map(&order_binding_eqs))
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
        let append_probe_required_labels = |plan: &PhysicalPlan, reqs: &mut Vec<Label>, probe: &PhysicalExpr| {
            probe.visit(&mut |expr| {
                if let PhysicalExpr::Field(TupleField { label, .. }) = expr {
                    append_required_label(plan, reqs, *label);
                }
            });
        };
        match self {
            Self::Filter(input, expr) => {
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(TupleField { label: var, .. }) = expr
                        && !reqs.contains(var)
                    {
                        reqs.push(*var);
                    }
                });
                Self::Filter(Box::new(input.introduce_semijoins(reqs)), expr)
            }
            // Like an index join that keeps both sides, since the binding row holds the binding id
            Self::BindingLookup(input, lookup) => {
                let mut reqs: Vec<_> = reqs.into_iter().filter(|label| label != &lookup.label).collect();
                for field in lookup.key_fields() {
                    append_required_label(&input, &mut reqs, field.label);
                }
                Self::BindingLookup(Box::new(input.introduce_semijoins(reqs)), lookup)
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
                let mut lhs_reqs = vec![];
                append_probe_required_labels(&join.lhs, &mut lhs_reqs, &join.probe);
                let lhs = join.lhs.introduce_semijoins(lhs_reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Rhs)
            }
            Self::IxJoin(join, Semi::All) if reqs.iter().all(|var| *var != join.rhs_label) => {
                append_probe_required_labels(&join.lhs, &mut reqs, &join.probe);
                let lhs = join.lhs.introduce_semijoins(reqs);
                let lhs = Box::new(lhs);
                Self::IxJoin(IxJoin { lhs, ..join }, Semi::Lhs)
            }
            Self::IxJoin(join, Semi::All) => {
                let mut reqs: Vec<_> = reqs.into_iter().filter(|label| label != &join.rhs_label).collect();
                append_probe_required_labels(&join.lhs, &mut reqs, &join.probe);
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
            Self::TableScan(TableScan { schema, .. }, var) => var == label && schema.as_ref().is_unique(&**cols),
            // Is there a unique constraint for these cols + the index cols?
            Self::IxScan(scan, var) if var == label && matches!(&scan.probe, IndexProbe::Point(_)) => {
                var == label
                    && scan
                        .schema
                        .as_ref()
                        .is_unique(&*ColSet::from_iter(cols.iter().chain(scan.index_cols().iter())))
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
            Self::IxJoin(join @ IxJoin { lhs, rhs, .. }, _) => {
                join.single_probe_field().is_some_and(|(_, lhs_field)| {
                    lhs.returns_distinct_values(&lhs_field.label, &ColSet::from(ColId(lhs_field.field_pos as u16)))
                        && rhs.as_ref().is_unique(&**cols)
                })
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
                    if let PhysicalExpr::BinOp(BinOp::Eq, expr, value) = plan
                        && let (PhysicalExpr::Field(proj), PhysicalExpr::Value(..) | PhysicalExpr::Param(..)) =
                            (&**expr, &**value)
                        && proj.label == *label
                    {
                        cols.push(proj.field_pos.into());
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
            Self::TableScan(_, var)
            | Self::IxScan(_, var)
            | Self::IxJoin(IxJoin { rhs_label: var, .. }, _)
            | Self::BindingLookup(_, BindingLookup { label: var, .. }) => var == label,
            _ => false,
        })
    }

    /// How many fields do the tuples returned by this plan have?
    fn nfields(&self) -> usize {
        match self {
            Self::TableScan(..) | Self::IxScan(..) | Self::IxJoin(_, Semi::Rhs) => 1,
            Self::Filter(input, _) => input.nfields(),
            Self::BindingLookup(input, _) => input.nfields() + 1,
            Self::IxJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::IxJoin(join, Semi::All) => join.lhs.nfields() + 1,
            Self::HashJoin(join, Semi::Rhs) => join.rhs.nfields(),
            Self::HashJoin(join, Semi::Lhs) => join.lhs.nfields(),
            Self::HashJoin(join, Semi::All) => join.lhs.nfields() + join.rhs.nfields(),
            Self::NLJoin(lhs, rhs) => lhs.nfields() + rhs.nfields(),
        }
    }

    /// Moves each built-in param, such as `:sender`, to a binding slot.
    /// `params` holds the source of each binding slot, in order, and gains any built-in it hasn't seen.
    /// A template's RLS fragments share it, so they agree on the slots. Run before `optimize`.
    pub fn remap_builtin_params(&mut self, params: &mut Vec<(ParamSource, AlgebraicType)>) {
        self.visit_exprs_mut(&mut |expr| {
            let PhysicalExpr::Param(slot, ty) = expr else {
                return;
            };
            if slot.is_binding() {
                return;
            }
            // `:sender` resolves differently as an identity and as bytes
            let builtin = (ParamSource::Builtin(*slot), ty.clone());
            let i = match params.iter().position(|seen| *seen == builtin) {
                Some(i) => i,
                None => {
                    params.push(builtin);
                    params.len() - 1
                }
            };
            *slot = ParamSlot::binding(u16::try_from(i).expect("binding slot out of range"));
        });
        // Binding params don't fall back to `ExecutionParams`, so no built-in may be left
        self.visit_exprs_mut(&mut |expr| {
            if let PhysicalExpr::Param(slot, _) = expr {
                assert!(slot.is_binding(), "built-in param left in a template: {slot:?}");
            }
        });
    }

    /// Replaces each param, such as `:sender`, with its value from `params`
    pub fn inline_params(&mut self, params: &impl ParamResolver) {
        self.visit_exprs_mut(&mut |expr| {
            if let PhysicalExpr::Param(slot, ty) = expr {
                *expr = PhysicalExpr::Value(params.resolve_param(*slot, ty));
            }
        });
    }

    /// Feeds `state` the parts of this plan that `Eq` compares.
    /// Operators that only appear once a plan is optimized hash as just their kind.
    fn hash_structure(&self, state: &mut impl Hasher) {
        self.visit(&mut |plan| {
            mem::discriminant(plan).hash(state);
            match plan {
                Self::TableScan(scan, label) => {
                    scan.schema.table_id.hash(state);
                    scan.limit.hash(state);
                    scan.delta.map(|delta| mem::discriminant(&delta)).hash(state);
                    label.hash(state);
                }
                Self::Filter(_, expr) => expr.hash_structure(state),
                Self::HashJoin(join, semi) => {
                    join.lhs_field.hash(state);
                    join.rhs_field.hash(state);
                    join.unique.hash(state);
                    mem::discriminant(semi).hash(state);
                }
                Self::IxScan(..) | Self::IxJoin(..) | Self::NLJoin(..) | Self::BindingLookup(..) => {}
            }
        });
    }

    /// Moves each predicate that reads a binding out of the plan's filters into `lifted`, once each.
    /// An unoptimized join keeps both sides, so its filters can move above it.
    fn lift_binding_predicates(self, lifted: &mut Vec<PhysicalExpr>) -> Self {
        match self {
            Self::Filter(input, expr) => {
                let input = input.lift_binding_predicates(lifted);
                let exprs = match expr {
                    PhysicalExpr::LogOp(LogOp::And, exprs) => exprs,
                    expr => vec![expr],
                };
                let (reads, mut keep): (Vec<_>, Vec<_>) = exprs.into_iter().partition(PhysicalExpr::reads_binding);
                for expr in reads {
                    // RLS can repeat a predicate
                    if !lifted.contains(&expr) {
                        lifted.push(expr);
                    }
                }
                match keep.len() {
                    0 => input,
                    1 => Self::Filter(Box::new(input), keep.remove(0)),
                    _ => Self::Filter(Box::new(input), PhysicalExpr::LogOp(LogOp::And, keep)),
                }
            }
            Self::NLJoin(lhs, rhs) => Self::NLJoin(
                Box::new(lhs.lift_binding_predicates(lifted)),
                Box::new(rhs.lift_binding_predicates(lifted)),
            ),
            Self::HashJoin(join, Semi::All) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.lift_binding_predicates(lifted)),
                    rhs: Box::new(join.rhs.lift_binding_predicates(lifted)),
                    ..join
                },
                Semi::All,
            ),
            Self::TableScan(..) => self,
            _ => panic!("not an unoptimized plan: {self:#?}"),
        }
    }

    /// Puts `lookup` directly above the scan labeled `scan`, in an unoptimized plan
    fn look_up_after_scan(self, scan: Label, lookup: &mut Option<BindingLookup>) -> Self {
        match self {
            Self::TableScan(_, label) if label == scan => {
                Self::BindingLookup(Box::new(self), lookup.take().expect("a label has one scan"))
            }
            Self::Filter(input, expr) => Self::Filter(Box::new(input.look_up_after_scan(scan, lookup)), expr),
            Self::NLJoin(lhs, rhs) => Self::NLJoin(
                Box::new(lhs.look_up_after_scan(scan, lookup)),
                Box::new(rhs.look_up_after_scan(scan, lookup)),
            ),
            Self::HashJoin(join, semi) => Self::HashJoin(
                HashJoin {
                    lhs: Box::new(join.lhs.look_up_after_scan(scan, lookup)),
                    rhs: Box::new(join.rhs.look_up_after_scan(scan, lookup)),
                    ..join
                },
                semi,
            ),
            plan => plan,
        }
    }

    /// Calls `f` on each node of each expression in the plan
    fn visit_exprs_mut(&mut self, f: &mut impl FnMut(&mut PhysicalExpr)) {
        self.visit_mut(&mut |plan| match plan {
            Self::Filter(_, expr) => expr.visit_mut(f),
            Self::IxScan(scan, _) => scan.probe.visit_mut(f),
            Self::IxJoin(join, _) => join.probe.visit_mut(f),
            Self::TableScan(..) | Self::HashJoin(..) | Self::NLJoin(..) | Self::BindingLookup(..) => {}
        });
    }

    /// The label of this plan's binding lookup, if it has one
    fn binding_label(&self) -> Option<Label> {
        let mut labels = vec![];
        self.visit(&mut |plan| {
            if let PhysicalPlan::BindingLookup(_, lookup) = plan {
                labels.push(lookup.label);
            }
        });
        // A template has one binding relation
        assert!(labels.len() <= 1, "more than one binding lookup: {self:#?}");
        labels.first().copied()
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
                PhysicalPlan::BindingLookup(input, BindingLookup { label, .. }) => {
                    find(input, labels);
                    labels.push(*label);
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
    /// A filter or binding lookup on such a scan counts too, so the delta still drives a join.
    pub fn is_delta_scan(&self) -> bool {
        match self {
            Self::TableScan(scan, _) => scan.delta.is_some(),
            Self::IxScan(scan, _) => scan.delta.is_some(),
            Self::Filter(input, _) | Self::BindingLookup(input, _) => input.is_delta_scan(),
            _ => false,
        }
    }

    /// Does this plan select or return whole (unprojected) rows from a single table?
    pub fn return_table(&self) -> Option<Arc<TableSchema>> {
        match self {
            Self::TableScan(scan, _) => Some(scan.schema.clone()),
            Self::IxScan(scan, _) => Some(scan.schema.clone()),
            Self::Filter(input, _) => input.return_table(),
            Self::IxJoin(join, Semi::Lhs) => join.lhs.return_table(),
            Self::IxJoin(join, Semi::Rhs) => Some(join.rhs.clone()),
            Self::HashJoin(join, Semi::Lhs) => join.lhs.return_table(),
            Self::HashJoin(join, Semi::Rhs) => join.rhs.return_table(),
            Self::IxJoin(_, Semi::All) | Self::HashJoin(_, Semi::All) | Self::NLJoin(..) | Self::BindingLookup(..) => {
                None
            }
        }
    }

    /// Returns the [`TableSchema`] for a return label.
    /// Returns `None` if the plan does not return this label.
    pub fn find_table_schema(&self, name: &Label) -> Option<Arc<TableSchema>> {
        match self {
            Self::TableScan(scan, label) if name == label => Some(scan.schema.clone()),
            Self::IxScan(scan, label) if name == label => Some(scan.schema.clone()),
            Self::Filter(input, _) | Self::BindingLookup(input, _) => input.find_table_schema(name),
            Self::IxJoin(join, Semi::Rhs | Semi::All) if name == &join.rhs_label => Some(join.rhs.clone()),
            Self::IxJoin(join, Semi::Lhs | Semi::All) => join.lhs.find_table_schema(name),
            Self::HashJoin(join, Semi::Lhs) => join.lhs.find_table_schema(name),
            Self::HashJoin(join, Semi::Rhs) => join.rhs.find_table_schema(name),
            Self::HashJoin(HashJoin { lhs, rhs, .. }, Semi::All) | Self::NLJoin(lhs, rhs) => {
                lhs.find_table_schema(name).or_else(|| rhs.find_table_schema(name))
            }
            _ => None,
        }
    }

    /// Does this plan select or return whole (unprojected) rows from a view?
    pub fn returns_view_table(&self) -> bool {
        self.return_table().is_some_and(|schema| schema.is_view())
    }

    /// Does this plan read from an (anonymous) view?
    pub fn reads_from_view(&self, anonymous: bool) -> bool {
        self.any(&|plan| match plan {
            Self::TableScan(scan, _) if anonymous => scan.schema.is_anonymous_view(),
            Self::TableScan(scan, _) => scan.schema.is_view() && !scan.schema.is_anonymous_view(),
            Self::IxScan(scan, _) if anonymous => scan.schema.is_anonymous_view(),
            Self::IxScan(scan, _) => scan.schema.is_view() && !scan.schema.is_anonymous_view(),
            Self::IxJoin(join, _) if anonymous => join.rhs.is_anonymous_view(),
            Self::IxJoin(join, _) => join.rhs.is_view() && !join.rhs.is_anonymous_view(),
            _ => false,
        })
    }

    /// Does this plan use an event table as the lookup (rhs) table in a semi-join?
    ///
    /// Note, we only care about index joins because this method is only relevant for subscriptions,
    /// and index joins are the only type of join allowed in subscriptions.
    pub fn reads_from_event_table(&self) -> bool {
        self.any(&|plan| match plan {
            Self::IxJoin(join, _) => join.rhs.is_event,
            _ => false,
        })
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
    /// The arguments of a parameterized view call, with its bound `arg_hash`.
    /// `None` for tables and for views without parameters.
    pub view_args: Option<ViewArgs>,
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
    /// The expression used to probe the index.
    pub probe: IndexProbe,
}

/// The key expression for an index scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexProbe {
    Point(PhysicalExpr),
    /// NOTE(centril): We currently never construct this variant.
    /// We do have non-ranged hash indices.
    /// This means that when we get around to using this variant,
    /// we must change the rewrite rules such that we do not emit
    /// [`IxScan`] on a hash index.
    ///
    /// Moreover, an equality scan (the variant above)
    /// `(a0, b0)` on an index `(a, b, c)` is actually a ranged scan.
    /// We also currently do not emit such `IxScan`s where the number
    /// of equalities provided are fewer than the number of columns in the index.
    /// When we do, we must also account for hash indices in the rewrite rules.
    Range(Bound<PhysicalExpr>, Bound<PhysicalExpr>),
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
    /// Is the index a unique constraint index?
    pub unique: bool,
    /// The point expression for computing probe values from the lhs.
    pub probe: PhysicalExpr,
    // Is the rhs a delta table?
    pub rhs_delta: Option<Delta>,
}

/// An index on a subscription template's binding relation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingIndexId(pub u32);

/// Appends each matching binding row to the input tuple
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingLookup {
    /// The label of the binding relation in the output tuple
    pub label: Label,
    /// How candidate bindings are found
    pub candidates: BindingCandidates,
}

/// Which bindings a [`BindingLookup`] pairs with each input tuple
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingCandidates {
    /// Every binding
    All,
    /// The bindings found by probing one index
    Probe(BindingProbe),
    // TODO: Probe each branch of an `OR` instead of using `All`
}

/// Like the probe of an [`IxJoin`], but into a binding index
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingProbe {
    /// The binding index to probe
    pub index: BindingIndexId,
    /// The probe key, in the order of the index's columns
    pub key: Vec<TupleField>,
}

impl BindingLookup {
    /// The probes this lookup makes for each input tuple
    pub fn probes(&self) -> &[BindingProbe] {
        match &self.candidates {
            BindingCandidates::All => &[],
            BindingCandidates::Probe(probe) => std::slice::from_ref(probe),
        }
    }

    fn probes_mut(&mut self) -> &mut [BindingProbe] {
        match &mut self.candidates {
            BindingCandidates::All => &mut [],
            BindingCandidates::Probe(probe) => std::slice::from_mut(probe),
        }
    }

    /// The input fields this lookup reads
    pub fn key_fields(&self) -> impl Iterator<Item = &TupleField> {
        self.probes().iter().flat_map(|probe| probe.key.iter())
    }

    /// The input fields this lookup reads, mutably
    pub fn key_fields_mut(&mut self) -> impl Iterator<Item = &mut TupleField> {
        self.probes_mut().iter_mut().flat_map(|probe| probe.key.iter_mut())
    }
}

/// Build a single physical key expression for an index probe.
///
/// Single-column keys are represented directly. Composite keys are represented
/// as products and evaluated at runtime. This intentionally does not fold
/// all-literal products in the physical plan.
pub fn index_key_expr(mut parts: Vec<PhysicalExpr>) -> PhysicalExpr {
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        PhysicalExpr::Product(parts)
    }
}

fn schema_index_cols(schema: &TableSchema, index_id: IndexId) -> ColOrCols<'_> {
    schema
        .indexes
        .iter()
        .find(|index| index.index_id == index_id)
        .expect("physical plan referenced missing index")
        .index_algorithm
        .columns()
}

fn align_index_key_components<'a>(
    cols: ColOrCols<'_>,
    expr: &'a PhysicalExpr,
) -> Option<Vec<(ColId, &'a PhysicalExpr)>> {
    let cols = cols.iter().collect::<Vec<_>>();
    match expr {
        PhysicalExpr::Product(parts) if parts.len() == cols.len() => Some(cols.into_iter().zip(parts.iter()).collect()),
        _ if cols.len() == 1 => Some(vec![(cols[0], expr)]),
        _ => None,
    }
}

impl IndexProbe {
    pub fn visit(&self, f: &mut impl FnMut(&PhysicalExpr)) {
        match self {
            Self::Point(expr) => expr.visit(f),
            Self::Range(lower, upper) => {
                match lower {
                    Bound::Included(expr) | Bound::Excluded(expr) => expr.visit(f),
                    Bound::Unbounded => {}
                }
                match upper {
                    Bound::Included(expr) | Bound::Excluded(expr) => expr.visit(f),
                    Bound::Unbounded => {}
                }
            }
        }
    }

    pub fn visit_mut(&mut self, f: &mut impl FnMut(&mut PhysicalExpr)) {
        match self {
            Self::Point(expr) => expr.visit_mut(f),
            Self::Range(lower, upper) => {
                match lower {
                    Bound::Included(expr) | Bound::Excluded(expr) => expr.visit_mut(f),
                    Bound::Unbounded => {}
                }
                match upper {
                    Bound::Included(expr) | Bound::Excluded(expr) => expr.visit_mut(f),
                    Bound::Unbounded => {}
                }
            }
        }
    }
}

impl IxScan {
    pub fn index_cols(&self) -> ColOrCols<'_> {
        schema_index_cols(&self.schema, self.index_id)
    }

    pub fn literal_eq_terms(&self) -> Option<Vec<(ColId, AlgebraicValue)>> {
        self.point_terms(|expr| match expr {
            PhysicalExpr::Value(value) => Some(value.clone()),
            _ => None,
        })
    }

    fn point_terms(
        &self,
        mut value_of: impl FnMut(&PhysicalExpr) -> Option<AlgebraicValue>,
    ) -> Option<Vec<(ColId, AlgebraicValue)>> {
        let IndexProbe::Point(expr) = &self.probe else {
            return None;
        };

        let mut terms = Vec::new();
        for (col, expr) in align_index_key_components(self.index_cols(), expr)? {
            terms.push((col, value_of(expr)?));
        }
        Some(terms)
    }
}

impl IxJoin {
    pub fn index_cols(&self) -> ColOrCols<'_> {
        schema_index_cols(&self.rhs, self.rhs_index)
    }

    pub fn probe_field_components(&self) -> Vec<(ColId, &TupleField)> {
        align_index_key_components(self.index_cols(), &self.probe)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(col, expr)| match expr {
                PhysicalExpr::Field(field) => Some((col, field)),
                _ => None,
            })
            .collect()
    }

    pub fn single_probe_field(&self) -> Option<(ColId, &TupleField)> {
        let fields = self.probe_field_components();
        let [(col, field)] = fields.as_slice() else {
            return None;
        };
        Some((*col, *field))
    }
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
    /// A composite value expression, used primarily for multi-column index keys.
    Product(Vec<PhysicalExpr>),
    /// A binary expression
    BinOp(BinOp, Box<PhysicalExpr>, Box<PhysicalExpr>),
    /// A constant algebraic value
    Value(AlgebraicValue),
    /// A runtime parameter.
    Param(ParamSlot, AlgebraicType),
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
            Self::Product(exprs) => {
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
            Self::Product(exprs) => {
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
            param @ Self::Param(..) => param,
            field @ Self::Field(..) => field,
            Self::BinOp(op, a, b) => Self::BinOp(op, Box::new(a.map(f)), Box::new(b.map(f))),
            Self::LogOp(op, exprs) => Self::LogOp(op, exprs.into_iter().map(|expr| expr.map(f)).collect()),
            Self::Product(exprs) => Self::Product(exprs.into_iter().map(|expr| expr.map(f)).collect()),
        }
    }

    /// Evaluate this boolean expression over `row` with runtime parameters.
    pub fn eval_bool_with_params(
        &self,
        row: &impl ProjectField,
        params: &impl ParamResolver,
        bytes_scanned: &mut usize,
    ) -> bool {
        self.eval_with_params(row, params, bytes_scanned)
            .as_bool()
            .copied()
            .unwrap_or(false)
    }

    /// Evaluate this expression over `row` with runtime parameters.
    pub fn eval_with_params<'a>(
        &'a self,
        row: &impl ProjectField,
        params: &impl ParamResolver,
        bytes_scanned: &mut usize,
    ) -> Cow<'a, AlgebraicValue> {
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
                &a.eval_with_params(row, params, bytes_scanned),
                &b.eval_with_params(row, params, bytes_scanned),
            )),
            Self::LogOp(LogOp::And, exprs) => into(
                exprs
                    .iter()
                    // ALL is equivalent to AND
                    .all(|expr| expr.eval_bool_with_params(row, params, bytes_scanned)),
            ),
            Self::LogOp(LogOp::Or, exprs) => into(
                exprs
                    .iter()
                    // ANY is equivalent to OR
                    .any(|expr| expr.eval_bool_with_params(row, params, bytes_scanned)),
            ),
            Self::Product(exprs) => Cow::Owned(AlgebraicValue::product(
                exprs
                    .iter()
                    .map(|expr| expr.eval_with_params(row, params, bytes_scanned).into_owned())
                    .collect::<ProductValue>(),
            )),
            Self::Field(field) => {
                let value = row.project(field);
                *bytes_scanned += value.size_of();
                Cow::Owned(value)
            }
            Self::Param(param, ty) => Cow::Owned(params.resolve_param(*param, ty)),
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
            Self::Product(exprs) => Self::Product(exprs.into_iter().map(Self::flatten).collect()),
            Self::Field(..) | Self::Value(..) | Self::Param(..) => self,
        }
    }

    /// The binding index of a `field = param`, if the param is a binding
    fn binding_eq_index(&self) -> Option<u16> {
        match self {
            Self::BinOp(BinOp::Eq, a, b) => match (&**a, &**b) {
                (Self::Field(_), Self::Param(slot, _)) => slot.binding_index(),
                _ => None,
            },
            _ => None,
        }
    }

    /// Feeds `state` the parts of this expression that `Eq` compares
    fn hash_structure(&self, state: &mut impl Hasher) {
        self.visit(&mut |expr| {
            mem::discriminant(expr).hash(state);
            match expr {
                // The number of children tells where each one ends
                Self::LogOp(op, exprs) => {
                    mem::discriminant(op).hash(state);
                    exprs.len().hash(state);
                }
                Self::BinOp(op, ..) => mem::discriminant(op).hash(state),
                Self::Value(value) => value.hash(state),
                Self::Param(slot, _) => slot.0.hash(state),
                Self::Field(field) => field.hash(state),
                Self::Product(exprs) => exprs.len().hash(state),
            }
        });
    }

    /// Does this expression read a binding param?
    fn reads_binding(&self) -> bool {
        let mut reads = false;
        self.visit(&mut |expr| {
            reads |= matches!(expr, Self::Param(slot, _) if slot.is_binding());
        });
        reads
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
    use spacetimedb_data_structures::map::HashSet;
    use spacetimedb_expr::{
        check::{SchemaView, TypingResult},
        expr::{CollectViews, ProjectName, ViewCall},
        statement::{parse_and_type_sql, Statement},
    };
    use spacetimedb_lib::{
        db::auth::{StAccess, StTableType},
        identity::AuthCtx,
        query::Delta,
        view_instance_arg_hash_value, AlgebraicType, AlgebraicValue, Identity, ProductType, ProductValue,
    };
    use spacetimedb_primitives::{ColId, ColList, ColSet, TableId, ViewId};
    use spacetimedb_schema::{
        def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData},
        identifier::Identifier,
        schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableOrViewSchema, TableSchema, ViewDefInfo},
        table_name::TableName,
    };
    use spacetimedb_sql_parser::ast::{BinOp, LogOp};

    use crate::{
        compile::{compile_select, compile_select_list},
        plan::{
            BindingCandidates, BindingIndexId, BindingLookup, BindingProbe, IndexProbe, IxScan, Label, PhysicalPlan,
            ProjectListPlan, Semi, TupleField,
        },
    };

    use super::{
        ParamSource, PhysicalExpr, ProjectPlan, TableScan, PARAM_SENDER, PARAM_VIEW_ARG_HASH_EMPTY,
        PARAM_VIEW_ARG_HASH_SENDER, VIEW_ARG_HASH_COL,
    };

    struct SchemaViewer {
        schemas: Vec<Arc<TableOrViewSchema>>,
    }

    impl SchemaView for SchemaViewer {
        fn table_id(&self, name: &str) -> Option<TableId> {
            self.schemas
                .iter()
                .find(|schema| schema.table_name.as_ref() == name)
                .map(|schema| schema.table_id)
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableOrViewSchema>> {
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
    ) -> TableOrViewSchema {
        TableOrViewSchema::from(Arc::new(TableSchema::new(
            table_id,
            TableName::new(Identifier::for_test(table_name)),
            None,
            columns
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| ColumnSchema {
                    table_id,
                    col_name: Identifier::for_test(*name),
                    col_pos: i.into(),
                    col_type: ty.clone(),
                    alias: None,
                })
                .collect(),
            indexes
                .iter()
                .enumerate()
                .map(|(i, cols)| IndexSchema {
                    table_id,
                    index_id: i.into(),
                    index_name: "".into(),
                    index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm {
                        columns: ColList::from_iter(cols.iter().copied()),
                    }),
                    alias: None,
                })
                .collect(),
            unique
                .iter()
                .enumerate()
                .map(|(i, cols)| ConstraintSchema {
                    table_id,
                    constraint_id: i.into(),
                    constraint_name: "".into(),
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
            false,
            None,
            None,
        )))
    }

    /// A wrapper around [spacetimedb_expr::check::parse_and_type_sub] that takes a dummy [AuthCtx]
    fn parse_and_type_sub(sql: &str, tx: &impl SchemaView) -> TypingResult<ProjectName> {
        spacetimedb_expr::check::parse_and_type_sub(sql, tx, &AuthCtx::for_testing()).map(|(plan, _)| plan)
    }

    fn assert_join_probe(join: &super::IxJoin, rhs_col: ColId, lhs_field_pos: usize) {
        assert_eq!(
            join.single_probe_field().map(|(col, field)| (col, field.field_pos)),
            Some((rhs_col, lhs_field_pos))
        );
    }

    fn value(value: AlgebraicValue) -> PhysicalExpr {
        PhysicalExpr::Value(value)
    }

    fn product(values: Vec<AlgebraicValue>) -> PhysicalExpr {
        PhysicalExpr::Product(values.into_iter().map(value).collect())
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
            proj => panic!("unexpected project: {proj:#?}"),
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
                    plan => panic!("unexpected plan: {plan:#?}"),
                }
            }
            proj => panic!("unexpected project: {proj:#?}"),
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
            proj => panic!("unexpected project: {proj:#?}"),
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
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, b_id);
                assert!(join.unique);
                assert_join_probe(&join, ColId(0), 0);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //       rx
        //      /  \
        //     rx   l
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, l_id);
                assert!(!join.unique);
                assert_join_probe(&join, ColId(1), 1);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(u)  l
        let plan = match plan {
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, l_id);
                assert!(join.unique);
                assert_join_probe(&join, ColId(0), 1);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan: ix(u)
        match plan {
            PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _) => {
                assert_eq!(probe, IndexProbe::Point(value(AlgebraicValue::U64(5))));
                assert_eq!(schema.table_id, u_id);
            }
            plan => panic!("unexpected plan: {plan:#?}"),
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
            proj => panic!("unexpected project: {proj:#?}"),
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
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, p_id);
                assert!(join.unique);
                assert_join_probe(&join, ColId(0), 1);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //         s(v.employee = 5)
        //          |
        //          rx
        //        /    \
        //       rx     w
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs)) => {
                assert!(matches!(
                    (&*lhs, &*rhs),
                    (
                        PhysicalExpr::Field(TupleField { field_pos: 0, .. }),
                        PhysicalExpr::Value(AlgebraicValue::U64(5))
                    )
                ));
                *input
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //         rx
        //        /  \
        //       rx   w
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, w_id);
                assert!(!join.unique);
                assert_join_probe(&join, ColId(1), 1);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //       rx
        //      /  \
        //     rx   w
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, w_id);
                assert!(!join.unique);
                assert_join_probe(&join, ColId(0), 0);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan:
        //     rx
        //    /  \
        // ix(m)  m
        let plan = match plan {
            PhysicalPlan::IxJoin(join, Semi::Rhs) => {
                assert_eq!(join.rhs.table_id, m_id);
                assert!(!join.unique);
                assert_join_probe(&join, ColId(1), 1);
                *join.lhs
            }
            plan => panic!("unexpected plan: {plan:#?}"),
        };

        // Plan: ix(m)
        match plan {
            PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _) => {
                assert_eq!(probe, IndexProbe::Point(value(AlgebraicValue::U64(5))));
                assert_eq!(schema.table_id, m_id);
            }
            plan => panic!("unexpected plan: {plan:#?}"),
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
            ProjectPlan::None(PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _)) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(
                    probe,
                    IndexProbe::Point(product(vec![
                        AlgebraicValue::U8(3),
                        AlgebraicValue::U8(4),
                        AlgebraicValue::U8(5),
                    ]))
                );
            }
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        // Test permutations of the same query
        let sql = "select * from t where z = 5 and y = 4 and x = 3";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _)) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(
                    probe,
                    IndexProbe::Point(product(vec![
                        AlgebraicValue::U8(3),
                        AlgebraicValue::U8(4),
                        AlgebraicValue::U8(5),
                    ]))
                );
            }
            proj => panic!("unexpected plan: {proj:#?}"),
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
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        match plan {
            PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(probe, IndexProbe::Point(value(AlgebraicValue::U8(3))));
            }
            plan => panic!("unexpected plan: {plan:#?}"),
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
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        match plan {
            PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(probe, IndexProbe::Point(value(AlgebraicValue::U8(4))));
            }
            plan => panic!("unexpected plan: {plan:#?}"),
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
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        // Select index on [y, z]
        let sql = "select * from t where y = 1 and z = 2";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _)) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(
                    probe,
                    IndexProbe::Point(product(vec![AlgebraicValue::U8(1), AlgebraicValue::U8(2)]))
                );
            }
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        // Check permutations of the same query
        let sql = "select * from t where z = 2 and y = 1";
        let lp = parse_and_type_sub(sql, &db).unwrap();
        let pp = compile_select(lp).optimize().unwrap();

        match pp {
            ProjectPlan::None(PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _)) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(
                    probe,
                    IndexProbe::Point(product(vec![AlgebraicValue::U8(1), AlgebraicValue::U8(2)]))
                );
            }
            proj => panic!("unexpected plan: {proj:#?}"),
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
            proj => panic!("unexpected plan: {proj:#?}"),
        };

        match plan {
            PhysicalPlan::IxScan(IxScan { schema, probe, .. }, _) => {
                assert_eq!(schema.table_id, t_id);
                assert_eq!(
                    probe,
                    IndexProbe::Point(product(vec![AlgebraicValue::U8(2), AlgebraicValue::U8(3)]))
                );
            }
            plan => panic!("unexpected plan: {plan:#?}"),
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

    /// A parameterized view call's bound `arg_hash` scopes the optimized scan to that call,
    /// and collecting from the unoptimized plan returns the call's args.
    #[test]
    fn view_call_arg_hash_scopes_scan() {
        let caller = Identity::ONE;
        let args = ProductValue::from_iter([AlgebraicValue::U32(42)]);

        for is_anonymous in [true, false] {
            // A view's backing table: the hidden `arg_hash` column, indexed, then its row columns.
            let mut v = TableSchema::clone(
                &schema(
                    TableId(1),
                    "v",
                    &[("arg_hash", AlgebraicType::U256), ("x", AlgebraicType::U8)],
                    &[&[0]],
                    &[],
                    None,
                )
                .inner(),
            );
            v.view_info = Some(ViewDefInfo {
                view_id: ViewId(1),
                is_anonymous,
                params: ProductType::from_iter([("id", AlgebraicType::U32)]),
            });
            let db = SchemaViewer {
                schemas: vec![Arc::new(TableOrViewSchema::from(Arc::new(v)))],
            };

            let mut lp = parse_and_type_sub("select * from v(42)", &db).unwrap();
            lp.bind_view_arg_hashes(caller);
            let pp = compile_select(lp);

            let mut calls = HashSet::default();
            pp.collect_views(&mut calls);
            let expected_calls: HashSet<_> = [ViewCall {
                view_id: ViewId(1),
                args: args.clone(),
            }]
            .into_iter()
            .collect();
            assert_eq!(calls, expected_calls);

            let expected_hash = view_instance_arg_hash_value((!is_anonymous).then_some(caller), &args);
            match pp.optimize().unwrap() {
                ProjectPlan::None(PhysicalPlan::IxScan(IxScan { probe, .. }, _)) => {
                    assert_eq!(probe, IndexProbe::Point(value(expected_hash)));
                }
                plan => panic!("unexpected plan: {plan:#?}"),
            }
        }
    }

    /// Compilation scopes a view scan to the caller's rows, before any optimization
    #[test]
    fn compiled_view_scans_are_scoped() {
        for (is_anonymous, param) in [(true, PARAM_VIEW_ARG_HASH_EMPTY), (false, PARAM_VIEW_ARG_HASH_SENDER)] {
            let mut v = TableSchema::clone(
                &schema(
                    TableId(1),
                    "v",
                    &[("arg_hash", AlgebraicType::U256), ("x", AlgebraicType::U8)],
                    &[&[0]],
                    &[],
                    None,
                )
                .inner(),
            );
            v.view_info = Some(ViewDefInfo {
                view_id: ViewId(1),
                is_anonymous,
                params: ProductType::unit(),
            });
            let db = SchemaViewer {
                schemas: vec![Arc::new(TableOrViewSchema::from(Arc::new(v)))],
            };

            let plan = compile_select(parse_and_type_sub("select * from v", &db).unwrap());
            let ProjectPlan::None(PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs))) = plan else {
                panic!("expected a filter over the view scan: {plan:#?}");
            };
            let PhysicalPlan::TableScan(_, label) = *input else {
                panic!("expected a view scan: {input:#?}");
            };
            assert_eq!(*lhs, PhysicalExpr::Param(param, AlgebraicType::U256));
            let PhysicalExpr::Field(field) = *rhs else {
                panic!("expected the arg hash column: {rhs:#?}");
            };
            assert_eq!(field.label, label);
            assert_eq!(field.field_pos, VIEW_ARG_HASH_COL.idx());
        }
    }

    const PLAYER_ID: TableId = TableId(1);
    const TEAM_ID: TableId = TableId(2);

    /// The binding relation's label, distinct from the query's labels
    const BINDINGS: Label = Label(100);

    /// player(id, team_id, rank, score) and team(id, region)
    fn player_team_db() -> SchemaViewer {
        let player = schema(
            PLAYER_ID,
            "player",
            &[
                ("id", AlgebraicType::U64),
                ("team_id", AlgebraicType::U64),
                ("rank", AlgebraicType::U32),
                ("score", AlgebraicType::U32),
            ],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        );
        let team = schema(
            TEAM_ID,
            "team",
            &[("id", AlgebraicType::U64), ("region", AlgebraicType::String)],
            &[&[0], &[1]],
            &[&[0]],
            Some(0),
        );
        SchemaViewer {
            schemas: vec![Arc::new(player), Arc::new(team)],
        }
    }

    /// The label of the scan over `table_id`
    fn label_for(plan: &ProjectPlan, table_id: TableId) -> Label {
        let mut label = None;
        plan.visit(&mut |plan| {
            if let PhysicalPlan::TableScan(scan, var) = plan
                && scan.schema.table_id == table_id
            {
                label = Some(*var);
            }
        });
        label.expect("the plan does not scan this table")
    }

    /// Column `field_pos` of `label`, positioned later by `optimize`
    fn field(label: Label, field_pos: usize) -> TupleField {
        TupleField {
            label,
            label_pos: None,
            field_pos,
        }
    }

    /// `lhs = rhs` for two fields
    fn fields_eq(lhs: TupleField, rhs: TupleField) -> PhysicalExpr {
        PhysicalExpr::BinOp(
            BinOp::Eq,
            Box::new(PhysicalExpr::Field(lhs)),
            Box::new(PhysicalExpr::Field(rhs)),
        )
    }

    /// A probe of binding index 0 with these input fields
    fn probe(key: Vec<TupleField>) -> BindingCandidates {
        BindingCandidates::Probe(BindingProbe {
            index: BindingIndexId(0),
            key,
        })
    }

    /// Put a binding lookup, and optionally a filter, on top of an unoptimized plan
    fn with_binding_lookup(
        plan: ProjectPlan,
        returns: Label,
        candidates: BindingCandidates,
        filter: Option<PhysicalExpr>,
    ) -> ProjectPlan {
        let (ProjectPlan::None(plan) | ProjectPlan::Name(plan, ..)) = plan;
        let lookup = BindingLookup {
            label: BINDINGS,
            candidates,
        };
        let plan = PhysicalPlan::BindingLookup(Box::new(plan), lookup);
        let plan = match filter {
            Some(expr) => PhysicalPlan::Filter(Box::new(plan), expr),
            None => plan,
        };
        ProjectPlan::Name(plan, returns, None)
    }

    /// Optimize the base plan and every delta fragment, as `Fragments::compile_from_plan` builds them
    fn optimize_base_and_fragments(plan: &ProjectPlan, labels: &[Label]) -> Vec<ProjectPlan> {
        let mut combos: Vec<Vec<(Label, Delta)>> = vec![vec![]];
        for label in labels {
            combos = combos
                .into_iter()
                .flat_map(|combo| {
                    [None, Some(Delta::Inserts), Some(Delta::Deletes)].map(|delta| {
                        let mut combo = combo.clone();
                        combo.extend(delta.map(|delta| (*label, delta)));
                        combo
                    })
                })
                .collect();
        }
        combos
            .into_iter()
            .map(|combo| {
                let mut plan = plan.clone();
                for (label, delta) in combo {
                    plan.visit_mut(&mut |plan| {
                        if let PhysicalPlan::TableScan(scan @ TableScan { delta: None, .. }, var) = plan
                            && *var == label
                        {
                            scan.delta = Some(delta);
                        }
                    });
                }
                plan.optimize().unwrap()
            })
            .collect()
    }

    /// The only binding lookup in an optimized plan
    fn find_lookup(plan: &PhysicalPlan) -> (PhysicalPlan, BindingLookup) {
        let mut found = vec![];
        plan.visit(&mut |plan| {
            if let PhysicalPlan::BindingLookup(_, lookup) = plan {
                found.push((plan.clone(), lookup.clone()));
            }
        });
        assert_eq!(found.len(), 1, "expected exactly one binding lookup: {plan:#?}");
        found.swap_remove(0)
    }

    /// Checks shared by every optimized lookup plan
    fn assert_lookup(plan: &ProjectPlan, return_table_id: TableId) {
        let ProjectPlan::Name(root, _, Some(_)) = plan else {
            panic!("expected a projection of one row from each tuple: {plan:#?}");
        };
        let (node, lookup) = find_lookup(root);
        assert!(!node.any(&|plan| matches!(plan, PhysicalPlan::IxScan(..))));
        for field in lookup.key_fields() {
            assert!(field.label_pos.is_some(), "unresolved key field: {field:?}");
        }
        assert!(node.has_label(&BINDINGS));
        assert!(
            root.position(&BINDINGS).is_some(),
            "the binding row must reach the root: {plan:#?}"
        );
        assert_eq!(node.position(&BINDINGS), Some(node.nfields() - 1));
        assert_eq!(plan.return_table().map(|schema| schema.table_id), Some(return_table_id));
    }

    /// The filter on binding fields must stay directly above the lookup
    fn assert_filter_above_lookup(plan: &ProjectPlan) {
        let PhysicalPlan::Filter(input, expr) = plan.physical_plan() else {
            panic!("expected a filter at the root: {plan:#?}");
        };
        assert!(matches!(**input, PhysicalPlan::BindingLookup(..)));
        let binding_pos = input.position(&BINDINGS);
        assert!(binding_pos.is_some());
        let mut binding_fields = 0;
        expr.visit(&mut |expr| {
            if let PhysicalExpr::Field(field) = expr
                && field.label == BINDINGS
            {
                assert_eq!(field.label_pos, binding_pos);
                binding_fields += 1;
            }
        });
        assert!(binding_fields > 0);
    }

    /// A key on an indexed column must not become an index scan:
    /// `where id = ?`
    #[test]
    fn binding_lookup_single_table() {
        let db = player_team_db();
        let plan = compile_select(parse_and_type_sub("select * from player", &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let plan = with_binding_lookup(plan, player, probe(vec![field(player, 0)]), None);
        let plans = optimize_base_and_fragments(&plan, &[player]);
        assert_eq!(plans.len(), 3);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert_eq!(plan.binding_position(), Some(1));
            match plan.physical_plan() {
                PhysicalPlan::BindingLookup(input, _) => {
                    assert!(matches!(**input, PhysicalPlan::TableScan(..)));
                }
                plan => panic!("unexpected plan: {plan:#?}"),
            }
        }
    }

    /// The key reads the team table, so a semijoin must not drop it:
    /// `where t.region = ? and p.rank = ?`
    #[test]
    fn binding_lookup_join_keys_on_both_sides() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let key = vec![field(team, 1), field(player, 2)];
        let plan = with_binding_lookup(plan, player, probe(key), None);
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert!(plan.physical_plan().position(&team).is_some());
        }
    }

    /// A fixed predicate is still pushed below the lookup:
    /// `where p.score >= 100 and t.region = ? and p.rank = ?`
    #[test]
    fn binding_lookup_join_with_fixed_filter() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id where p.score >= 100";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let key = vec![field(team, 1), field(player, 2)];
        let plan = with_binding_lookup(plan, player, probe(key), None);
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert!(plan.physical_plan().position(&team).is_some());
            match plan.physical_plan() {
                PhysicalPlan::BindingLookup(input, _) => {
                    assert!(input.has_filter());
                }
                plan => panic!("unexpected plan: {plan:#?}"),
            }
        }
    }

    /// Without a probe, every binding is checked by a filter above the lookup:
    /// `where t.region = ? or p.rank = ?`
    #[test]
    fn binding_lookup_join_or_filter() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let filter = PhysicalExpr::LogOp(
            LogOp::Or,
            vec![
                fields_eq(field(team, 1), field(BINDINGS, 0)),
                fields_eq(field(player, 2), field(BINDINGS, 1)),
            ],
        );
        let plan = with_binding_lookup(plan, player, BindingCandidates::All, Some(filter));
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert_filter_above_lookup(plan);
            assert!(plan.physical_plan().position(&team).is_some());
        }
    }

    /// A key narrows the candidates, and a filter above checks the rest:
    /// `where t.region = ? and (p.rank = ? or p.rank = ?)`
    #[test]
    fn binding_lookup_join_key_and_filter() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let filter = PhysicalExpr::LogOp(
            LogOp::Or,
            vec![
                fields_eq(field(player, 2), field(BINDINGS, 1)),
                fields_eq(field(player, 2), field(BINDINGS, 2)),
            ],
        );
        let plan = with_binding_lookup(plan, player, probe(vec![field(team, 1)]), Some(filter));
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert_filter_above_lookup(plan);
            assert!(plan.physical_plan().position(&team).is_some());
        }
    }

    /// A key on the returned table still lets a semijoin drop the other table:
    /// `where p.rank = ?`
    #[test]
    fn binding_lookup_join_key_on_returned_table() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let plan = with_binding_lookup(plan, player, probe(vec![field(player, 2)]), None);
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert_eq!(plan.physical_plan().position(&team), None);
        }
    }

    /// A whole row projection of a lookup plan is rejected
    #[test]
    fn binding_lookup_needs_a_named_projection() {
        let db = player_team_db();
        let plan = compile_select(parse_and_type_sub("select * from player", &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let ProjectPlan::None(scan) = plan else {
            panic!("unexpected plan: {plan:#?}");
        };
        let lookup = BindingLookup {
            label: BINDINGS,
            candidates: probe(vec![field(player, 0)]),
        };
        let plan = ProjectPlan::None(PhysicalPlan::BindingLookup(Box::new(scan), lookup));
        let err = plan.optimize().unwrap_err();
        assert!(
            err.to_string().contains("Cannot return whole rows"),
            "unexpected error: {err}"
        );
    }

    /// An unresolvable key field is rejected
    #[test]
    fn binding_lookup_unresolvable_key_is_rejected() {
        let db = player_team_db();
        let plan = compile_select(parse_and_type_sub("select * from player", &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let missing = Label(99);
        let plan = with_binding_lookup(plan, player, probe(vec![field(missing, 0)]), None);
        let err = plan.optimize().unwrap_err();
        assert!(
            err.to_string().contains("Could not compute positional arguments"),
            "unexpected error: {err}"
        );
    }

    /// A lookup below a join keeps its binding row:
    /// `where t.region = ?`
    #[test]
    fn binding_lookup_below_join_keeps_binding_row() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id";
        let plan = compile_select(parse_and_type_sub(sql, &db).unwrap());
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        let ProjectPlan::Name(PhysicalPlan::HashJoin(mut join, semi), returns, pos) = plan else {
            panic!("unexpected plan: {plan:#?}");
        };
        let PhysicalPlan::TableScan(_, label) = &*join.rhs else {
            panic!("expected the team scan on the rhs: {join:#?}");
        };
        assert_eq!(*label, team);
        let lookup = BindingLookup {
            label: BINDINGS,
            candidates: probe(vec![field(team, 1)]),
        };
        join.rhs = Box::new(PhysicalPlan::BindingLookup(join.rhs, lookup));
        let plan = ProjectPlan::Name(PhysicalPlan::HashJoin(join, semi), returns, pos);
        let plans = optimize_base_and_fragments(&plan, &[player, team]);
        assert_eq!(plans.len(), 9);
        for plan in &plans {
            assert_lookup(plan, PLAYER_ID);
            assert!(!matches!(plan.physical_plan(), PhysicalPlan::BindingLookup(..)));
            assert_eq!(plan.binding_position(), plan.physical_plan().position(&BINDINGS));
        }
    }

    /// The plan of a template before optimization, in the form templates are compared in
    fn template_plan(sql: &str, db: &SchemaViewer) -> (ProjectPlan, Vec<(ParamSource, AlgebraicType)>) {
        let mut plan = parse_and_type_sub(sql, db).unwrap();
        // View calls' hashes are lifted too, so they're bound first
        plan.bind_view_arg_hashes(Identity::ONE);
        let literals = plan.lift_literals();
        let mut params: Vec<_> = literals
            .into_iter()
            .map(|(value, ty)| (ParamSource::Literal(value), ty))
            .collect();
        let mut plan = compile_select(plan);
        plan.remap_builtin_params(&mut params);
        (plan.canonicalize(), params)
    }

    /// The base plan and fragments of `sql` as a template: literals lifted, built-ins remapped.
    /// Also returns the source of each binding param.
    fn template_plans(sql: &str, db: &SchemaViewer) -> (Vec<ProjectPlan>, Vec<(ParamSource, AlgebraicType)>) {
        let (plan, params) = template_plan(sql, db);
        let mut labels = vec![];
        plan.visit(&mut |plan| {
            if let PhysicalPlan::TableScan(_, label) = plan {
                labels.push(*label);
            }
        });
        (optimize_base_and_fragments(&plan, &labels), params)
    }

    /// The binding indexes of `plan`'s params, sorted, without repeats
    fn binding_indexes(plan: &mut ProjectPlan) -> Vec<u16> {
        let mut indexes = vec![];
        plan.visit_exprs_mut(&mut |expr| {
            if let PhysicalExpr::Param(slot, _) = expr {
                indexes.push(slot.binding_index().expect("a template has only binding params"));
            }
        });
        indexes.sort();
        indexes.dedup();
        indexes
    }

    /// account(id: identity, n: u32, owner: identity)
    fn account_db() -> SchemaViewer {
        let account = schema(
            TableId(1),
            "account",
            &[
                ("id", AlgebraicType::identity()),
                ("n", AlgebraicType::U32),
                ("owner", AlgebraicType::identity()),
            ],
            &[&[0]],
            &[&[0]],
            Some(0),
        );
        SchemaViewer {
            schemas: vec![Arc::new(account)],
        }
    }

    #[test]
    fn template_plans_ignore_predicate_order() {
        let db = player_team_db();
        let join = "select p.* from player p join team t on p.team_id = t.id";
        let (q3, _) = template_plans(&format!("{join} where t.region = 'west' and p.rank = 3"), &db);
        let (q5, _) = template_plans(&format!("{join} where p.rank = 3 and t.region = 'west'"), &db);
        let (q6, _) = template_plans(&format!("{join} where 3 = p.rank and 'west' = t.region"), &db);
        assert_eq!(q3.len(), 9);
        assert_eq!(q3, q5);
        assert_eq!(q3, q6);

        // Swapping `FROM` gives another template
        let q7 = "select p.* from team t join player p on p.team_id = t.id where t.region = 'west' and p.rank = 3";
        assert_ne!(q3, template_plans(q7, &db).0);

        // Reordering the branches of `OR` changes only the values, when the branches have the same shape
        let or = "(t.region = 'west' and p.rank = 1) or (t.region = 'east' and p.rank = 2)";
        let (q8, q8_params) = template_plans(&format!("{join} where {or}"), &db);
        let or = "(p.rank = 2 and t.region = 'east') or (p.rank = 1 and t.region = 'west')";
        let (q9, q9_params) = template_plans(&format!("{join} where {or}"), &db);
        assert_ne!(q3, q8);
        assert_eq!(q8, q9);
        assert_ne!(q8_params, q9_params);
    }

    #[test]
    fn canonical_template_ignores_predicate_order() {
        let db = player_team_db();
        let template = |filter: &str| template_plan(&format!("select * from player where {filter}"), &db).0;

        // `rank` and `score` have no index, so `optimize` alone would keep these in their order
        let expected = template("rank = 1 and score = 2 and id > 3");
        for filter in [
            "score = 4 and rank = 5 and id > 3",
            "3 < id and 4 = score and rank = 5",
            "rank = 5 and (id > 3 and score = 4)",
        ] {
            assert_eq!(template(filter), expected, "{filter}");
        }

        // The rest keep their order
        assert_ne!(
            template("rank = 1 and id > 3 and score > 4"),
            template("rank = 1 and score > 4 and id > 3"),
        );

        // Inside `OR` too, but not across it
        let or = template("(rank = 1 and score = 2) or id = 3");
        assert_eq!(or, template("(2 = score and rank = 1) or id = 3"));
        assert_ne!(or, template("id = 3 or (rank = 1 and score = 2)"));
    }

    #[test]
    fn remap_adds_no_builtins_for_literals() {
        let (plans, params) = template_plans("select * from player where id = 3", &player_team_db());
        let literal = (ParamSource::Literal(AlgebraicValue::U64(3)), AlgebraicType::U64);
        assert_eq!(params, vec![literal]);
        for mut plan in plans {
            assert_eq!(binding_indexes(&mut plan), vec![0]);
        }
    }

    #[test]
    fn remap_moves_sender_after_literals() {
        let (plans, params) = template_plans("select * from account where id = :sender and n = 3", &account_db());
        let literal = (ParamSource::Literal(AlgebraicValue::U32(3)), AlgebraicType::U32);
        let sender = (ParamSource::Builtin(PARAM_SENDER), AlgebraicType::identity());
        assert_eq!(params, vec![literal, sender]);
        for mut plan in plans {
            assert_eq!(binding_indexes(&mut plan), vec![0, 1]);
        }
    }

    /// v(arg_hash: u256, x: u8), a sender-scoped view with `params`, with its hidden `arg_hash` column
    fn sender_view_db(params: ProductType) -> SchemaViewer {
        let mut v = TableSchema::clone(
            &schema(
                TableId(1),
                "v",
                &[("arg_hash", AlgebraicType::U256), ("x", AlgebraicType::U8)],
                &[&[0]],
                &[],
                None,
            )
            .inner(),
        );
        v.view_info = Some(ViewDefInfo {
            view_id: ViewId(1),
            is_anonymous: false,
            params,
        });
        SchemaViewer {
            schemas: vec![Arc::new(TableOrViewSchema::from(Arc::new(v)))],
        }
    }

    #[test]
    fn remap_moves_view_arg_hashes() {
        let (plans, params) = template_plans("select * from v", &sender_view_db(ProductType::unit()));
        let hash = (ParamSource::Builtin(PARAM_VIEW_ARG_HASH_SENDER), AlgebraicType::U256);
        assert_eq!(params, vec![hash]);
        for mut plan in plans {
            assert_eq!(binding_indexes(&mut plan), vec![0]);
        }
    }

    /// `sql`'s template plan with its binding lookup inserted, the binding indexes, and the lookup
    fn lookup_plan(sql: &str, db: &SchemaViewer) -> (ProjectPlan, Vec<ColList>, BindingLookup) {
        let (plan, _) = template_plan(sql, db);
        let mut indexes = vec![];
        let plan = plan.insert_binding_lookup(&mut indexes);
        let (_, lookup) = find_lookup(plan.physical_plan());
        (plan, indexes, lookup)
    }

    /// Every fragment of a plan with a lookup reads no param, optimizes, and returns its binding row
    fn assert_lookup_fragments(plan: &ProjectPlan, lookup: &BindingLookup) {
        let mut plan = plan.clone();
        assert_eq!(binding_indexes(&mut plan), Vec::<u16>::new());
        let mut labels = vec![];
        plan.visit(&mut |plan| {
            if let PhysicalPlan::TableScan(_, label) = plan {
                labels.push(*label);
            }
        });
        assert!(!labels.contains(&lookup.label));
        for plan in optimize_base_and_fragments(&plan, &labels) {
            assert!(plan.binding_position().is_some(), "no binding row: {plan:#?}");
            assert_eq!(plan.binding_position(), plan.physical_plan().position(&lookup.label));
        }
    }

    /// The lookup probes every `field = binding`, and the other predicates stay below it
    #[test]
    fn binding_lookup_probes_binding_equalities() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id \
                   where t.region = 'west' and p.rank = 3 and p.score >= 100";
        let (plan, indexes, lookup) = lookup_plan(sql, &db);
        let player = label_for(&plan, PLAYER_ID);
        let team = label_for(&plan, TEAM_ID);
        // Slots follow the `FROM` order: `p.rank` is binding 0, `t.region` binding 1
        assert_eq!(lookup.candidates, probe(vec![field(player, 2), field(team, 1)]));
        assert_eq!(indexes, vec![ColList::from_iter([1, 2])]);
        let PhysicalPlan::BindingLookup(input, _) = plan.physical_plan() else {
            panic!("expected the lookup at the root: {plan:#?}");
        };
        assert!(
            matches!(**input, PhysicalPlan::Filter(..)),
            "the range stays below: {input:#?}"
        );
        assert_lookup_fragments(&plan, &lookup);
    }

    /// Predicates that read a binding but don't probe it read the binding row above the lookup
    #[test]
    fn binding_lookup_filters_other_binding_predicates_above() {
        let (plan, indexes, lookup) =
            lookup_plan("select * from player where rank = 3 or score = 5", &player_team_db());
        assert_eq!(lookup.candidates, BindingCandidates::All);
        assert!(indexes.is_empty());
        // The lookup adds a field to each tuple, so the returned rows are named
        let ProjectPlan::Name(PhysicalPlan::Filter(input, expr), ..) = &plan else {
            panic!("expected a filter at the root: {plan:#?}");
        };
        assert!(matches!(**input, PhysicalPlan::BindingLookup(..)));
        let player = label_for(&plan, PLAYER_ID);
        let expected = PhysicalExpr::LogOp(
            LogOp::Or,
            vec![
                PhysicalExpr::BinOp(
                    BinOp::Eq,
                    Box::new(PhysicalExpr::Field(field(player, 2))),
                    Box::new(PhysicalExpr::Field(field(lookup.label, 1))),
                ),
                PhysicalExpr::BinOp(
                    BinOp::Eq,
                    Box::new(PhysicalExpr::Field(field(player, 3))),
                    Box::new(PhysicalExpr::Field(field(lookup.label, 2))),
                ),
            ],
        );
        assert_eq!(*expr, expected);
        assert_lookup_fragments(&plan, &lookup);
    }

    /// A query without params has one empty binding, which pairs with every row
    #[test]
    fn binding_lookup_without_params_pairs_every_row() {
        let (plan, indexes, lookup) = lookup_plan("select * from player", &player_team_db());
        assert_eq!(lookup.candidates, BindingCandidates::All);
        assert!(indexes.is_empty());
        assert!(matches!(plan, ProjectPlan::Name(PhysicalPlan::BindingLookup(..), ..)));
        assert_lookup_fragments(&plan, &lookup);
    }

    /// Binding predicates in nested filters, such as a view's arg hash below a `WHERE`, move to the lookup
    #[test]
    fn binding_lookup_lifts_nested_binding_predicates() {
        let db = sender_view_db(ProductType::unit());
        let (plan, indexes, lookup) = lookup_plan("select * from v where x = 1", &db);
        let v = label_for(&plan, TableId(1));
        // `x` is binding 0 and the hash binding 1
        assert_eq!(lookup.candidates, probe(vec![field(v, 1), field(v, 0)]));
        assert_eq!(indexes, vec![ColList::from_iter([1, 2])]);
        assert!(matches!(
            plan,
            ProjectPlan::Name(PhysicalPlan::BindingLookup(ref input, _), ..) if matches!(**input, PhysicalPlan::TableScan(..))
        ));
        assert_lookup_fragments(&plan, &lookup);
    }

    /// A predicate repeated in another filter, as RLS can repeat one, is probed once
    #[test]
    fn binding_lookup_lifts_a_repeated_predicate_once() {
        let (plan, _) = template_plan("select * from player where rank = 3", &player_team_db());
        let ProjectPlan::None(PhysicalPlan::Filter(_, expr)) = &plan else {
            panic!("expected a filter: {plan:#?}");
        };
        let expr = expr.clone();
        let (ProjectPlan::None(inner) | ProjectPlan::Name(inner, ..)) = plan;
        let plan = ProjectPlan::None(PhysicalPlan::Filter(Box::new(inner), expr));

        let plan = plan.insert_binding_lookup(&mut vec![]);
        let player = label_for(&plan, PLAYER_ID);
        let (_, lookup) = find_lookup(plan.physical_plan());
        assert_eq!(lookup.candidates, probe(vec![field(player, 2)]));
        assert!(matches!(plan, ProjectPlan::Name(PhysicalPlan::BindingLookup(..), ..)));
    }

    /// A view call's lifted arg hash is probed
    #[test]
    fn binding_lookup_probes_a_view_calls_arg_hash() {
        let db = sender_view_db(ProductType::from_iter([("id", AlgebraicType::U32)]));
        let (plan, indexes, lookup) = lookup_plan("select * from v(3)", &db);
        let v = label_for(&plan, TableId(1));
        assert_eq!(lookup.candidates, probe(vec![field(v, 0)]));
        assert_eq!(indexes, vec![ColList::from_iter([1])]);
        assert_lookup_fragments(&plan, &lookup);
    }

    /// A binding compared twice, such as `:sender` on two columns, is probed once
    /// and compared against the binding row the second time
    #[test]
    fn binding_lookup_probes_a_repeated_binding_once() {
        let sql = "select * from account where id = :sender and owner = :sender";
        let (plan, indexes, lookup) = lookup_plan(sql, &account_db());
        let account = label_for(&plan, TableId(1));
        assert_eq!(lookup.candidates, probe(vec![field(account, 0)]));
        assert_eq!(indexes, vec![ColList::from_iter([1])]);
        let ProjectPlan::Name(PhysicalPlan::Filter(_, expr), ..) = &plan else {
            panic!("expected a filter above the lookup: {plan:#?}");
        };
        let expected = PhysicalExpr::BinOp(
            BinOp::Eq,
            Box::new(PhysicalExpr::Field(field(account, 2))),
            Box::new(PhysicalExpr::Field(field(lookup.label, 1))),
        );
        assert_eq!(*expr, expected);
        assert_lookup_fragments(&plan, &lookup);
    }

    /// Fragments that probe the same columns share an index, and others add one
    #[test]
    fn binding_lookup_shares_a_templates_indexes() {
        let db = player_team_db();
        let (rank, _) = template_plan("select * from player where rank = 3", &db);
        let (both, _) = template_plan("select * from player where rank = 3 and score = 4", &db);
        let mut indexes = vec![];
        let index_of = |plan: ProjectPlan, indexes: &mut Vec<ColList>| {
            let plan = plan.insert_binding_lookup(indexes);
            match find_lookup(plan.physical_plan()).1.candidates {
                BindingCandidates::Probe(probe) => probe.index,
                BindingCandidates::All => panic!("expected a probe: {plan:#?}"),
            }
        };
        assert_eq!(index_of(rank.clone(), &mut indexes), BindingIndexId(0));
        assert_eq!(index_of(both, &mut indexes), BindingIndexId(1));
        assert_eq!(index_of(rank, &mut indexes), BindingIndexId(0));
        assert_eq!(indexes, vec![ColList::from_iter([1]), ColList::from_iter([1, 2])]);
    }

    /// When the changed table alone gives the key, the lookup follows its delta scan,
    /// so its rows that no binding matches never reach the join
    #[test]
    fn binding_lookup_follows_a_delta_scan_with_the_key() {
        let db = player_team_db();
        let sql = "select p.* from player p join team t on p.team_id = t.id where t.region = 'west'";
        let (plan, _) = template_plan(sql, &db);
        let team = label_for(&plan, TEAM_ID);
        let player = label_for(&plan, PLAYER_ID);
        let changed = |label: Label| {
            let mut plan = plan.clone();
            plan.visit_mut(&mut |plan| {
                if let PhysicalPlan::TableScan(scan, var) = plan
                    && *var == label
                {
                    scan.delta = Some(Delta::Inserts);
                }
            });
            plan.insert_binding_lookup(&mut vec![])
        };
        let scan_below_lookup = |plan: &PhysicalPlan| {
            let (PhysicalPlan::BindingLookup(input, _), _) = find_lookup(plan) else {
                unreachable!("`find_lookup` returns a lookup");
            };
            matches!(*input, PhysicalPlan::TableScan(_, var) if var == team)
        };

        // A team changed: its region is the key, so look up before fetching its players
        let team_changed = changed(team);
        assert!(scan_below_lookup(team_changed.physical_plan()), "{team_changed:#?}");
        let optimized = team_changed.clone().optimize().unwrap();
        assert!(scan_below_lookup(optimized.physical_plan()), "{optimized:#?}");

        // A player changed: the join finds the key
        let player_changed = changed(player);
        assert!(matches!(
            player_changed.physical_plan(),
            PhysicalPlan::BindingLookup(..)
        ));

        for plan in [team_changed, player_changed] {
            let (_, lookup) = find_lookup(plan.physical_plan());
            let plan = plan.optimize().unwrap();
            assert_eq!(plan.binding_position(), plan.physical_plan().position(&lookup.label));
            assert!(plan.binding_position().is_some());
        }
    }

    /// Every delta fragment of a join with indexes on its join columns joins by index, wherever its lookup goes
    #[test]
    fn binding_lookup_keeps_delta_joins_on_indexes() {
        let db = player_team_db();
        let join = "select p.* from player p join team t on p.team_id = t.id";
        for filter in [
            "t.region = 'west'",
            // A fixed filter on the other side used to stop the delta from driving the join
            "t.region = 'west' and p.score >= 100",
            "t.id = 3",
            "p.rank = 2",
            "t.region = 'west' and p.rank = 2",
            "t.region = 'west' or p.rank = 2",
        ] {
            let sql = format!("{join} where {filter}");
            let (plan, _) = template_plan(&sql, &db);
            let labels = [label_for(&plan, PLAYER_ID), label_for(&plan, TEAM_ID)];
            for deltas in [
                vec![(labels[0], Delta::Inserts)],
                vec![(labels[1], Delta::Inserts)],
                vec![(labels[0], Delta::Inserts), (labels[1], Delta::Deletes)],
            ] {
                let mut plan = plan.clone();
                plan.visit_mut(&mut |plan| {
                    if let PhysicalPlan::TableScan(scan, var) = plan
                        && let Some((_, delta)) = deltas.iter().find(|(label, _)| label == var)
                    {
                        scan.delta = Some(*delta);
                    }
                });
                let plan = plan.insert_binding_lookup(&mut vec![]).optimize().unwrap();
                assert!(
                    !plan.any(&|plan| matches!(plan, PhysicalPlan::HashJoin(..) | PhysicalPlan::NLJoin(..))),
                    "`{sql}` with deltas {deltas:?}: {plan:#?}"
                );
            }
        }
    }

    /// Templates that compare equal hash equally, and ranges and shapes still tell templates apart
    #[test]
    fn equal_templates_hash_equally() {
        let db = player_team_db();
        let hash = |sql: &str| {
            let (plan, _) = template_plan(sql, &db);
            let mut state = std::collections::hash_map::DefaultHasher::new();
            plan.hash_structure(&mut state);
            std::hash::Hasher::finish(&state)
        };
        let base = hash("select * from player where rank = 1 and score > 2");
        assert_eq!(base, hash("select * from player where score > 2 and 5 = rank"));
        assert_ne!(base, hash("select * from player where rank = 1 and score > 3"));
        assert_ne!(base, hash("select * from player where rank = 1 and score < 2"));
        assert_ne!(base, hash("select * from player where rank = 1"));
        assert_ne!(
            hash("select * from player where (id > 1 and team_id > 2) or rank > 3 or score > 4"),
            hash("select * from player where (id > 1 and team_id > 2 and rank > 3) or score > 4")
        );
        let join = "select p.* from player p join team t on p.team_id = t.id";
        assert_eq!(
            hash(&format!("{join} where t.region = 'a'")),
            hash(&format!("{join} where 'b' = t.region"))
        );
        assert_ne!(
            hash(&format!("{join} where t.region = 'a'")),
            hash(&format!("{join} where p.rank = 1"))
        );
    }
}
