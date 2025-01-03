use spacetimedb_primitives::{ColId, ColSet, IndexId};
use spacetimedb_schema::schema::IndexSchema;
use spacetimedb_sql_parser::ast::{BinOp, LogOp};

use crate::plan::{HashJoin, IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, Sarg, TupleField};

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

/// Turn an equality predicate into a single column index scan
pub(crate) struct EqToIxScan;

pub(crate) struct IxScanInfo {
    index_id: IndexId,
    col_id: ColId,
}

impl RewriteRule for EqToIxScan {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

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
                                .as_singleton()
                                .filter(|col_id| col_id.idx() == *pos)
                                .map(|col_id| IxScanInfo {
                                    index_id: *index_id,
                                    col_id,
                                })
                        },
                    );
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
pub(crate) struct ConjunctionToIxScan;

impl RewriteRule for ConjunctionToIxScan {
    type Plan = PhysicalPlan;
    type Info = (usize, IxScanInfo);

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
                            );
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
pub(crate) struct HashToIxJoin;

impl RewriteRule for HashToIxJoin {
    type Plan = PhysicalPlan;
    type Info = IxScanInfo;

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
