//! This module defines the rewrite rules used for query optimization.
//!
//! These include:
//!
//! * [PushConstEq]  
//!     Push down predicates of the form `x=1`
//! * [PushConstAnd]  
//!     Push down predicates of the form `x=1 and y=2`
//! * [IxScanEq]  
//!     Generate 1-column index scan for `x=1`
//! * [IxScanAnd]  
//!     Generate 1-column index scan for `x=1 and y=2`
//! * [IxScanEq2Col]  
//!     Generate 2-column index scan
//! * [IxScanEq3Col]  
//!     Generate 3-column index scan
//! * [ReorderHashJoin]  
//!     Reorder the sides of a hash join
//! * [ReorderDeltaJoinRhs]
//!     Reorder the sides of a hash join with delta tables
//! * [PullFilterAboveHashJoin]
//!     Pull a filter above a hash join with delta tables
//! * [HashToIxJoin]  
//!     Convert hash join to index join
//! * [UniqueIxJoinRule]  
//!     Mark index join as unique
//! * [UniqueHashJoinRule]  
//!     Mark hash join as unique
use std::ops::Bound;

use anyhow::{bail, Result};
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

use crate::plan::{HashJoin, IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, Sarg, Semi, TupleField};

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
    type Info = Label;

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::Filter(_, expr) => {
                let mut label = None;
                expr.visit(&mut |expr| {
                    if let PhysicalExpr::Field(t @ TupleField { label_pos: None, .. }) = expr {
                        label = Some(t.label);
                    }
                });
                label
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
            ) => Some(t.label),
            _ => None,
        }
    }

    fn rewrite(mut plan: Self::Plan, label: Self::Info) -> Result<Self::Plan> {
        match &mut plan {
            PhysicalPlan::Filter(input, expr) => {
                if let Some(i) = input.position(&label) {
                    expr.visit_mut(&mut |expr| match expr {
                        PhysicalExpr::Field(t @ TupleField { label_pos: None, .. }) if t.label == label => {
                            t.label_pos = Some(i);
                        }
                        _ => {}
                    });
                }
            }
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs: input,
                    lhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            )
            | PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: input,
                    lhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            )
            | PhysicalPlan::HashJoin(
                HashJoin {
                    rhs: input,
                    rhs_field: t @ TupleField { label_pos: None, .. },
                    ..
                },
                _,
            ) => {
                if let Some(i) = input.position(&label) {
                    t.label_pos = Some(i);
                }
            }
            _ => {}
        }
        Ok(plan)
    }
}

/// Mark or flag sargable equality conditions such as:
///
/// ```sql
/// select * from t where x = 3
/// ```
pub(crate) struct SargableEq;

impl RewriteRule for SargableEq {
    type Plan = PhysicalExpr;
    type Info = ();

    fn matches(expr: &PhysicalExpr) -> Option<Self::Info> {
        match expr {
            PhysicalExpr::BinOp(BinOp::Eq, lhs, rhs) => {
                (matches!(&**lhs, PhysicalExpr::Field(_)) && matches!(&**rhs, PhysicalExpr::Value(_))).then_some(())
            }
            _ => None,
        }
    }

    fn rewrite(expr: PhysicalExpr, _: Self::Info) -> Result<Self::Plan> {
        match expr {
            PhysicalExpr::BinOp(op, lhs, rhs) => match (*lhs, *rhs) {
                (PhysicalExpr::Field(lhs), PhysicalExpr::Value(value)) => Ok(PhysicalExpr::Equal(lhs, value)),
                (lhs, rhs) => Ok(PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs))),
            },
            _ => Ok(expr),
        }
    }
}

/// Mark or flag sargable range conditions such as:
///
/// ```sql
/// select * from t where x > 3
/// select * from t where x > 3 and x < 10
/// ```
pub(crate) struct SargableRange;

impl RewriteRule for SargableRange {
    type Plan = PhysicalExpr;
    type Info = ();

    fn matches(expr: &PhysicalExpr) -> Option<Self::Info> {
        match expr {
            PhysicalExpr::BinOp(BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte, lhs, rhs) => {
                (matches!(&**lhs, PhysicalExpr::Field(_)) && matches!(&**rhs, PhysicalExpr::Value(_))).then_some(())
            }
            _ => None,
        }
    }

    fn rewrite(expr: PhysicalExpr, _: Self::Info) -> Result<Self::Plan> {
        match expr {
            PhysicalExpr::BinOp(op, lhs, rhs) => match (*lhs, *rhs, op) {
                (
                    // a < 5
                    PhysicalExpr::Field(field),
                    PhysicalExpr::Value(value),
                    BinOp::Lt,
                ) => Ok(PhysicalExpr::Range(
                    // a in [MIN, 5)
                    field,
                    Bound::Unbounded,
                    Bound::Excluded(value),
                )),
                (
                    // a > 5
                    PhysicalExpr::Field(field),
                    PhysicalExpr::Value(value),
                    BinOp::Gt,
                ) => Ok(PhysicalExpr::Range(
                    // a in (5, MAX]
                    field,
                    Bound::Excluded(value),
                    Bound::Unbounded,
                )),
                (
                    // a <= 5
                    PhysicalExpr::Field(field),
                    PhysicalExpr::Value(value),
                    BinOp::Lte,
                ) => Ok(PhysicalExpr::Range(
                    // a in [MIN, 5]
                    field,
                    Bound::Unbounded,
                    Bound::Included(value),
                )),
                (
                    // a >= 5
                    PhysicalExpr::Field(field),
                    PhysicalExpr::Value(value),
                    BinOp::Gte,
                ) => Ok(PhysicalExpr::Range(
                    // a in [5, MAX]
                    field,
                    Bound::Included(value),
                    Bound::Unbounded,
                )),
                (
                    // Unexpected expression
                    lhs,
                    rhs,
                    op,
                ) => Ok(PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs))),
            },
            _ => Ok(expr),
        }
    }
}

/// Merge sargable range predicates.
///
/// Example:
///
/// ```sql
/// select * from t where x > 3 and x < 10
/// ```
///
/// ... to ...
///
/// ```sql
/// select * from t where x between 3 and 10
/// ```
pub(crate) struct MergeRange;

impl RewriteRule for MergeRange {
    type Plan = PhysicalExpr;
    type Info = (usize, usize);

    fn matches(expr: &PhysicalExpr) -> Option<Self::Info> {
        if let PhysicalExpr::LogOp(LogOp::And, exprs) = expr {
            for (i, a) in exprs.iter().enumerate() {
                for (j, b) in exprs.iter().enumerate().filter(|(j, _)| i < *j) {
                    match (a, b) {
                        (
                            PhysicalExpr::Range(
                                // x in [_, MAX]
                                // x in (_, MAX]
                                field_i,
                                Bound::Included(lower) | Bound::Excluded(lower),
                                Bound::Unbounded,
                            ),
                            PhysicalExpr::Range(
                                // x in [MIN, _]
                                // x in [MIN, _)
                                field_j,
                                Bound::Unbounded,
                                Bound::Included(upper) | Bound::Excluded(upper),
                            ),
                        ) if field_i == field_j && lower < upper => return Some((i, j)),
                        (
                            PhysicalExpr::Range(
                                // x in [MIN, _]
                                // x in [MIN, _)
                                field_i,
                                Bound::Unbounded,
                                Bound::Included(upper) | Bound::Excluded(upper),
                            ),
                            PhysicalExpr::Range(
                                // x in [_, MAX]
                                // x in (_, MAX]
                                field_j,
                                Bound::Included(lower) | Bound::Excluded(lower),
                                Bound::Unbounded,
                            ),
                        ) if field_i == field_j && lower < upper => return Some((i, j)),
                        _ => {}
                    }
                }
            }
        }
        None
    }

    fn rewrite(expr: PhysicalExpr, (i, j): Self::Info) -> Result<Self::Plan> {
        match expr {
            PhysicalExpr::LogOp(LogOp::And, mut exprs) => {
                // Note, because the match guarantees that i < j,
                // the following swap_remove sequence is valid.
                let x = exprs.swap_remove(j);
                let y = exprs.swap_remove(i);
                match (x, y) {
                    (
                        PhysicalExpr::Range(
                            // x in [_, MAX]
                            // x in (_, MAX]
                            field,
                            lower @ Bound::Included(_) | lower @ Bound::Excluded(_),
                            Bound::Unbounded,
                        ),
                        PhysicalExpr::Range(
                            // x in [MIN, _]
                            // x in [MIN, _)
                            _,
                            Bound::Unbounded,
                            upper @ Bound::Included(_) | upper @ Bound::Excluded(_),
                        ),
                    ) => Ok(PhysicalExpr::Range(field, lower, upper)),
                    (
                        PhysicalExpr::Range(
                            // x in [MIN, _]
                            // x in [MIN, _)
                            field,
                            Bound::Unbounded,
                            upper @ Bound::Included(_) | upper @ Bound::Excluded(_),
                        ),
                        PhysicalExpr::Range(
                            // x in [_, MAX]
                            // x in (_, MAX]
                            _,
                            lower @ Bound::Included(_) | lower @ Bound::Excluded(_),
                            Bound::Unbounded,
                        ),
                    ) => Ok(PhysicalExpr::Range(field, lower, upper)),
                    (x, y) => {
                        exprs.push(x);
                        exprs.push(y);
                        Ok(PhysicalExpr::LogOp(LogOp::And, exprs))
                    }
                }
            }
            _ => Ok(expr),
        }
    }
}

/// The top level rule that applies [SargableEq], [SargableRange], and [MergeRange].
pub(crate) struct SargablePredicates;

impl RewriteRule for SargablePredicates {
    type Plan = PhysicalPlan;
    type Info = bool;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::Filter(_, expr) if SargableEq::matches(expr).is_some() => Some(false),
            PhysicalPlan::Filter(_, expr) if SargableRange::matches(expr).is_some() => Some(true),
            _ => None,
        }
    }

    fn rewrite(plan: PhysicalPlan, is_range: Self::Info) -> Result<PhysicalPlan> {
        Ok(match plan {
            PhysicalPlan::Filter(input, expr) if is_range => {
                PhysicalPlan::Filter(input, SargableRange::rewrite(expr, ())?)
            }
            PhysicalPlan::Filter(input, expr) => PhysicalPlan::Filter(input, SargableEq::rewrite(expr, ())?),
            _ => plan,
        })
    }
}

/// Push sargable predicates down to the leaves.
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
/// ```sql
/// select b.*
/// from a join b on a.id = b.id
/// where a.x between 3 and 5
/// ```
///
/// ... to ...
///
/// ```sql
/// select b.*
/// from (select * from a where x between 3 and 5) a
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
pub(crate) struct PushSargable;

impl RewriteRule for PushSargable {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::Equal(field, _) | PhysicalExpr::Range(field, _, _)) = plan {
            return match &**input {
                PhysicalPlan::TableScan(..) => None,
                input => input
                    .any(&|plan| match plan {
                        PhysicalPlan::TableScan(_, alias, _) => *alias == field.label,
                        _ => false,
                    })
                    .then_some(field.label),
            };
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, relvar: Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::Filter(input, expr) => input.map_if(
                |scan, _| Ok(PhysicalPlan::Filter(Box::new(scan), expr)),
                |plan| match plan {
                    PhysicalPlan::TableScan(_, var, _) if var == &relvar => Some(()),
                    _ => None,
                },
            ),
            _ => Ok(plan),
        }
    }
}

/// Push sargable conjunctions down to the leaves.
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
pub(crate) struct PushSargableAnd;

impl RewriteRule for PushSargableAnd {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        match plan {
            PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) => match **input {
                PhysicalPlan::TableScan(..) => None,
                _ => exprs.iter().find_map(|expr| match expr {
                    PhysicalExpr::Equal(field, _) | PhysicalExpr::Range(field, _, _) => input
                        .any(&|plan| match plan {
                            PhysicalPlan::TableScan(_, alias, _) => *alias == field.label,
                            _ => false,
                        })
                        .then_some(field.label),
                    _ => None,
                }),
            },
            _ => None,
        }
    }

    fn rewrite(plan: PhysicalPlan, table_alias: Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) => {
                let mut leaf_exprs = vec![];
                let mut root_exprs = vec![];
                for expr in exprs {
                    if let PhysicalExpr::Equal(field, _) | PhysicalExpr::Range(field, _, _) = &expr {
                        if field.label == table_alias {
                            leaf_exprs.push(expr);
                            continue;
                        }
                    }
                    root_exprs.push(expr);
                }
                // Match on table or delta scan
                let ok = |plan: &PhysicalPlan| match plan {
                    PhysicalPlan::TableScan(_, var, _) if var == &table_alias => Some(()),
                    _ => None,
                };
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

pub(crate) struct IxScanInfo {
    index_id: IndexId,
    cols: Vec<(usize, ColId)>,
}

/// Match a single sargable condition such as:
///
/// ```sql
/// select * from t where x = 1
/// ```
///
/// Rewrite as an index scan if applicable.
///
/// NOTE: This rule does not consider multi-column indexes.
pub(crate) struct IxScanFromFilter;

impl RewriteRule for IxScanFromFilter {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::Equal(lhs, _) | PhysicalExpr::Range(lhs, _, _)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                return schema
                    .find_btree_index_id_for_col_pos(lhs.field_pos)
                    .map(|index_id| (index_id, ColId(lhs.field_pos as u16)));
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (index_id, col_id): Self::Info) -> Result<PhysicalPlan> {
        match plan {
            PhysicalPlan::Filter(input, PhysicalExpr::Equal(lhs, value)) => match *input {
                PhysicalPlan::TableScan(schema, var, None) => Ok(PhysicalPlan::IxScan(
                    IxScan {
                        schema,
                        index_id,
                        prefix: vec![],
                        arg: Sarg::Eq(col_id, value),
                    },
                    var,
                )),
                input => Ok(PhysicalPlan::Filter(Box::new(input), PhysicalExpr::Equal(lhs, value))),
            },
            PhysicalPlan::Filter(input, PhysicalExpr::Range(lhs, lower, upper)) => match *input {
                PhysicalPlan::TableScan(schema, var, None) => Ok(PhysicalPlan::IxScan(
                    IxScan {
                        schema,
                        index_id,
                        prefix: vec![],
                        arg: Sarg::Range(col_id, lower, upper),
                    },
                    var,
                )),
                input => Ok(PhysicalPlan::Filter(
                    Box::new(input),
                    PhysicalExpr::Range(lhs, lower, upper),
                )),
            },
            _ => Ok(plan),
        }
    }
}

/// Match multiple sargable predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1
/// ```
///
/// And create an index scan for one of them.
///
/// NOTE: This rule does not consider multi-column indexes.
pub(crate) struct IxScanPlusFilter;

impl RewriteRule for IxScanPlusFilter {
    type Plan = PhysicalPlan;
    type Info = (IndexId, usize, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                return exprs.iter().enumerate().find_map(|(i, expr)| {
                    if let PhysicalExpr::Equal(lhs, _) | PhysicalExpr::Range(lhs, _, _) = expr {
                        return schema
                            .find_btree_index_id_for_col_pos(lhs.field_pos)
                            .map(|index_id| (index_id, i, ColId(lhs.field_pos as u16)));
                    }
                    None
                });
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (index_id, i, col_id): Self::Info) -> Result<PhysicalPlan> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, mut exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, label, None) = *input {
                match exprs.swap_remove(i) {
                    PhysicalExpr::Equal(_, value) => {
                        return Ok(PhysicalPlan::Filter(
                            Box::new(PhysicalPlan::IxScan(
                                IxScan {
                                    schema,
                                    index_id,
                                    prefix: vec![],
                                    arg: Sarg::Eq(col_id, value),
                                },
                                label,
                            )),
                            match exprs.len() {
                                1 => exprs.swap_remove(0),
                                _ => PhysicalExpr::LogOp(LogOp::And, exprs),
                            },
                        ));
                    }
                    PhysicalExpr::Range(_, lower, upper) => {
                        return Ok(PhysicalPlan::Filter(
                            Box::new(PhysicalPlan::IxScan(
                                IxScan {
                                    schema,
                                    index_id,
                                    prefix: vec![],
                                    arg: Sarg::Range(col_id, lower, upper),
                                },
                                label,
                            )),
                            match exprs.len() {
                                1 => exprs.swap_remove(0),
                                _ => PhysicalExpr::LogOp(LogOp::And, exprs),
                            },
                        ));
                    }
                    _ => {}
                }
            }
        }
        bail!("{INVARIANT_VIOLATION}: Failed to create single column index scan from conjunction")
    }
}

/// Match multiple sargable predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1
/// ```
///
/// Rewrite as a multi-column index scan if applicable.
///
/// NOTE: This rule does not consider indexes on 3 or more columns.
pub(crate) struct IxScan2Col;

impl RewriteRule for IxScan2Col {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan else {
            return None;
        };

        let PhysicalPlan::TableScan(schema, _, None) = &**input else {
            return None;
        };

        for (i, a) in exprs.iter().enumerate() {
            for (j, b) in exprs.iter().enumerate().filter(|(j, _)| i != *j) {
                let (PhysicalExpr::Equal(u, _), PhysicalExpr::Equal(v, _)) = (a, b) else {
                    continue;
                };
                let u = ColId(u.field_pos as u16);
                let v = ColId(v.field_pos as u16);
                if let Some(info) = schema
                    .find_btree_index_for_cols([u, v].into())
                    .map(|index_id| IxScanInfo {
                        index_id,
                        cols: vec![(i, u), (j, v)],
                    })
                {
                    return Some(info);
                }
            }
        }

        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> Result<PhysicalPlan> {
        match info.cols.as_slice() {
            [(i, a), (j, b)] => {
                if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
                    if let PhysicalPlan::TableScan(schema, label, None) = *input {
                        // Remove the ith and jth expressions
                        let remainder = |exprs: Vec<PhysicalExpr>| {
                            exprs
                                .into_iter()
                                .enumerate()
                                .filter(|(pos, _)| pos != i && pos != j)
                                .map(|(_, expr)| expr)
                                .collect::<Vec<_>>()
                        };
                        // Remove the ith and jth expressions and wrap with AND
                        let new_expr = |exprs: Vec<PhysicalExpr>| PhysicalExpr::LogOp(LogOp::And, remainder(exprs));
                        match (exprs.get(*i), exprs.get(*j)) {
                            (Some(PhysicalExpr::Equal(_, u)), Some(PhysicalExpr::Range(_, lower, upper))) => {
                                return Ok(match exprs.len() {
                                    n @ 0 | n @ 1 => {
                                        bail!(
                                            "{INVARIANT_VIOLATION}: Cannot create 2-column index scan from {n} conditions"
                                        )
                                    }
                                    // If there are only 2 conditions in this filter,
                                    // we replace the filter with an index scan.
                                    2 => PhysicalPlan::IxScan(
                                        IxScan {
                                            schema,
                                            index_id: info.index_id,
                                            prefix: vec![(*a, u.clone())],
                                            arg: Sarg::Range(*b, lower.clone(), upper.clone()),
                                        },
                                        label,
                                    ),
                                    // If there are 3 conditions in this filter,
                                    // we create an index scan from 2 of them.
                                    // The original conjunction is no longer well defined,
                                    // because it only has a single operand now.
                                    // Hence we must replace it with its operand.
                                    3 => PhysicalPlan::Filter(
                                        Box::new(PhysicalPlan::IxScan(
                                            IxScan {
                                                schema,
                                                index_id: info.index_id,
                                                prefix: vec![(*a, u.clone())],
                                                arg: Sarg::Range(*b, lower.clone(), upper.clone()),
                                            },
                                            label,
                                        )),
                                        remainder(exprs).swap_remove(0),
                                    ),
                                    // If there are more than 3 conditions in this filter,
                                    // we remove the 2 conditions used in the index scan.
                                    // The remaining conditions still form a conjunction.
                                    _ => PhysicalPlan::Filter(
                                        Box::new(PhysicalPlan::IxScan(
                                            IxScan {
                                                schema,
                                                index_id: info.index_id,
                                                prefix: vec![(*a, u.clone())],
                                                arg: Sarg::Range(*b, lower.clone(), upper.clone()),
                                            },
                                            label,
                                        )),
                                        new_expr(exprs),
                                    ),
                                });
                            }
                            (Some(PhysicalExpr::Equal(_, u)), Some(PhysicalExpr::Equal(_, v))) => {
                                return Ok(match exprs.len() {
                                    n @ 0 | n @ 1 => {
                                        bail!(
                                            "{INVARIANT_VIOLATION}: Cannot create 2-column index scan from {n} conditions"
                                        )
                                    }
                                    // If there are only 2 conditions in this filter,
                                    // we replace the filter with an index scan.
                                    2 => PhysicalPlan::IxScan(
                                        IxScan {
                                            schema,
                                            index_id: info.index_id,
                                            prefix: vec![(*a, u.clone())],
                                            arg: Sarg::Eq(*b, v.clone()),
                                        },
                                        label,
                                    ),
                                    // If there are 3 conditions in this filter,
                                    // we create an index scan from 2 of them.
                                    // The original conjunction is no longer well defined,
                                    // because it only has a single operand now.
                                    // Hence we must replace it with its operand.
                                    3 => PhysicalPlan::Filter(
                                        Box::new(PhysicalPlan::IxScan(
                                            IxScan {
                                                schema,
                                                index_id: info.index_id,
                                                prefix: vec![(*a, u.clone())],
                                                arg: Sarg::Eq(*b, v.clone()),
                                            },
                                            label,
                                        )),
                                        remainder(exprs).swap_remove(0),
                                    ),
                                    // If there are more than 3 conditions in this filter,
                                    // we remove the 2 conditions used in the index scan.
                                    // The remaining conditions still form a conjunction.
                                    _ => PhysicalPlan::Filter(
                                        Box::new(PhysicalPlan::IxScan(
                                            IxScan {
                                                schema,
                                                index_id: info.index_id,
                                                prefix: vec![(*a, u.clone())],
                                                arg: Sarg::Eq(*b, v.clone()),
                                            },
                                            label,
                                        )),
                                        new_expr(exprs),
                                    ),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                bail!("{INVARIANT_VIOLATION}: Failed to create 2-column index scan")
            }
            _ => Ok(plan),
        }
    }
}

/// Match multi-field equality predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1 and z = 1
/// ```
///
/// Rewrite as a multi-column index scan if applicable.
///
/// NOTE: This rule does not consider indexes on 4 or more columns.
pub(crate) struct IxScanEq3Col;

impl RewriteRule for IxScanEq3Col {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        // Match outer plan structure
        let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan else {
            return None;
        };

        let PhysicalPlan::TableScan(schema, _, None) = &**input else {
            return None;
        };

        for (i, a) in exprs.iter().enumerate() {
            for (j, b) in exprs.iter().enumerate().filter(|(j, _)| i != *j) {
                for (k, c) in exprs.iter().enumerate().filter(|(k, _)| i != *k && j != *k) {
                    let (PhysicalExpr::Equal(u, _), PhysicalExpr::Equal(v, _), PhysicalExpr::Equal(w, _)) = (a, b, c)
                    else {
                        continue;
                    };
                    let u = ColId(u.field_pos as u16);
                    let v = ColId(v.field_pos as u16);
                    let w = ColId(w.field_pos as u16);
                    if let Some(scan) = schema
                        .find_btree_index_for_cols([u, v, w].into())
                        .map(|index_id| IxScanInfo {
                            index_id,
                            cols: vec![(i, u), (j, v), (k, w)],
                        })
                    {
                        return Some(scan);
                    }
                }
            }
        }

        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> Result<PhysicalPlan> {
        match info.cols.as_slice() {
            [(i, a), (j, b), (k, c)] => {
                if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
                    if let PhysicalPlan::TableScan(schema, label, None) = *input {
                        if let (
                            Some(PhysicalExpr::Equal(_, u)),
                            Some(PhysicalExpr::Equal(_, v)),
                            Some(PhysicalExpr::Equal(_, w)),
                        ) = (exprs.get(*i), exprs.get(*j), exprs.get(*k))
                        {
                            // Remove the ith and jth expressions
                            let remainder = |exprs: Vec<PhysicalExpr>| {
                                exprs
                                    .into_iter()
                                    .enumerate()
                                    .filter(|(pos, _)| pos != i && pos != j)
                                    .map(|(_, expr)| expr)
                                    .collect::<Vec<_>>()
                            };
                            // Remove the ith and jth expressions and wrap with AND
                            let new_expr = |exprs: Vec<PhysicalExpr>| PhysicalExpr::LogOp(LogOp::And, remainder(exprs));
                            return Ok(match exprs.len() {
                                n @ 0 | n @ 1 | n @ 2 => {
                                    bail!(
                                        "{INVARIANT_VIOLATION}: Cannot create 3-column index scan from {n} conditions"
                                    )
                                }
                                // If there are only 3 conditions in this filter,
                                // we replace the filter with an index scan.
                                3 => PhysicalPlan::IxScan(
                                    IxScan {
                                        schema,
                                        index_id: info.index_id,
                                        prefix: vec![(*a, u.clone()), (*b, v.clone())],
                                        arg: Sarg::Eq(*c, w.clone()),
                                    },
                                    label,
                                ),
                                // If there are 4 conditions in this filter,
                                // we create an index scan from 3 of them.
                                // The original conjunction is no longer well defined,
                                // because it only has a single operand now.
                                // Hence we must replace it with its operand.
                                4 => PhysicalPlan::Filter(
                                    Box::new(PhysicalPlan::IxScan(
                                        IxScan {
                                            schema,
                                            index_id: info.index_id,
                                            prefix: vec![(*a, u.clone()), (*b, v.clone())],
                                            arg: Sarg::Eq(*c, w.clone()),
                                        },
                                        label,
                                    )),
                                    remainder(exprs).swap_remove(0),
                                ),
                                // If there are more than 4 conditions in this filter,
                                // we remove the 3 conditions used in the index scan.
                                // The remaining conditions still form a conjunction.
                                _ => PhysicalPlan::Filter(
                                    Box::new(PhysicalPlan::IxScan(
                                        IxScan {
                                            schema,
                                            index_id: info.index_id,
                                            prefix: vec![(*a, u.clone()), (*b, v.clone())],
                                            arg: Sarg::Eq(*c, w.clone()),
                                        },
                                        label,
                                    )),
                                    new_expr(exprs),
                                ),
                            });
                        }
                    }
                }
                bail!("{INVARIANT_VIOLATION}: Failed to create 3-column index scan")
            }
            _ => Ok(plan),
        }
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
                (matches!(&**lhs, PhysicalPlan::TableScan(..)) && !matches!(&**rhs, PhysicalPlan::TableScan(..)))
                    .then_some(())
            }
            _ => None,
        }
    }

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Result<Self::Plan> {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => Ok(PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    lhs_field: join.rhs_field,
                    rhs_field: join.lhs_field,
                    unique: false,
                },
                Semi::All,
            )),
            _ => Ok(plan),
        }
    }
}

/// Reorder a hash join if the rhs is a delta table.
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
            if let PhysicalPlan::Filter(input, _) = &**lhs {
                return (matches!(&**input, PhysicalPlan::TableScan(_, _, None))
                    && matches!(&**rhs, PhysicalPlan::TableScan(_, _, Some(_))))
                .then_some(());
            }
        }
        None
    }

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Result<Self::Plan> {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => Ok(PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    ..join
                },
                Semi::All,
            )),
            _ => Ok(plan),
        }
    }
}

/// Pull a filter above a hash join if:
///
/// 1. The lhs is a delta table
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
                if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                    return (matches!(&**lhs, PhysicalPlan::TableScan(_, _, Some(_)))
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
                PhysicalPlan::TableScan(schema, _, None) => {
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
            if let PhysicalPlan::TableScan(rhs, rhs_label, None) = *join.rhs {
                return Ok(PhysicalPlan::IxJoin(
                    IxJoin {
                        lhs: join.lhs,
                        rhs,
                        rhs_label,
                        rhs_index,
                        rhs_field,
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
