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

/// To preserve semantics while reordering of operations,
/// the physical plan assumes tuples with named labels.
/// However positions are computed for them before execution.
pub(crate) struct ComputePositions;

impl RewriteRule for ComputePositions {
    type Plan = PhysicalPlan;
    type Info = ();

    fn matches(plan: &Self::Plan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(_, expr) = plan {
            return expr
                .any(|expr| matches!(expr, PhysicalExpr::Field(TupleField { label_pos: None, .. })))
                .then_some(());
        }
        matches!(
            plan,
            PhysicalPlan::IxJoin(
                IxJoin {
                    lhs_field: TupleField { label_pos: None, .. },
                    ..
                },
                _,
            ) | PhysicalPlan::HashJoin(
                HashJoin {
                    lhs_field: TupleField { label_pos: None, .. },
                    rhs_field: TupleField { label_pos: None, .. },
                    ..
                },
                _,
            )
        )
        .then_some(())
    }

    fn rewrite(plan: Self::Plan, _: Self::Info) -> Self::Plan {
        match plan {
            PhysicalPlan::Filter(input, mut expr) => {
                expr.label_positions(&input);
                PhysicalPlan::Filter(input, expr)
            }
            PhysicalPlan::IxJoin(
                join @ IxJoin {
                    lhs_field:
                        TupleField {
                            label,
                            label_pos: None,
                            field_pos,
                        },
                    ..
                },
                semi,
            ) => PhysicalPlan::IxJoin(
                IxJoin {
                    lhs_field: TupleField {
                        label,
                        label_pos: join.lhs.label_pos(&label),
                        field_pos,
                    },
                    ..join
                },
                semi,
            ),
            PhysicalPlan::HashJoin(
                join @ HashJoin {
                    lhs_field:
                        TupleField {
                            label: lhs_label,
                            label_pos: None,
                            field_pos: lhs_field_pos,
                        },
                    rhs_field:
                        TupleField {
                            label: rhs_label,
                            label_pos: None,
                            field_pos: rhs_field_pos,
                        },
                    ..
                },
                semi,
            ) => PhysicalPlan::HashJoin(
                HashJoin {
                    lhs_field: TupleField {
                        label: lhs_label,
                        label_pos: join.lhs.label_pos(&lhs_label),
                        field_pos: lhs_field_pos,
                    },
                    rhs_field: TupleField {
                        label: rhs_label,
                        label_pos: join.rhs.label_pos(&rhs_label),
                        field_pos: rhs_field_pos,
                    },
                    ..join
                },
                semi,
            ),
            _ => plan,
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
pub(crate) struct PushConstFilter;

impl RewriteRule for PushConstFilter {
    type Plan = PhysicalPlan;
    type Info = Label;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(_, expr, value)) = plan {
            if let (PhysicalExpr::Field(TupleField { label: var, .. }), PhysicalExpr::Value(_)) = (&**expr, &**value) {
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
pub(crate) struct PushConjunction;

impl RewriteRule for PushConjunction {
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

/// Match single field equality predicates such as:
///
/// ```sql
/// select * from t where x = 1
/// ```
///
/// Rewrite as an index scan if applicable.
///
/// NOTE: This rule does not consider multi-column indexes.
pub(crate) struct EqIxScan1;

pub(crate) struct IxScanInfo {
    index_id: IndexId,
    cols: Vec<(usize, ColId)>,
}

impl RewriteRule for EqIxScan1 {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::BinOp(BinOp::Eq, expr, value)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
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
            if let PhysicalPlan::TableScan(schema, var) = *input {
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
pub(crate) struct EqIxScan2;

impl RewriteRule for EqIxScan2 {
    type Plan = PhysicalPlan;
    type Info = (IndexId, usize, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
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
            if let PhysicalPlan::TableScan(schema, label) = *input {
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
pub(crate) struct EqIxScan2Col;

impl RewriteRule for EqIxScan2Col {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
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
                    if let PhysicalPlan::TableScan(schema, label) = *input {
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
pub(crate) struct EqIxScan3Col;

impl RewriteRule for EqIxScan3Col {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::Filter(input, PhysicalExpr::LogOp(LogOp::And, exprs)) = plan {
            if let PhysicalPlan::TableScan(schema, _) = &**input {
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
                    if let PhysicalPlan::TableScan(schema, label) = *input {
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
pub(crate) struct HashToIxJoin;

impl RewriteRule for HashToIxJoin {
    type Plan = PhysicalPlan;
    type Info = (IndexId, ColId);

    fn matches(plan: &PhysicalPlan) -> Option<Self::Info> {
        if let PhysicalPlan::HashJoin(
            HashJoin {
                rhs,
                rhs_field:
                    TupleField {
                        label: rhs_var,
                        field_pos,
                        ..
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
            if let PhysicalPlan::TableScan(rhs, rhs_label) = *join.rhs {
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
