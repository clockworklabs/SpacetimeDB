//! Lowering from the logical plan to the physical plan.

use crate::plan;
use crate::plan::{CrossJoin, Filter, PhysicalCtx, PhysicalExpr, PhysicalPlan};
use spacetimedb_expr::expr::{Expr, Let, LetCtx, Project, RelExpr, Select};
use spacetimedb_expr::statement::Statement;
use spacetimedb_expr::ty::TyId;
use spacetimedb_expr::StatementCtx;
use spacetimedb_sql_parser::ast::BinOp;

fn compile_expr(ctx: &LetCtx, expr: Expr) -> PhysicalExpr {
    match expr {
        Expr::Bin(op, lhs, rhs) => {
            let lhs = compile_expr(ctx, *lhs);
            let rhs = compile_expr(ctx, *rhs);
            PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs))
        }
        Expr::Var(sym, _ty) => {
            let var = ctx.get_var(sym).cloned().unwrap();
            compile_expr(ctx, var)
        }
        Expr::Row(row, ty) => {
            PhysicalExpr::Tuple(
                row.into_vec()
                    .into_iter()
                    // The `sym` is inline in `expr`
                    .map(|(_sym, expr)| compile_expr(ctx, expr))
                    .collect(),
                ty,
            )
        }
        Expr::Lit(value, ty) => PhysicalExpr::Value(value, ty),
        Expr::Field(expr, pos, ty) => {
            let expr = compile_expr(ctx, *expr);
            PhysicalExpr::Field(Box::new(expr), pos, ty)
        }
        Expr::Input(ty) => PhysicalExpr::Input(ty),
    }
}

fn join_exprs(exprs: Vec<PhysicalExpr>) -> Option<PhysicalExpr> {
    exprs
        .into_iter()
        .reduce(|lhs, rhs| PhysicalExpr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs)))
}

fn compile_let(expr: Let) -> Vec<PhysicalExpr> {
    let ctx = LetCtx { vars: &expr.vars };

    expr.exprs.into_iter().map(|expr| compile_expr(&ctx, expr)).collect()
}

fn compile_filter(select: Select) -> PhysicalPlan {
    let input = compile_rel_expr(select.input);
    if let Some(op) = join_exprs(compile_let(select.expr)) {
        PhysicalPlan::Filter(Filter {
            input: Box::new(input),
            op,
        })
    } else {
        input
    }
}

fn compile_project(expr: Project) -> PhysicalPlan {
    let proj = plan::Project {
        input: Box::new(compile_rel_expr(expr.input)),
        op: join_exprs(compile_let(expr.expr)).unwrap(),
    };

    PhysicalPlan::Project(proj)
}

fn compile_cross_joins(joins: Vec<RelExpr>, ty: TyId) -> PhysicalPlan {
    joins
        .into_iter()
        .map(compile_rel_expr)
        .reduce(|lhs, rhs| {
            PhysicalPlan::CrossJoin(CrossJoin {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                ty,
            })
        })
        .unwrap()
}

fn compile_rel_expr(ast: RelExpr) -> PhysicalPlan {
    match ast {
        RelExpr::RelVar(table, _ty) => PhysicalPlan::TableScan(table),
        RelExpr::Select(select) => compile_filter(*select),
        RelExpr::Proj(proj) => compile_project(*proj),
        RelExpr::Join(joins, ty) => compile_cross_joins(joins.into_vec(), ty),
        RelExpr::Union(_, _) | RelExpr::Minus(_, _) | RelExpr::Dedup(_) => {
            unreachable!("DISTINCT is not implemented")
        }
    }
}

/// Compile a SQL statement into a physical plan.
///
/// The input [Statement] is assumed to be valid so the lowering is not expected to fail.
///
/// **NOTE:** It does not optimize the plan.
pub fn compile(ast: StatementCtx) -> PhysicalCtx {
    let plan = match ast.statement {
        Statement::Select(expr) => compile_rel_expr(expr),
        _ => {
            unreachable!("Only `SELECT` is implemented")
        }
    };

    PhysicalCtx {
        plan,
        sql: ast.sql,
        source: ast.source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_expr::check::compile_sql_sub;
    use spacetimedb_expr::check::test_utils::{build_module_def, SchemaViewer};
    use spacetimedb_expr::statement::compile_sql_stmt;
    use spacetimedb_expr::ty::TyCtx;
    use spacetimedb_expr::StatementCtx;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::{AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    fn module_def() -> ModuleDef {
        build_module_def(vec![
            (
                "t",
                ProductType::from([
                    ("u32", AlgebraicType::U32),
                    ("f32", AlgebraicType::F32),
                    ("str", AlgebraicType::String),
                ]),
            ),
            (
                "u",
                ProductType::from([
                    ("u32", AlgebraicType::U32),
                    ("f32", AlgebraicType::F32),
                    ("str", AlgebraicType::String),
                ]),
            ),
            ("x", ProductType::from([("u32", AlgebraicType::U32)])),
        ])
    }

    fn compile_sql_sub_test(sql: &str) -> ResultTest<StatementCtx> {
        let tx = SchemaViewer(module_def());
        let expr = compile_sql_sub(&mut TyCtx::default(), sql, &tx)?;
        Ok(expr)
    }

    fn compile_sql_stmt_test(sql: &str) -> ResultTest<StatementCtx> {
        let tx = SchemaViewer(module_def());
        let statement = compile_sql_stmt(sql, &tx)?;
        Ok(statement)
    }

    #[test]
    fn test_project() -> ResultTest<()> {
        let ast = compile_sql_sub_test("SELECT * FROM t")?;
        assert!(matches!(compile(ast).plan, PhysicalPlan::TableScan(_)));

        let ast = compile_sql_stmt_test("SELECT u32 FROM t")?;
        assert!(matches!(compile(ast).plan, PhysicalPlan::Project(_)));

        Ok(())
    }

    #[test]
    fn test_select() -> ResultTest<()> {
        let ast = compile_sql_sub_test("SELECT * FROM t WHERE u32 = 1")?;
        assert!(matches!(compile(ast).plan, PhysicalPlan::Filter(_)));

        let ast = compile_sql_sub_test("SELECT * FROM t WHERE u32 = 1 AND f32 = f32")?;
        assert!(matches!(compile(ast).plan, PhysicalPlan::Filter(_)));
        Ok(())
    }

    #[test]
    fn test_joins() -> ResultTest<()> {
        // Check we can do a cross join
        let ast = compile(compile_sql_sub_test("SELECT t.* FROM t JOIN u")?).plan;
        let plan::Project { input, op } = ast.as_project().unwrap();
        let CrossJoin { lhs, rhs, ty: _ } = input.as_cross().unwrap();

        assert!(matches!(op, PhysicalExpr::Field(_, _, _)));
        assert!(matches!(&**lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(&**rhs, PhysicalPlan::TableScan(_)));

        // Check we can do multiple joins
        let ast = compile(compile_sql_sub_test("SELECT t.* FROM t JOIN u JOIN x")?).plan;
        let plan::Project { input, op: _ } = ast.as_project().unwrap();
        let CrossJoin { lhs, rhs, ty: _ } = input.as_cross().unwrap();
        assert!(matches!(&**rhs, PhysicalPlan::TableScan(_)));

        let CrossJoin { lhs, rhs, ty: _ } = lhs.as_cross().unwrap();
        assert!(matches!(&**lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(&**rhs, PhysicalPlan::TableScan(_)));

        // Check we can do a join with a filter
        let ast = compile(compile_sql_stmt_test("SELECT t.* FROM t JOIN u ON t.u32 = u.u32")?).plan;

        let plan::Project { input, op: _ } = ast.as_project().unwrap();
        let Filter { input, op } = input.as_filter().unwrap();
        assert!(matches!(op, PhysicalExpr::BinOp(_, _, _)));

        let CrossJoin { lhs, rhs, ty: _ } = input.as_cross().unwrap();
        assert!(matches!(&**lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(&**rhs, PhysicalPlan::TableScan(_)));

        Ok(())
    }
}
