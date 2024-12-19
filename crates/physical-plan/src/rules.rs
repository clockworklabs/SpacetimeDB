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
use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::IndexSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

use crate::plan::{HashJoin, IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, Sarg, Semi, TupleField};

pub trait RewriteRule {
    type Plan;
    type Info;

    fn matches(plan: &Self::Plan) -> Option<Self::Info>;
    fn rewrite(plan: Self::Plan, info: Self::Info) -> Self::Plan;
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

    fn rewrite(mut plan: Self::Plan, label: Self::Info) -> Self::Plan {
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
        plan
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
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(_, expr, value)) = plan {
            if let (PhysicalExpr::Field(TupleField { label: var, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value) {
                return match &**input {
                    PhysicalPlan::TableScan(..) => None,
                    input => input
                        .any(&|plan| match plan {
                            PhysicalPlan::TableScan(_, label, _) => label == var,
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
                    PhysicalPlan::TableScan(_, var, _) if var == &relvar => Some(()),
                    _ => None,
                },
            );
        }
        unreachable!()
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
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            return exprs.iter().find_map(|expr| {
                if let PhysicalExpr::BinOp(_, expr, value) = expr {
                    if let (PhysicalExpr::Field(TupleField { label: var, .. }), PhysicalExpr::Value(_)) =
                        (&**expr, &**value)
                    {
                        return match &**input {
                            PhysicalPlan::TableScan(..) => None,
                            input => input
                                .any(&|plan| match plan {
                                    PhysicalPlan::TableScan(_, label, _) => label == var,
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
            let ok = |plan: &PhysicalPlan| match plan {
                PhysicalPlan::TableScan(_, var, _) if var == &relvar => Some(()),
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

/// Match single field equality predicates such as:
///
/// ```sql
/// select * from t where x = 1
/// ```
///
/// Rewrite as an index scan if applicable.
///
/// NOTE: This rule does not consider multi-column indexes.
pub(crate) struct IxScanEq;

pub(crate) struct IxScanInfo {
    index_id: IndexId,
    cols: Vec<(usize, ColId)>,
}

impl RewriteRule for IxScanEq {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, expr, value)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                if let (PhysicalExpr::Field(TupleField { field_pos: pos, .. }), PhysicalExpr::Value(_)) =
                    (&**expr, &**value)
                {
                    return schema.indexes.iter().find_map(
                        |IndexSchema {
                             index_id,
                             index_algorithm,
                             ..
                         }| {
                            index_algorithm
                                .columns()
                                // TODO: Support prefix scans
                                .as_singleton()
                                .filter(|col_id| col_id.idx() == *pos)
                                .map(|col_id| (*index_id, col_id))
                        },
                    );
                }
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (index_id, col_id): Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, _, value)) = plan {
            if let PhysicalPlan::TableScan(schema, var, None) = *input {
                if let PhysicalExpr::Value(v) = *value {
                    return PhysicalPlan::IxScan(
                        IxScan {
                            schema,
                            index_id,
                            prefix: vec![],
                            arg: Sarg::Eq(col_id, v),
                        },
                        var,
                    );
                }
            }
        }
        unreachable!()
    }
}

/// Match multi-field equality predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1
/// ```
///
/// Create an index scan for one of the equality conditions.
///
/// NOTE: This rule does not consider multi-column indexes.
pub(crate) struct IxScanAnd;

impl RewriteRule for IxScanAnd {
    type Plan = PhysicalPlan;
    type Info = (IndexId, usize, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                return exprs.iter().enumerate().find_map(|(i, expr)| {
                    if let PhysicalExpr::BinOp(BinOp::Eq, lhs, value) = expr {
                        if let (PhysicalExpr::Field(TupleField { field_pos: pos, .. }), PhysicalExpr::Value(_)) =
                            (&**lhs, &**value)
                        {
                            return schema.indexes.iter().find_map(
                                |IndexSchema {
                                     index_id,
                                     index_algorithm,
                                     ..
                                 }| {
                                    index_algorithm
                                        .columns()
                                        // TODO: Support prefix scans
                                        .as_singleton()
                                        .filter(|col_id| col_id.idx() == *pos)
                                        .map(|col_id| (*index_id, i, col_id))
                                },
                            );
                        }
                    }
                    None
                });
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, (index_id, i, col_id): Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, mut exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, label, None) = *input {
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

/// Match multi-field equality predicates such as:
///
/// ```sql
/// select * from t where x = 1 and y = 1
/// ```
///
/// Rewrite as a multi-column index scan if applicable.
///
/// NOTE: This rule does not consider indexes on 3 or more columns.
pub(crate) struct IxScanEq2Col;

impl RewriteRule for IxScanEq2Col {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                for (i, a) in exprs.iter().enumerate() {
                    for (j, b) in exprs.iter().enumerate().filter(|(j, _)| i != *j) {
                        if let (PhysicalExpr::BinOp(BinOp::Eq, a, u), PhysicalExpr::BinOp(BinOp::Eq, b, v)) = (a, b) {
                            if let (
                                PhysicalExpr::Field(u),
                                PhysicalExpr::Value(_),
                                PhysicalExpr::Field(v),
                                PhysicalExpr::Value(_),
                            ) = (&**a, &**u, &**b, &**v)
                            {
                                return schema
                                    .indexes
                                    .iter()
                                    .filter(|IndexSchema { index_algorithm, .. }| {
                                        // TODO: Support prefix scans
                                        index_algorithm.columns().len() == 2
                                    })
                                    .find_map(
                                        |IndexSchema {
                                             index_id,
                                             index_algorithm,
                                             ..
                                         }| {
                                            Some(IxScanInfo {
                                                index_id: *index_id,
                                                cols: vec![
                                                    (
                                                        i,
                                                        index_algorithm
                                                            .columns()
                                                            .iter()
                                                            .next()
                                                            .filter(|col_id| col_id.idx() == u.field_pos)?,
                                                    ),
                                                    (
                                                        j,
                                                        index_algorithm
                                                            .columns()
                                                            .iter()
                                                            .nth(1)
                                                            .filter(|col_id| col_id.idx() == v.field_pos)?,
                                                    ),
                                                ],
                                            })
                                        },
                                    );
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> PhysicalPlan {
        match info.cols.as_slice() {
            [(i, a), (j, b)] => {
                if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
                    if let PhysicalPlan::TableScan(schema, label, None) = *input {
                        if let (
                            Some(PhysicalExpr::BinOp(BinOp::Eq, _, u)),
                            Some(PhysicalExpr::BinOp(BinOp::Eq, _, v)),
                        ) = (exprs.get(*i), exprs.get(*j))
                        {
                            if let (PhysicalExpr::Value(u), PhysicalExpr::Value(v)) = (&**u, &**v) {
                                return match exprs.len() {
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
                                        exprs
                                            .into_iter()
                                            .enumerate()
                                            .find(|(pos, _)| pos != i && pos != j)
                                            .map(|(_, expr)| expr)
                                            .unwrap(),
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
                                        PhysicalExpr::LogOp(
                                            LogOp::And,
                                            exprs
                                                .into_iter()
                                                .enumerate()
                                                .filter(|(pos, _)| pos != i && pos != j)
                                                .map(|(_, expr)| expr)
                                                .collect(),
                                        ),
                                    ),
                                };
                            }
                        }
                    }
                }
                unreachable!()
            }
            _ => plan,
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
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _, None) = &**input {
                for (i, a) in exprs.iter().enumerate() {
                    for (j, b) in exprs.iter().enumerate().filter(|(j, _)| i != *j) {
                        for (k, c) in exprs.iter().enumerate().filter(|(k, _)| i != *k && j != *k) {
                            if let (
                                PhysicalExpr::BinOp(BinOp::Eq, a, u),
                                PhysicalExpr::BinOp(BinOp::Eq, b, v),
                                PhysicalExpr::BinOp(BinOp::Eq, c, w),
                            ) = (a, b, c)
                            {
                                if let (
                                    PhysicalExpr::Field(u),
                                    PhysicalExpr::Value(_),
                                    PhysicalExpr::Field(v),
                                    PhysicalExpr::Value(_),
                                    PhysicalExpr::Field(w),
                                    PhysicalExpr::Value(_),
                                ) = (&**a, &**u, &**b, &**v, &**c, &**w)
                                {
                                    return schema
                                        .indexes
                                        .iter()
                                        .filter(|IndexSchema { index_algorithm, .. }| {
                                            // TODO: Support prefix scans
                                            index_algorithm.columns().len() == 3
                                        })
                                        .find_map(
                                            |IndexSchema {
                                                 index_id,
                                                 index_algorithm,
                                                 ..
                                             }| {
                                                Some(IxScanInfo {
                                                    index_id: *index_id,
                                                    cols: vec![
                                                        (
                                                            i,
                                                            index_algorithm
                                                                .columns()
                                                                .iter()
                                                                .next()
                                                                .filter(|col_id| col_id.idx() == u.field_pos)?,
                                                        ),
                                                        (
                                                            j,
                                                            index_algorithm
                                                                .columns()
                                                                .iter()
                                                                .nth(1)
                                                                .filter(|col_id| col_id.idx() == v.field_pos)?,
                                                        ),
                                                        (
                                                            k,
                                                            index_algorithm
                                                                .columns()
                                                                .iter()
                                                                .nth(2)
                                                                .filter(|col_id| col_id.idx() == w.field_pos)?,
                                                        ),
                                                    ],
                                                })
                                            },
                                        );
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn rewrite(plan: PhysicalPlan, info: Self::Info) -> PhysicalPlan {
        match info.cols.as_slice() {
            [(i, a), (j, b), (k, c)] => {
                if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
                    if let PhysicalPlan::TableScan(schema, label, None) = *input {
                        if let (
                            Some(PhysicalExpr::BinOp(BinOp::Eq, _, u)),
                            Some(PhysicalExpr::BinOp(BinOp::Eq, _, v)),
                            Some(PhysicalExpr::BinOp(BinOp::Eq, _, w)),
                        ) = (exprs.get(*i), exprs.get(*j), exprs.get(*k))
                        {
                            if let (PhysicalExpr::Value(u), PhysicalExpr::Value(v), PhysicalExpr::Value(w)) =
                                (&**u, &**v, &**w)
                            {
                                return match exprs.len() {
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
                                        exprs
                                            .into_iter()
                                            .enumerate()
                                            .find(|(pos, _)| pos != i && pos != j && pos != k)
                                            .map(|(_, expr)| expr)
                                            .unwrap(),
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
                                        PhysicalExpr::LogOp(
                                            LogOp::And,
                                            exprs
                                                .into_iter()
                                                .enumerate()
                                                .filter(|(pos, _)| pos != i && pos != j && pos != k)
                                                .map(|(_, expr)| expr)
                                                .collect(),
                                        ),
                                    ),
                                };
                            }
                        }
                    }
                }
                unreachable!()
            }
            _ => plan,
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

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Self::Plan {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    lhs_field: join.rhs_field,
                    rhs_field: join.lhs_field,
                    unique: false,
                },
                Semi::All,
            ),
            _ => plan,
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

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Self::Plan {
        match plan {
            PhysicalPlan::HashJoin(join, Semi::All) => PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: join.rhs,
                    rhs: join.lhs,
                    ..join
                },
                Semi::All,
            ),
            _ => plan,
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

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Self::Plan {
        if let PhysicalPlan::HashJoin(join, semi) = plan {
            if let PhysicalPlan::Filter(rhs, expr) = *join.rhs {
                return PhysicalPlan::Filter(
                    Box::new(PhysicalPlan::HashJoin(
                        HashJoin {
                            lhs: join.lhs,
                            rhs,
                            ..join
                        },
                        semi,
                    )),
                    expr,
                );
            }
        }
        unreachable!()
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

    fn rewrite(plan: PhysicalPlan, (rhs_index, rhs_field): Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::HashJoin(join, semi) = plan {
            if let PhysicalPlan::TableScan(rhs, rhs_label, None) = *join.rhs {
                return PhysicalPlan::IxJoin(
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
                );
            }
        }
        unreachable!()
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

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::IxJoin(IxJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        plan
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

    fn rewrite(mut plan: PhysicalPlan, _: Self::Info) -> PhysicalPlan {
        if let PhysicalPlan::HashJoin(HashJoin { unique, .. }, _) = &mut plan {
            *unique = true;
        }
        plan
    }
}
