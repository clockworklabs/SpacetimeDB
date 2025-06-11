//! This module defines the rewrite rules used for query optimization.
//!
//! These include:
//!
//! * [PushConstEq]  
//!   Push down predicates of the form `x=1`
//! * [PushConstAnd]  
//! * [IxScanBinOp]
//!   Generate 1-column index scan for `x=1`
//! * [ReorderDeltaJoinRhs]
//!   Reorder the sides of a hash join with delta tables
//! * [PullFilterAboveHashJoin]
//!   Pull a filter above a hash join with delta tables
//! * [HashToIxJoin]
//!   Convert hash join to index join
//! * [UniqueIxJoinRule]
//!   Mark index join as unique
//! * [UniqueHashJoinRule]
//!   Mark hash join as unique
//!
//! We optimize with the following rules in mind:
//! - When all the comparisons are`!=` is always a full table scan
//! - Multi-column indexes are only used if the query has a prefix match (ie all operators are `=`)
//! - Else are converted to a single column index scan on the leftmost column after `=` and a filter on the rest

use crate::plan::{
    HashJoin, IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Sarg, Semi, TableScan,
    TupleField,
};
use anyhow::{bail, Result};
use either::Either;
use itertools::Itertools;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::IndexSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};
use std::collections::{HashMap, HashSet};

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
            PhysicalPlan::IxJoin(
                IxJoin {
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
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            )
            | PhysicalPlan::HashJoin(
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
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(_, expr, value)) = plan {
            if let (PhysicalExpr::Field(TupleField { label, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value) {
                return (input.has_table_scan(Some(label)) && !is_filter(input)).then_some(*label);
            }
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
                if let PhysicalExpr::BinOp(_, expr, value) = expr {
                    if let (PhysicalExpr::Field(TupleField { label, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value)
                    {
                        return (input.has_table_scan(Some(label)) && !is_filter(input)).then_some(*label);
                    }
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
                    if let PhysicalExpr::BinOp(_, lhs, value) = &expr {
                        if let (PhysicalExpr::Field(TupleField { label: var, .. }), PhysicalExpr::Value(_)) =
                            (&**lhs, &**value)
                        {
                            if var == &relvar {
                                leaf_exprs.push(expr);
                                continue;
                            }
                        }
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

/// Match single field for [`BinOp`] predicates such as:
///
/// ```sql
/// select * from t where x = 1
/// ```
///
/// Rewrite as an index scan if applicable.
///
/// NOTE: This rule does not consider multi-column indexes, or [`BinOp::Ne`] predicates.
pub(crate) struct IxScanBinOp;

impl RewriteRule for IxScanBinOp {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(op, expr, value)) = plan {
            if *op == BinOp::Ne {
                return None;
            }
            if let PhysicalPlan::TableScan(
                TableScan {
                    schema,
                    limit: None,
                    delta: _,
                },
                _,
            ) = &**input
            {
                if let (PhysicalExpr::Field(TupleField { field_pos: pos, .. }), PhysicalExpr::Value(_)) =
                    (&**expr, &**value)
                {
                    return schema.indexes.iter().find_map(
                        |IndexSchema {
                             index_id,
                             index_algorithm,
                             ..
                         }| {
                            // TODO: Support prefix scans
                            if index_algorithm.columns().len() == 1 {
                                Some((*index_id, index_algorithm.find_col_index(*pos)?))
                            } else {
                                None
                            }
                        },
                    );
                }
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (index_id, col_id): Self::Info) -> Result<PhysicalPlan> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(op, _, value)) = plan {
            if let PhysicalPlan::TableScan(TableScan { schema, limit, delta }, var) = *input {
                if let PhysicalExpr::Value(v) = *value {
                    return Ok(PhysicalPlan::IxScan(
                        IxScan {
                            schema,
                            limit,
                            delta,
                            index_id,
                            prefix: vec![],
                            arg: Sarg::from_op(op, col_id, v),
                        },
                        var,
                    ));
                }
            }
        }
        bail!("{INVARIANT_VIOLATION}: Failed to create single column index scan from equality condition")
    }
}

/// Match `1...N` multi-field [`BinOp`] predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1 // Full match
/// select * from t where x = 1 and y > 1 // Partial match
/// ```
///
/// Rewrite as a multi-column index scan if applicable.
///
/// NOTE: This rule does not consider [`BinOp::Ne`] predicates.
pub(crate) struct IxScanOpMultiCol;

#[derive(Debug, Clone)]
struct ColInfo {
    op: BinOp,
    field: TupleField,
    value: AlgebraicValue,
}

#[derive(Debug)]
struct IndexMatch {
    index_id: IndexId,
    index_name: Box<str>,
    matched_columns: Vec<ColInfo>,
}

#[derive(Debug)]
pub(crate) struct IxScanInfo {
    matched: Vec<IndexMatch>,
    unmatched: Vec<usize>,
    unmatched_filter: Vec<ColInfo>,
}

impl RewriteRule for IxScanOpMultiCol {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;
    // We need to match the conditions against the index columns, so that:
    // 1. We match the columns in the same order as the index
    // 2. Build a prefix on the `columns - N` with `BinOp::Eq`
    // 3. Build a [Sarg] on the first column after the prefix (Note: it must always succeed)
    //    [a Eq 1, b Eq 2, c Lte 3] -> [(a, 1), (b, 2), Sarg::from_op(Lte, c, 3)]
    // 4. All the others become unmatched and turns into a filter scan`
    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        // Match a filter with a conjunction of binary expressions
        let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan else {
            return None;
        };
        let PhysicalPlan::TableScan(
            TableScan {
                schema, limit: None, ..
            },
            _,
        ) = &**input
        else {
            return None;
        };

        if schema.indexes.is_empty() {
            return None;
        }
        // Partition expressions into indexable and non-indexable
        let (expr_cols, unmatched): (Vec<_>, Vec<_>) =
            exprs.iter().enumerate().partition_map(|(expr_idx, e)| match e {
                PhysicalExpr::BinOp(op, lhs, rhs)
                    if matches!(op, BinOp::Eq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte) =>
                {
                    if let (PhysicalExpr::Field(field), PhysicalExpr::Value(value)) = (&**lhs, &**rhs) {
                        Either::Left(ColInfo {
                            op: *op,
                            field: field.clone(),
                            value: value.clone(),
                        })
                    } else {
                        Either::Right(expr_idx)
                    }
                }
                _ => Either::Right(expr_idx),
            });

        if expr_cols.is_empty() {
            return None;
        }

        // We should allow for same column to be used multiple times, like `x = 1 and x > 2`.
        let mut cond_map: HashMap<ColId, Vec<&ColInfo>> = HashMap::new();
        let binding = expr_cols.clone();
        for c in &binding {
            cond_map.entry(c.field.field_pos.into()).or_default().push(c);
        }

        let mut candidates = vec![];

        for idx in &schema.indexes {
            let mut prefix_buf: Vec<&ColInfo> = vec![];
            let mut has_range = false;

            for col_id in idx.index_algorithm.columns().iter() {
                let Some(conds) = cond_map.get(&col_id) else { break };
                let mut pushed = false;

                for cond in conds {
                    // If we have duplicate fields like in `x = 1 and x = 2 and y = 3`, then split:
                    // prefix: [x = 1, y = 3]
                    // extra : [x = 2]
                    if prefix_buf.iter().any(|c| c.field.field_pos == cond.field.field_pos) {
                        if cond.op == BinOp::Eq || !has_range {
                            candidates.push((idx.index_id, idx.index_name.clone(), vec![*cond]));
                        }

                        continue;
                    }
                    match cond.op {
                        BinOp::Eq => {
                            prefix_buf.push(*cond);
                            pushed = true;
                        }
                        // Only take the first range condition
                        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte if !has_range => {
                            prefix_buf.push(*cond);
                            has_range = true;
                            break;
                        }
                        _ => {}
                    }
                }

                if !pushed {
                    break;
                }
            }

            if !prefix_buf.is_empty() {
                candidates.push((idx.index_id, idx.index_name.clone(), prefix_buf));
            }
        }
        if candidates.is_empty() {
            return None;
        }
        // Match the index with the longest prefix first
        candidates.sort_by_key(|(_, _, cols)| -(cols.len() as isize));

        let mut matched = vec![];
        let mut covered = HashSet::new();

        for (index_id, index_name, prefix) in candidates {
            if prefix
                .iter()
                .all(|c| covered.contains(&(c.op, c.field.field_pos, &c.value)))
            {
                continue;
            }
            covered.extend(prefix.iter().map(|c| (c.op, c.field.field_pos, &c.value)));

            matched.push(IndexMatch {
                index_id,
                index_name,
                matched_columns: prefix.into_iter().cloned().collect(),
            });
        }

        if matched.is_empty() {
            return None;
        }

        matched.sort_by_key(|m| m.index_name.clone());

        let unmatched_filter = expr_cols
            .into_iter()
            .filter(|c| !covered.contains(&(c.op, c.field.field_pos, &c.value)))
            .collect();

        Some(IxScanInfo {
            matched,
            unmatched,
            unmatched_filter,
        })
    }

    fn rewrite(plan: Self::Plan, info: Self::Info) -> Result<Self::Plan> {
        let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan else {
            bail!("{INVARIANT_VIOLATION}: Expected Filter(LogOp::And)")
        };

        let PhysicalPlan::TableScan(scan, label) = *input else {
            bail!("{INVARIANT_VIOLATION}: Expected TableScan")
        };

        let mut plans = Vec::with_capacity(info.matched.len());
        for m in &info.matched {
            let (prefix, arg) = {
                let take = m.matched_columns.len() - 1;

                let mut it = m.matched_columns.clone().into_iter();
                let prefix: Vec<_> = it
                    .by_ref()
                    .take(take)
                    .map(|c| (c.field.field_pos.into(), c.value.clone()))
                    .collect();

                let arg = it
                    .next()
                    .map(|c| Sarg::from_op(c.op, c.field.field_pos.into(), c.value))
                    .unwrap();

                (prefix, arg)
            };

            plans.push((
                m,
                PhysicalPlan::IxScan(
                    IxScan {
                        schema: scan.schema.clone(),
                        limit: scan.limit,
                        delta: scan.delta,
                        index_id: m.index_id,
                        prefix,
                        arg,
                    },
                    label,
                ),
            ));
        }
        let mut unmatched_filter = info.unmatched_filter;
        let index_plan = if plans.len() == 1 {
            plans.pop().unwrap().1
        } else {
            // Must add back the filter expression because the index matches need to form an intersection
            // of the index scans, like in `x > 1 and y < 2` each index scan output partial matches.
            unmatched_filter.extend(info.matched.iter().flat_map(|m| m.matched_columns.iter()).cloned());

            // Sort by the number of matched columns by `=` operator, so it scan less rows later
            let plans = plans
                .into_iter()
                .sorted_by_key(|(m, _)| -(m.matched_columns.iter().filter(|c| c.op == BinOp::Eq).count() as isize))
                .map(|(_, plan)| plan)
                .collect();
            PhysicalPlan::IxScansAnd(plans)
        };

        // No unmatched filters: done
        if info.unmatched.is_empty() && unmatched_filter.is_empty() {
            return Ok(index_plan);
        }

        // Reconstruct remaining filter expressions
        let remaining = exprs
            .into_iter()
            .enumerate()
            .filter(|(i, _)| info.unmatched.contains(i))
            .map(|(_, e)| e)
            .chain(unmatched_filter.into_iter().map(|c| {
                PhysicalExpr::BinOp(
                    c.op,
                    Box::new(PhysicalExpr::Field(c.field)),
                    Box::new(PhysicalExpr::Value(c.value)),
                )
            }))
            .collect();

        Ok(PhysicalPlan::Filter(
            Box::new(index_plan),
            PhysicalExpr::LogOp(LogOp::And, remaining),
        ))
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
        if let PhysicalPlan::HashJoin(HashJoin { lhs, rhs, .. }, Semi::All) = plan {
            return (match &**lhs {
                PhysicalPlan::Filter(lhs, _) => rhs.is_delta_scan() && !lhs.is_delta_scan(),
                lhs => rhs.is_delta_scan() && !lhs.is_delta_scan(),
            })
            .then_some(());
        }
        None
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

/// Pull a filter above a hash join if:
///
/// 1. The lhs is a delta scan
/// 2. The rhs has an index for the join
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
///  s(b)
///   |
///   x
///  / \
/// a   b
/// ```
pub(crate) struct PullFilterAboveHashJoin;

impl RewriteRule for PullFilterAboveHashJoin {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                lhs, rhs, rhs_field, ..
            },
            Semi::All,
        ) = plan
        {
            if let PhysicalPlan::Filter(input, _) = &**rhs {
                if let PhysicalPlan::TableScan(TableScan { schema, .. }, _) = &**input {
                    return (lhs.is_delta_scan()
                        && schema.indexes.iter().any(|schema| {
                            schema
                                .index_algorithm
                                .columns()
                                .as_singleton()
                                .is_some_and(|col_id| col_id.idx() == rhs_field.field_pos)
                        }))
                    .then_some(());
                }
            }
        }
        None
    }

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Result<Self::Plan> {
        if let PhysicalPlan::HashJoin(join, semi) = plan {
            if let PhysicalPlan::Filter(rhs, expr) = *join.rhs {
                return Ok(PhysicalPlan::Filter(
                    Box::new(PhysicalPlan::HashJoin(
                        HashJoin {
                            lhs: join.lhs,
                            rhs,
                            ..join
                        },
                        semi,
                    )),
                    expr,
                ));
            }
        }
        bail!("{INVARIANT_VIOLATION}: Failed to pull filter above hash join")
    }
}

/// Always prefer an index join to a hash join
pub(crate) struct HashToIxJoin;

impl RewriteRule for HashToIxJoin {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                rhs,
                rhs_field: TupleField { field_pos, .. },
                ..
            },
            _,
        ) = plan
        {
            return match &**rhs {
                PhysicalPlan::TableScan(
                    TableScan {
                        schema,
                        limit: None,
                        delta: _,
                    },
                    _,
                ) => {
                    // Is there a single column index on this field?
                    schema.indexes.iter().find_map(|ix| {
                        ix.index_algorithm
                            .columns()
                            .as_singleton()
                            .filter(|col_id| col_id.idx() == *field_pos)
                            .map(|col_id| (ix.index_id, col_id))
                    })
                }
                _ => None,
            };
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (rhs_index, rhs_field): Self::Info) -> Result<PhysicalPlan> {
        if let PhysicalPlan::HashJoin(join, semi) = plan {
            if let PhysicalPlan::TableScan(
                TableScan {
                    schema: rhs,
                    limit: None,
                    delta: rhs_delta,
                },
                rhs_label,
            ) = *join.rhs
            {
                return Ok(PhysicalPlan::IxJoin(
                    IxJoin {
                        lhs: join.lhs,
                        rhs,
                        rhs_label,
                        rhs_index,
                        rhs_field,
                        rhs_delta,
                        unique: false,
                        lhs_field: join.lhs_field,
                    },
                    semi,
                ));
            }
        }
        bail!("{INVARIANT_VIOLATION}: Failed to rewrite hash join as index join")
    }
}

/// Does this index join use a unique index?
pub(crate) struct UniqueIxJoinRule;

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
