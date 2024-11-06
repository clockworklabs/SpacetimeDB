//! Optimizes a physical plan by applying a set of rules, aka a Rule-based optimizer.

use crate::plan::*;
use crate::printer::PrintPlan;
use spacetimedb_expr::ty::TyId;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;
use std::ops::Bound;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum SortOp {
    Filter,
    Project,
    Join,
    TableScan,
    IndexScan,
    // Add other operations as needed
}

fn index_op(op: BinOp, value: AlgebraicValue, ty: TyId) -> IndexOp {
    match op {
        BinOp::Eq => IndexOp::Eq(value, ty),
        BinOp::Ne => IndexOp::Range(Bound::Excluded(value.clone()), Bound::Excluded(value), ty),
        // Exclusive upper bound => field < value
        BinOp::Lt => IndexOp::Range(Bound::Unbounded, Bound::Excluded(value), ty),
        // Inclusive upper bound => field <= value
        BinOp::Lte => IndexOp::Range(Bound::Unbounded, Bound::Included(value), ty),
        // Exclusive lower bound => field > value
        BinOp::Gt => IndexOp::Range(Bound::Excluded(value), Bound::Unbounded, ty),
        // Inclusive lower bound => field >= value
        BinOp::Gte => IndexOp::Range(Bound::Included(value), Bound::Unbounded, ty),
        _ => unreachable!("Invalid operator for index scan: `{}`", op),
    }
}

fn find_idx(
    table_schema: &Arc<TableSchema>,
    index: Option<Index>,
    rhs: &AlgebraicValue,
    op: BinOp,
    id: TyId,
) -> Option<IndexScan> {
    if let Some(idx) = index {
        let index_id = idx.index.index_id;
        let op = index_op(op, rhs.clone(), id);

        Some(IndexScan {
            table_schema: table_schema.clone(),
            index_id,
            unique: idx.is_unique,
            prefix: vec![(idx.col, rhs.clone())],
            col: idx.col,
            op,
        })
    } else {
        None
    }
}

enum FieldsOp<'a> {
    Lhs(usize, &'a AlgebraicValue, TyId),
    Rhs(usize, &'a AlgebraicValue, TyId),
    Expr {
        lhs: &'a PhysicalExpr,
        rhs: &'a PhysicalExpr,
    },
}

fn extract_fields_op<'a>(lhs: &'a PhysicalExpr, rhs: &'a PhysicalExpr) -> FieldsOp<'a> {
    match (lhs, rhs) {
        (PhysicalExpr::Field(_, pos, ty), PhysicalExpr::Value(rhs, _)) => FieldsOp::Lhs(*pos, rhs, *ty),
        (PhysicalExpr::Value(lhs, _), PhysicalExpr::Field(_, pos, ty)) => FieldsOp::Rhs(*pos, lhs, *ty),
        (lhs, rhs) => FieldsOp::Expr { lhs, rhs },
    }
}

// TODO: Support for multi-column indexes
/// Search for an index to use in a filter.
///
/// Support for `column` [BinOp] `value` | `value` [BinOp] `column`.
pub(crate) fn extract_idx(plan: &PhysicalPlan, op: BinOp, lhs: &PhysicalExpr, rhs: &PhysicalExpr) -> Option<IndexScan> {
    match extract_fields_op(lhs, rhs) {
        FieldsOp::Lhs(pos, rhs, ty) => {
            let (table_schema, index) = plan.table_schema().index_lhs(pos);
            find_idx(table_schema, index, rhs, op, ty)
        }
        FieldsOp::Rhs(pos, lhs, ty) => {
            let (table_schema, index) = plan.table_schema().index_rhs(pos.into());
            find_idx(table_schema, index, lhs, op, ty)
        }
        _ => None,
    }
}

fn optimize_filter(filter: Filter, source: StatementSource) -> PhysicalPlan {
    let plan = _optimize_plan(*filter.input, source);
    match filter.op.clone() {
        PhysicalExpr::BinOp(op, lhs, rhs) => {
            if let Some(idx) = extract_idx(&plan, op, &lhs, &rhs) {
                PhysicalPlan::IndexScan(idx)
            } else {
                PhysicalPlan::Filter(Filter {
                    input: Box::new(plan),
                    op: PhysicalExpr::BinOp(op, lhs, rhs),
                })
            }
        }
        expr => PhysicalPlan::Filter(Filter {
            input: Box::new(plan),
            op: expr,
        }),
    }
}

/// Optimize a project plan.
///
/// **NOTE:** For subscriptions, is not correct to apply a *Projection Push-Down* optimization, because
/// we should always return the full row to the subscriber.
fn optimize_project_sub(project: Project, source: StatementSource) -> PhysicalPlan {
    let plan = _optimize_plan(*project.input, source);
    PhysicalPlan::Project(Project {
        input: Box::new(plan),
        op: project.op,
    })
}

/// Optimize a project plan.
fn optimize_project_query(project: Project, source: StatementSource) -> PhysicalPlan {
    // TODO: Apply projection push-down
    optimize_project_sub(project, source)
}

fn optimize_cross_join(join: CrossJoin, source: StatementSource) -> PhysicalPlan {
    let left = _optimize_plan(*join.lhs, source);
    let right = _optimize_plan(*join.rhs, source);

    match (left, right) {
        // Is a pure cross join?
        (left @ PhysicalPlan::TableScan(_, _), right @ PhysicalPlan::TableScan(_, _)) => {
            PhysicalPlan::CrossJoin(CrossJoin {
                lhs: Box::new(left),
                rhs: Box::new(right),
                ty: join.ty,
            })
        }
        (left, right) => PhysicalPlan::CrossJoin(CrossJoin {
            lhs: Box::new(left),
            rhs: Box::new(right),
            ty: join.ty,
        }),
    }
}

fn optimize_query(plan: PhysicalPlan, source: StatementSource) -> PhysicalPlan {
    match plan {
        PhysicalPlan::Filter(filter) => optimize_filter(filter, source),
        PhysicalPlan::Project(project) => optimize_project_query(project, source),
        PhysicalPlan::CrossJoin(join) => optimize_cross_join(join, source),
        // Assumed to be already optimized
        PhysicalPlan::TableScan(_, _)
        | PhysicalPlan::IndexScan(_)
        | PhysicalPlan::IndexJoin(_)
        | PhysicalPlan::IndexSemiJoin(_) => plan,
    }
}

fn optimize_subscription(plan: PhysicalPlan, source: StatementSource) -> PhysicalPlan {
    match plan {
        PhysicalPlan::Project(project) => optimize_project_sub(project, source),
        _ => optimize_query(plan, source),
    }
}

fn _optimize_plan(plan: PhysicalPlan, source: StatementSource) -> PhysicalPlan {
    match source {
        StatementSource::Subscription => optimize_subscription(plan, source),
        StatementSource::Query => optimize_query(plan, source),
    }
}

/// Utility function to apply filters to a sub-plan.
fn apply_filters(plan: PhysicalPlan, filters: Vec<PhysicalExpr>) -> PhysicalPlan {
    filters.into_iter().fold(plan, |acc, filter| {
        PhysicalPlan::Filter(Filter {
            input: Box::new(acc),
            op: filter,
        })
    })
}

struct Split {
    lhs: Vec<PhysicalExpr>,
    rhs: Vec<PhysicalExpr>,
    pending: Vec<(BinOp, PhysicalExpr, PhysicalExpr)>,
}

/// Utility function to split filters based on referenced columns in a CrossJoin.
fn split_filters_by_relation(filter: &PhysicalExpr, join: &CrossJoin) -> Split {
    let mut lhs = Vec::with_capacity(1);
    let mut rhs = Vec::with_capacity(1);
    let mut pending = Vec::new();
    match filter {
        PhysicalExpr::BinOp(BinOp::And | BinOp::Or, lhs_expr, rhs_expr) => {
            let split = split_filters_by_relation(lhs_expr, join);
            lhs.extend(split.lhs);
            rhs.extend(split.rhs);

            let split = split_filters_by_relation(rhs_expr, join);
            lhs.extend(split.lhs);
            rhs.extend(split.rhs);
        }
        PhysicalExpr::BinOp(op, lhs_expr, rhs_expr) => match extract_fields_op(lhs_expr, rhs_expr) {
            FieldsOp::Lhs(pos, _, _) => {
                if join.lhs.table_schema().column_lhs(pos).is_some() {
                    lhs.push(filter.clone());
                }
            }
            FieldsOp::Rhs(pos, _, _) => {
                if join.rhs.table_schema().column_rhs(pos).is_some() {
                    rhs.push(filter.clone());
                }
            }

            FieldsOp::Expr { lhs, rhs } => {
                pending.push((*op, lhs.clone(), rhs.clone()));
            }
        },
        _ => {}
    }

    Split { lhs, rhs, pending }
}

fn try_filter_push_down(plan: PhysicalPlan, filter: Filter, source: StatementSource) -> PhysicalPlan {
    dbg!("try_filter_push_down");
    PrintPlan::new(&plan).print();
    match plan {
        PhysicalPlan::CrossJoin(join) => {
            let split = split_filters_by_relation(&filter.op, &join);
            let lhs = filter_push_down(*join.lhs, source);
            let rhs = filter_push_down(*join.rhs, source);

            let (lhs, rhs) = if split.lhs.is_empty() && split.rhs.is_empty() {
                (lhs, rhs)
            } else {
                // Apply filters to the sub-plans
                let lhs = apply_filters(lhs, split.lhs);
                let rhs = apply_filters(rhs, split.rhs);
                (lhs, rhs)
            };

            let mut plan = PhysicalPlan::CrossJoin(CrossJoin {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                ty: join.ty,
            });

            for (op, lhs, rhs) in split.pending {
                plan = PhysicalPlan::Filter(Filter {
                    input: Box::new(plan),
                    op: PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs)),
                });
            }
            dbg!("try_filter_push_down 2");
            PrintPlan::new(&plan).print();
            plan
        }
        PhysicalPlan::Filter(inner_filter) => {
            // Merge both filters
            let plan = inner_filter.input.clone();
            let filter = PhysicalExpr::BinOp(BinOp::And, Box::new(inner_filter.op), Box::new(filter.op));

            filter_push_down(
                PhysicalPlan::Filter(Filter {
                    input: plan,
                    op: filter,
                }),
                source,
            )
        }
        _ => PhysicalPlan::Filter(filter),
    }
}

/// Pushing filters below joins. In the logical plan, WHERE conditions follow joins and will need to be pushed below the join in order to perform index selection.
fn filter_push_down(plan: PhysicalPlan, source: StatementSource) -> PhysicalPlan {
    match plan {
        PhysicalPlan::Filter(filter) => {
            let plan = filter_push_down(*filter.input.clone(), source);
            try_filter_push_down(plan, filter, source)
        }
        PhysicalPlan::CrossJoin(join) => {
            let lhs = filter_push_down(*join.lhs, source);
            let rhs = filter_push_down(*join.rhs, source);
            PhysicalPlan::CrossJoin(CrossJoin {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                ty: join.ty,
            })
        }
        PhysicalPlan::TableScan(_, _)
        | PhysicalPlan::IndexScan(_)
        | PhysicalPlan::IndexJoin(_)
        | PhysicalPlan::IndexSemiJoin(_)
        | PhysicalPlan::Project(_) => plan,
    }
}

/// Apply a set of rules to optimize a physical plan:
///
/// * Filter push-down
/// * Projection pull-up
/// * Join reordering
fn sort_plan(plan: PhysicalPlan, source: StatementSource) -> PhysicalPlan {
    match plan {
        PhysicalPlan::Filter(_) => filter_push_down(plan, source),
        PhysicalPlan::Project(project) =>{
            let plan = filter_push_down(*project.input, source);
            PhysicalPlan::Project(Project {
                input: Box::new(plan),
                op: project.op,
            })
        }
        PhysicalPlan::CrossJoin(_) |
        // Assumed to be already optimized
        PhysicalPlan::TableScan(_,_)
        | PhysicalPlan::IndexScan(_)
        | PhysicalPlan::IndexJoin(_)
        | PhysicalPlan::IndexSemiJoin(_) => plan,
    }
}

/// Optimize the physical plan.
pub fn optimize_plan(plan: PhysicalCtx) -> PhysicalCtx {
    let (plan, sql, source) = plan.into_parts();

    let plan = _optimize_plan(sort_plan(plan, source), source);
    PhysicalCtx { plan, sql, source }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;
    use crate::compile::tests::{compile_sql_stmt_test, compile_sql_sub_test};
    use spacetimedb_lib::error::ResultTest;

    #[test]
    fn select_star() -> ResultTest<()> {
        let plan = compile(compile_sql_stmt_test("SELECT t.* FROM t  WHERE t.u32=1")?);
        plan.print_plan();
        let plan = optimize_plan(plan);
        //dbg!(&plan.plan);
        plan.print_plan();
        // let plan = plan.plan.as_table_scan().unwrap();
        //
        // assert!(matches!(plan.table_name.as_ref(), "t"));
        Ok(())
    }

    #[test]
    fn filter_no_index() -> ResultTest<()> {
        let plan = compile(compile_sql_sub_test("SELECT no_index.* FROM no_index WHERE u32 = 1")?);

        let plan = optimize_plan(plan);
        let project = plan.plan.as_project().unwrap();

        assert!(matches!(
            project.input.as_filter().unwrap().op,
            PhysicalExpr::BinOp(_, _, _)
        ));
        Ok(())
    }

    #[test]
    fn filter_use_index() -> ResultTest<()> {
        let plan = compile(compile_sql_sub_test("SELECT t.* FROM t WHERE u32 = 1")?);

        let plan = optimize_plan(plan);
        let project = plan.plan.as_project().unwrap();
        assert!(matches!(project.input.as_index_scan().unwrap().op, IndexOp::Eq(_, _)));

        let plan = compile(compile_sql_sub_test("SELECT t.* FROM t WHERE 1 = u32")?);

        let plan = optimize_plan(plan);
        let project = plan.plan.as_project().unwrap();
        assert!(matches!(project.input.as_index_scan().unwrap().op, IndexOp::Eq(_, _)));

        Ok(())
    }

    #[test]
    fn cross_join() -> ResultTest<()> {
        let plan = compile(compile_sql_stmt_test("SELECT t.* FROM t CROSS JOIN u")?);

        let plan = optimize_plan(plan).plan;
        let plan = plan.as_project().unwrap();

        let cross = plan.input.as_cross().unwrap();
        assert!(matches!(cross.lhs.as_table_scan(), Some(_)));
        assert!(matches!(cross.rhs.as_table_scan(), Some(_)));

        Ok(())
    }

    #[test]
    fn index_join() -> ResultTest<()> {
        let plan = compile(compile_sql_stmt_test(
            "SELECT t.* FROM t JOIN u ON t.u32 = u.u32 WHERE t.u32>1",
        )?);
        plan.print_plan();
        let plan = optimize_plan(plan);
        plan.print_plan();
        // let plan = plan.plan.as_project().unwrap();

        //dbg!(&plan);

        Ok(())
    }
}
