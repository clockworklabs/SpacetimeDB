//! This module defines the rewrite rules used for query optimization.
//!
//! These include:
//!
//! * [PushConstEq]
//!   Push down predicates of the form `x=1`
//! * [PushConstAnd]
//!   Push down predicates of the form `x=1 and y=2`
//! * [IxScanFromPredicates]
//!   Generate point index scans from equality predicates
//! * [ReorderHashJoin]
//!   Reorder the sides of a hash join
//! * [ReorderDeltaJoinRhs]
//!   Reorder the sides of a hash join with delta tables
//! * [HashToIxJoin]
//!   Convert hash join to index join
//! * [UniqueIxJoinRule]
//!   Mark index join as unique
//! * [UniqueHashJoinRule]
//!   Mark hash join as unique
use anyhow::{bail, Result};
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

use crate::plan::{
    index_key_expr, HashJoin, IndexProbe, IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, ProjectListPlan,
    ProjectPlan, Semi, TableScan, TupleField,
};

/// A rewrite will only fail due to an internal logic bug.
/// However we don't want to panic in such a situation.
/// Instead we leave it to the caller to determine how to proceed.
const INVARIANT_VIOLATION: &str = "Invariant violation during query planning";

pub trait RewriteRule {
    type Plan;
    type Info;

    fn matches(plan: &Self::Plan) -> Option<Self::Info>;
    fn rewrite(plan: Self::Plan, info: Self::Info) -> Result<Self::Plan>;
}

/// To preserve semantics while reordering operations,
/// the physical plan assumes tuples with named labels.
/// However positions are computed for them before execution.
/// Note, this should always be the last rewrite.
pub(crate) struct ComputePositions;

impl RewriteRule for ComputePositions {
    type Plan = PhysicalPlan;
    type Info = (Label, usize);

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::Filter(input, expr) => {
                let mut name_and_position = None;
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(TupleField {
                        label, label_pos: None, ..
                    }) = expr
                    {
                        name_and_position = input.position(label).map(|i| (*label, i));
                    }
                });
                name_and_position
            }
            PhysicalPlan::IxJoin(IxJoin { lhs: input, probe, .. }, _) => {
                let mut name_and_position = None;
                probe.visit(&mut |expr| {
                    if let PhysicalExpr::Field(TupleField {
                        label, label_pos: None, ..
                    }) = expr
                    {
                        name_and_position = input.position(label).map(|i| (*label, i));
                    }
                });
                name_and_position
            }
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: input,
                    lhs_field: TupleField {
                        label, label_pos: None, ..
                    },
                    ..
                },
                _,
            )
            | PhysicalPlan::HashJoin(
                HashJoin {
                    rhs: input,
                    rhs_field: TupleField {
                        label, label_pos: None, ..
                    },
                    ..
                },
                _,
            ) => input.position(label).map(|i| (*label, i)),
            _ => None,
        }
    }

    fn rewrite(mut plan: Self::Plan, (name, pos): Self::Info) -> Result<Self::Plan> {
        match &mut plan {
            PhysicalPlan::Filter(_, expr) => {
                expr.visit_mut(&mut |expr| match expr {
                    PhysicalExpr::Field(t @ TupleField { label_pos: None, .. }) if t.label == name => {
                        t.label_pos = Some(pos);
                    }
                    _ => {}
                });
            }
            PhysicalPlan::IxJoin(IxJoin { probe, .. }, _) => {
                probe.visit_mut(&mut |expr| match expr {
                    PhysicalExpr::Field(t @ TupleField { label_pos: None, .. }) if t.label == name => {
                        t.label_pos = Some(pos);
                    }
                    _ => {}
                });
            }
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            )
            | PhysicalPlan::HashJoin(
                HashJoin {
                    rhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            ) => {
                t.label_pos = Some(pos);
            }
            _ => {}
        }
        Ok(plan)
    }
}

/// Merge a limit with a table or index scan.
///
/// Note that for pull-based, tuple at a time iterators,
/// a limit is a short circuiting operator,
/// and therefore this optimization is essentially a no-op.
///
/// However for executors that materialize intermediate results,
/// this will avoid scanning the entire table.
pub(crate) struct PushLimit;

impl RewriteRule for PushLimit {
    type Plan = ProjectListPlan;
    type Info = ();

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        match plan {
            ProjectListPlan::Limit(scan, _) => {
                match &**scan {
                    ProjectListPlan::Name(plans) => plans.iter().any(|plan| {
                        matches!(
                            plan,
                            ProjectPlan::None(
                                PhysicalPlan::TableScan(TableScan { limit: None, .. }, _)
                                    | PhysicalPlan::IxScan(IxScan { limit: None, .. }, _)
                            )
                        )
                    }),
                    _ => false,
                }
            }
            .then_some(()),
            _ => None,
        }
    }

    fn rewrite(plan: Self::Plan, _: ()) -> Result<Self::Plan> {
        let select = |plan| ProjectPlan::None(plan);
        let limit_scan = |scan, n| match scan {
            PhysicalPlan::TableScan(scan, alias) => {
                select(PhysicalPlan::TableScan(
                    TableScan {
                        // Push limit into table scan
                        limit: Some(n),
                        ..scan
                    },
                    alias,
                ))
            }
            PhysicalPlan::IxScan(scan, alias) => select(PhysicalPlan::IxScan(
                IxScan {
                    // Push limit into index scan
                    limit: Some(n),
                    ..scan
                },
                alias,
            )),
            _ => select(scan),
        };
        match plan {
            ProjectListPlan::Limit(scan, n) => match *scan {
                ProjectListPlan::Name(plans) => Ok(ProjectListPlan::Name(
                    plans
                        .into_iter()
                        .map(|plan| match plan {
                            ProjectPlan::None(
                                scan @ PhysicalPlan::TableScan(TableScan { limit: None, .. }, _)
                                | scan @ PhysicalPlan::IxScan(IxScan { limit: None, .. }, _),
                            ) => limit_scan(scan, n),
                            _ => plan,
                        })
                        .collect(),
                )),
                input => Ok(ProjectListPlan::Limit(Box::new(input), n)),
            },
            _ => Ok(plan),
        }
    }
}

/// Push constant comparisons down to the leaves.
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
pub(crate) struct PushConstEq;

impl RewriteRule for PushConstEq {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        // Is this plan a table scan followed by a sequence of filters?
        // If so, it's already in a normalized state, so no need to push.
        let is_filter = |plan: &PhysicalPlan| {
            !plan.any(&|plan| !matches!(plan, PhysicalPlan::TableScan(..) | PhysicalPlan::Filter(..)))
        };
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(_, expr, value)) = plan
            && let (PhysicalExpr::Field(TupleField { label, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value)
        {
            return (input.has_table_scan(Some(label)) && !is_filter(input)).then_some(*label);
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, relvar: Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::Filter(input, expr) => input.map_if(
                |scan, _| Ok(PhysicalPlan::Filter(Box::new(scan), expr)),
                |plan| plan.is_table_scan(Some(&relvar)).then_some(()),
            ),
            _ => Ok(plan),
        }
    }
}

/// Push constant conjunctions down to the leaves.
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
pub(crate) struct PushConstAnd;

impl RewriteRule for PushConstAnd {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        // Is this plan a table scan followed by a sequence of filters?
        // If so, it's already in a normalized state, so no need to push.
        let is_filter = |plan: &PhysicalPlan| {
            !plan.any(&|plan| !matches!(plan, PhysicalPlan::TableScan(..) | PhysicalPlan::Filter(..)))
        };
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            return exprs.iter().find_map(|expr| {
                if let PhysicalExpr::BinOp(_, expr, value) = expr
                    && let (PhysicalExpr::Field(TupleField { label, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value)
                {
                    return (input.has_table_scan(Some(label)) && !is_filter(input)).then_some(*label);
                }
                None
            });
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, relvar: Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) => {
                let mut leaf_exprs = vec![];
                let mut root_exprs = vec![];
                for expr in exprs {
                    if let PhysicalExpr::BinOp(_, lhs, value) = &expr
                        && let (PhysicalExpr::Field(TupleField { label: var, .. }), PhysicalExpr::Value(_)) =
                            (&**lhs, &**value)
                        && var == &relvar
                    {
                        leaf_exprs.push(expr);
                        continue;
                    }
                    root_exprs.push(expr);
                }
                // Match on table or delta scan
                let ok = |plan: &PhysicalPlan| plan.is_table_scan(Some(&relvar)).then_some(());
                // Map scan to scan + filter
                let f = |scan, _| {
                    Ok(PhysicalPlan::Filter(
                        Box::new(scan),
                        match leaf_exprs.len() {
                            1 => leaf_exprs.swap_remove(0),
                            _ => PhysicalExpr::LogOp(LogOp::And, leaf_exprs),
                        },
                    ))
                };
                // Remove top level filter if all conditions were pushable
                if root_exprs.is_empty() {
                    return input.map_if(f, ok);
                }
                // Otherwise remove exprs from top level filter and push
                Ok(PhysicalPlan::Filter(
                    Box::new(input.map_if(f, ok)?),
                    match root_exprs.len() {
                        1 => root_exprs.swap_remove(0),
                        _ => PhysicalExpr::LogOp(LogOp::And, root_exprs),
                    },
                ))
            }
            _ => Ok(plan),
        }
    }
}

/// The first pass preserves the old equality-scan support boundary while
/// representing all exact keys through one physical probe expression.
const MAX_EXACT_INDEX_COLS: usize = 3;

pub(crate) struct IxScanFromPredicates;

pub(crate) struct IxScanInfo {
    index_id: IndexId,
    consumed_exprs: Vec<usize>,
    probe: PhysicalExpr,
}

fn top_level_filter_exprs(expr: &PhysicalExpr) -> Vec<&PhysicalExpr> {
    match expr {
        PhysicalExpr::LogOp(LogOp::And, exprs) => exprs.iter().collect(),
        expr => vec![expr],
    }
}

fn equality_term(expr: &PhysicalExpr, label: Label) -> Option<(ColId, AlgebraicValue)> {
    let PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs) = expr else {
        return None;
    };
    let (PhysicalExpr::Field(field), PhysicalExpr::Value(value)) = (&**lhs, &**rhs) else {
        return None;
    };
    (field.label == label).then(|| (ColId(field.field_pos as u16), value.clone()))
}

impl RewriteRule for IxScanFromPredicates {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        let PhysicalPlan::Filter(input, expr) = plan else {
            return None;
        };

        let PhysicalPlan::TableScan(
            TableScan {
                schema,
                limit: None,
                delta: _,
            },
            label,
        ) = &**input
        else {
            return None;
        };

        let exprs = top_level_filter_exprs(expr);
        let equality_terms = exprs
            .iter()
            .enumerate()
            .filter_map(|(i, expr)| equality_term(expr, *label).map(|(col, value)| (i, col, value)))
            .collect::<Vec<_>>();

        let mut best = None;
        for index in &schema.indexes {
            let cols = index.index_algorithm.columns();
            let cols = cols.iter().collect::<Vec<_>>();
            if cols.is_empty() || cols.len() > MAX_EXACT_INDEX_COLS {
                continue;
            }

            let mut consumed_exprs = Vec::with_capacity(cols.len());
            let mut key_parts = Vec::with_capacity(cols.len());
            for col in &cols {
                let Some((expr_pos, _, value)) = equality_terms.iter().find(|(_, candidate, _)| candidate == col)
                else {
                    continue;
                };
                consumed_exprs.push(*expr_pos);
                key_parts.push(PhysicalExpr::Value(value.clone()));
            }

            if consumed_exprs.len() != cols.len() {
                continue;
            }

            let candidate = IxScanInfo {
                index_id: index.index_id,
                consumed_exprs,
                probe: index_key_expr(key_parts),
            };

            if best
                .as_ref()
                .is_none_or(|best: &IxScanInfo| cols.len() > best.consumed_exprs.len())
            {
                best = Some(candidate);
            }
        }

        best
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> Result<PhysicalPlan> {
        let PhysicalPlan::Filter(input, expr) = plan else {
            bail!("{INVARIANT_VIOLATION}: Failed to create index scan from predicates");
        };
        let PhysicalPlan::TableScan(TableScan { schema, limit, delta }, label) = *input else {
            bail!("{INVARIANT_VIOLATION}: Failed to create index scan from predicates");
        };

        let ix_scan = PhysicalPlan::IxScan(
            IxScan {
                schema,
                limit,
                delta,
                index_id: info.index_id,
                probe: IndexProbe::Point(info.probe),
            },
            label,
        );

        let residual_exprs = match expr {
            PhysicalExpr::LogOp(LogOp::And, exprs) => exprs
                .into_iter()
                .enumerate()
                .filter(|(pos, _)| !info.consumed_exprs.contains(pos))
                .map(|(_, expr)| expr)
                .collect(),
            _ if info.consumed_exprs == [0] => vec![],
            expr => vec![expr],
        };

        Ok(match and_expr(residual_exprs) {
            Some(expr) => PhysicalPlan::Filter(Box::new(ix_scan), expr),
            None => ix_scan,
        })
    }
}

/// Reorder hash joins so that selections are on the lhs.
///
/// ```text
///   x
///  / \
/// a  s(b)
///     |
///     b
///
/// ... to ...
///
///    x
///   / \
/// s(b) a
///  |
///  b
/// ```
pub(crate) struct ReorderHashJoin;

impl RewriteRule for ReorderHashJoin {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::HashJoin(HashJoin { lhs, rhs, .. }, Semi::All) => {
                (matches!(&**lhs, PhysicalPlan::TableScan(TableScan { delta: None, .. }, _))
                    && !matches!(&**rhs, PhysicalPlan::TableScan(..)))
                .then_some(())
            }
            _ => None,
        }
    }

    // Swaps both the inputs and the fields
    fn rewrite(plan: Self::Plan, _: Self::Info) -> Result<Self::Plan> {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => Ok(PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    lhs_field: join.rhs_field,
                    rhs_field: join.lhs_field,
                    unique: join.unique,
                },
                Semi::All,
            )),
            _ => Ok(plan),
        }
    }
}

/// Reorder a hash join if the rhs is a delta scan, but the lhs is not.
///
/// ```text
///    x
///   / \
/// s(b) a
///  |
///  b
///
/// ... to ...
///
///   x
///  / \
/// a  s(b)
///     |
///     b
/// ```
pub(crate) struct ReorderDeltaJoinRhs;

impl RewriteRule for ReorderDeltaJoinRhs {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) if join.rhs.is_delta_scan() && !join.lhs.is_delta_scan() => {
                Some(())
            }
            _ => None,
        }
    }

    // Swaps both the inputs and the fields
    fn rewrite(plan: Self::Plan, _: Self::Info) -> Result<Self::Plan> {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => Ok(PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    lhs_field: join.rhs_field,
                    rhs_field: join.lhs_field,
                    unique: join.unique,
                },
                Semi::All,
            )),
            _ => Ok(plan),
        }
    }
}

/// Always prefer an index join to a hash join
pub(crate) struct HashToIxJoin;

/// A filter term of the form `rhs_col = constant`.
type EqConstFilterTerm = (ColId, AlgebraicValue);

/// Planning metadata derived while proving `HashJoin -> IxJoin` is valid.
#[derive(Clone)]
pub(crate) struct HashToIxJoinInfo {
    /// The index to probe on the RHS table.
    rhs_index: IndexId,
    /// The point probe expression to evaluate against each lhs tuple.
    probe: PhysicalExpr,
    /// RHS filter terms consumed into the point probe and therefore removable
    /// from the residual filter.
    consumed_filter_terms: Vec<EqConstFilterTerm>,
}

/// Collect RHS predicates of the form `rhs.col = constant`.
///
/// This is intentionally narrow: only equality predicates over RHS fields are
/// useful for building exact prefix probes into multi-column indexes.
fn rhs_eq_constants(expr: &PhysicalExpr, rhs_label: Label) -> Vec<EqConstFilterTerm> {
    match expr {
        PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs)
            if matches!(
                (&**lhs, &**rhs),
                (PhysicalExpr::Field(TupleField { label, .. }), PhysicalExpr::Value(_)) if *label == rhs_label
            ) =>
        {
            match (&**lhs, &**rhs) {
                (PhysicalExpr::Field(TupleField { field_pos, .. }), PhysicalExpr::Value(value)) => {
                    vec![(ColId(*field_pos as u16), value.clone())]
                }
                _ => vec![],
            }
        }
        PhysicalExpr::LogOp(LogOp::And, exprs) => exprs
            .iter()
            .flat_map(|expr| rhs_eq_constants(expr, rhs_label))
            .collect(),
        _ => vec![],
    }
}

/// Collect equality constants from a list of filter expressions.
fn rhs_eq_constants_from_filters<'a>(
    filters: impl IntoIterator<Item = &'a PhysicalExpr>,
    rhs_label: Label,
) -> Vec<EqConstFilterTerm> {
    filters
        .into_iter()
        .flat_map(|expr| rhs_eq_constants(expr, rhs_label))
        .collect()
}

/// Peel a chain of `Filter` nodes and return:
/// 1) the non-filter base plan
/// 2) all filter expressions in that chain
fn peel_filters_ref(mut plan: &PhysicalPlan) -> (&PhysicalPlan, Vec<&PhysicalExpr>) {
    let mut filters = Vec::new();
    while let PhysicalPlan::Filter(input, expr) = plan {
        filters.push(expr);
        plan = input;
    }
    (plan, filters)
}

/// Owned variant of `peel_filters_ref`.
fn peel_filters_owned(mut plan: PhysicalPlan) -> (PhysicalPlan, Vec<PhysicalExpr>) {
    let mut filters = Vec::new();
    while let PhysicalPlan::Filter(input, expr) = plan {
        filters.push(expr);
        plan = *input;
    }
    (plan, filters)
}

fn and_expr(exprs: Vec<PhysicalExpr>) -> Option<PhysicalExpr> {
    match exprs.len() {
        0 => None,
        1 => exprs.into_iter().next(),
        _ => Some(PhysicalExpr::LogOp(LogOp::And, exprs)),
    }
}

fn ix_join_candidate(
    schema: &TableSchema,
    rhs_field_pos: usize,
    lhs_field: &TupleField,
    constants: &[EqConstFilterTerm],
) -> Option<HashToIxJoinInfo> {
    let rhs_field = ColId(rhs_field_pos as u16);
    schema.indexes.iter().find_map(|ix| {
        let cols = ix.index_algorithm.columns();
        // For point-probe index joins we require:
        // 1) join key is the last index column,
        // 2) every leading index column is fixed by an RHS equality constant.
        if cols.iter().last()? != rhs_field {
            return None;
        };

        let mut key_parts = vec![];
        let mut consumed_filter_terms: Vec<EqConstFilterTerm> = vec![];
        for col in cols.iter().take(cols.len().saturating_sub(1).into()) {
            let (_, value) = constants.iter().find(|(candidate, _)| *candidate == col)?;
            key_parts.push(PhysicalExpr::Value(value.clone()));
            consumed_filter_terms.push((col, value.clone()));
        }
        key_parts.push(PhysicalExpr::Field(lhs_field.clone()));

        Some(HashToIxJoinInfo {
            rhs_index: ix.index_id,
            probe: index_key_expr(key_parts),
            consumed_filter_terms,
        })
    })
}

fn remove_consumed_filter_terms(
    expr: PhysicalExpr,
    rhs_label: Label,
    consumed_filter_terms: &[EqConstFilterTerm],
) -> Option<PhysicalExpr> {
    // These terms are now represented by the point probe; keeping them in a
    // residual filter is redundant work.
    let is_consumed = |col_id: ColId, value: &AlgebraicValue| {
        consumed_filter_terms
            .iter()
            .any(|(consumed_col, consumed_val)| consumed_col == &col_id && consumed_val == value)
    };

    match expr {
        PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs)
            if matches!(
                (&*lhs, &*rhs),
                (PhysicalExpr::Field(TupleField { label, field_pos, .. }), PhysicalExpr::Value(value))
                    if *label == rhs_label && is_consumed(ColId(*field_pos as u16), value)
            ) =>
        {
            None
        }
        PhysicalExpr::LogOp(LogOp::And, exprs) => {
            let mut kept: Vec<_> = exprs
                .into_iter()
                .filter_map(|expr| remove_consumed_filter_terms(expr, rhs_label, consumed_filter_terms))
                .collect();

            match kept.len() {
                0 => None,
                1 => Some(kept.swap_remove(0)),
                _ => Some(PhysicalExpr::LogOp(LogOp::And, kept)),
            }
        }
        expr => Some(expr),
    }
}

impl RewriteRule for HashToIxJoin {
    type Plan = PhysicalPlan;
    type Info = HashToIxJoinInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::HashJoin(
                HashJoin {
                    rhs,
                    lhs_field,
                    rhs_field: TupleField { field_pos, .. },
                    ..
                },
                _,
            ) => {
                let (base, filters) = peel_filters_ref(rhs);
                match base {
                    PhysicalPlan::TableScan(
                        TableScan {
                            schema,
                            limit: None,
                            delta: _,
                        },
                        rhs_label,
                    ) => {
                        let constants = rhs_eq_constants_from_filters(filters, *rhs_label);
                        ix_join_candidate(schema, *field_pos, lhs_field, &constants)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::HashJoin(join, semi) => {
                let HashToIxJoinInfo {
                    rhs_index,
                    probe,
                    consumed_filter_terms,
                } = info;

                let (rhs_plan, rhs_filters) = peel_filters_owned(*join.rhs);
                let (rhs, rhs_label, rhs_delta) = match rhs_plan {
                    PhysicalPlan::TableScan(
                        TableScan {
                            schema: rhs,
                            limit: None,
                            delta: rhs_delta,
                        },
                        rhs_label,
                    ) => (rhs, rhs_label, rhs_delta),
                    _ => bail!("{INVARIANT_VIOLATION}: Failed to rewrite hash join as index join"),
                };

                let ix_join = PhysicalPlan::IxJoin(
                    IxJoin {
                        lhs: join.lhs,
                        rhs,
                        rhs_label,
                        rhs_index,
                        rhs_delta,
                        unique: false,
                        probe,
                    },
                    semi,
                );

                let residual_filters = rhs_filters
                    .into_iter()
                    .filter_map(|expr| remove_consumed_filter_terms(expr, rhs_label, &consumed_filter_terms))
                    .collect();

                if let Some(expr) = and_expr(residual_filters) {
                    Ok(PhysicalPlan::Filter(Box::new(ix_join), expr))
                } else {
                    Ok(ix_join)
                }
            }
            _ => bail!("{INVARIANT_VIOLATION}: Failed to rewrite hash join as index join"),
        }
    }
}

/// Does this index join use a unique index?
pub(crate) struct UniqueIxJoinRule;

impl RewriteRule for UniqueIxJoinRule {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::IxJoin(join @ IxJoin { unique: false, rhs, .. }, _) = plan
            && let Some((rhs_field, _)) = join.single_probe_field()
        {
            return rhs
                .constraints
                .iter()
                .filter_map(|cs| cs.data.unique_columns())
                .filter_map(|cols| cols.as_singleton())
                .find(|col_id| *col_id == rhs_field)
                .map(|_| ());
        }
        None
    }

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> Result<PhysicalPlan> {
        if let PhysicalPlan::IxJoin(IxJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        Ok(plan)
    }
}

/// Does probing the hash table return at most one element?
pub(crate) struct UniqueHashJoinRule;

impl RewriteRule for UniqueHashJoinRule {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                unique: false,
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
        ) = plan
        {
            return rhs
                .returns_distinct_values(rhs_label, &ColSet::from(ColId(*rhs_field_pos as u16)))
                .then_some(());
        }
        None
    }

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> Result<PhysicalPlan> {
        if let PhysicalPlan::HashJoin(HashJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        Ok(plan)
    }
}
