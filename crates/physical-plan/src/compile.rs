//! Lowering from the logical plan to the physical plan.

use crate::plan::{PhysicalCtx, PhysicalExpr, PhysicalPlan};
use spacetimedb_expr::expr::{Expr, Let, LetCtx, Project, RelExpr, Select};
use spacetimedb_expr::statement::Statement;
use spacetimedb_expr::ty::{TyCtx, Type};
use spacetimedb_expr::StatementCtx;
use spacetimedb_sql_parser::ast::BinOp;

fn compile_expr(ctx: &TyCtx, vars: &LetCtx, expr: Expr) -> PhysicalExpr {
    match expr {
        Expr::Bin(op, lhs, rhs) => {
            let lhs = compile_expr(ctx, vars, *lhs);
            let rhs = compile_expr(ctx, vars, *rhs);
            PhysicalExpr::BinOp(op, Box::new(lhs), Box::new(rhs))
        }
        Expr::Var(sym, _ty) => {
            let var = vars.get_var(sym).cloned().unwrap();
            compile_expr(ctx, vars, var)
        }
        Expr::Row(row, _) => {
            PhysicalExpr::Tuple(
                row.into_vec()
                    .into_iter()
                    // The `sym` is inline in `expr`
                    .map(|(_sym, expr)| compile_expr(ctx, vars, expr))
                    .collect(),
            )
        }
        Expr::Lit(value, _) => PhysicalExpr::Value(value),
        Expr::Field(expr, pos, _) => {
            let expr = compile_expr(ctx, vars, *expr);
            PhysicalExpr::Field(Box::new(expr), pos)
        }
        Expr::Input(ty) if matches!(*ctx.try_resolve(ty).unwrap(), Type::Var(..)) => PhysicalExpr::Ptr,
        Expr::Input(_) => PhysicalExpr::Tup,
    }
}

fn join_exprs(exprs: Vec<PhysicalExpr>) -> Option<PhysicalExpr> {
    exprs
        .into_iter()
        .reduce(|lhs, rhs| PhysicalExpr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs)))
}

fn compile_let(ctx: &TyCtx, Let { vars, exprs }: Let) -> Vec<PhysicalExpr> {
    exprs
        .into_iter()
        .map(|expr| compile_expr(ctx, &LetCtx { vars: &vars }, expr))
        .collect()
}

fn compile_filter(ctx: &TyCtx, select: Select) -> PhysicalPlan {
    let input = compile_rel_expr(ctx, select.input);
    if let Some(op) = join_exprs(compile_let(ctx, select.expr)) {
        PhysicalPlan::Filter(Box::new(input), op)
    } else {
        input
    }
}

fn compile_project(ctx: &TyCtx, expr: Project) -> PhysicalPlan {
    let input = Box::new(compile_rel_expr(ctx, expr.input));
    let op = join_exprs(compile_let(ctx, expr.expr)).unwrap();

    PhysicalPlan::Project(input, op)
}

fn compile_cross_joins(ctx: &TyCtx, joins: Vec<RelExpr>) -> PhysicalPlan {
    joins
        .into_iter()
        .map(|expr| compile_rel_expr(ctx, expr))
        .reduce(|lhs, rhs| PhysicalPlan::NLJoin(Box::new(lhs), Box::new(rhs)))
        .unwrap()
}

fn compile_rel_expr(ctx: &TyCtx, ast: RelExpr) -> PhysicalPlan {
    match ast {
        RelExpr::RelVar(table, _ty) => PhysicalPlan::TableScan(table),
        RelExpr::Select(select) => compile_filter(ctx, *select),
        RelExpr::Proj(proj) => compile_project(ctx, *proj),
        RelExpr::Join(joins, _) => compile_cross_joins(ctx, joins.into_vec()),
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
pub fn compile<'a>(ctx: &TyCtx, ast: StatementCtx<'a>) -> PhysicalCtx<'a> {
    let plan = match ast.statement {
        Statement::Select(expr) => compile_rel_expr(ctx, expr),
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

    fn compile_sql_sub_test(sql: &str) -> ResultTest<(StatementCtx, TyCtx)> {
        let tx = SchemaViewer(module_def());
        let mut ctx = TyCtx::default();
        let expr = compile_sql_sub(&mut ctx, sql, &tx)?;
        Ok((expr, ctx))
    }

    fn compile_sql_stmt_test(sql: &str) -> ResultTest<StatementCtx> {
        let tx = SchemaViewer(module_def());
        let statement = compile_sql_stmt(sql, &tx)?;
        Ok(statement)
    }

    impl PhysicalPlan {
        pub fn as_project(&self) -> Option<(&PhysicalPlan, &PhysicalExpr)> {
            if let PhysicalPlan::Project(input, expr) = self {
                Some((input, expr))
            } else {
                None
            }
        }

        pub fn as_filter(&self) -> Option<(&PhysicalPlan, &PhysicalExpr)> {
            if let PhysicalPlan::Filter(input, expr) = self {
                Some((input, expr))
            } else {
                None
            }
        }

        pub fn as_nljoin(&self) -> Option<(&PhysicalPlan, &PhysicalPlan)> {
            if let PhysicalPlan::NLJoin(lhs, rhs) = self {
                Some((lhs, rhs))
            } else {
                None
            }
        }
    }

    #[test]
    fn test_project() -> ResultTest<()> {
        let (ast, ctx) = compile_sql_sub_test("SELECT * FROM t")?;
        assert!(matches!(compile(&ctx, ast).plan, PhysicalPlan::TableScan(_)));

        let ast = compile_sql_stmt_test("SELECT u32 FROM t")?;
        assert!(matches!(compile(&ctx, ast).plan, PhysicalPlan::Project(..)));

        Ok(())
    }

    #[test]
    fn test_select() -> ResultTest<()> {
        let (ast, ctx) = compile_sql_sub_test("SELECT * FROM t WHERE u32 = 1")?;
        assert!(matches!(compile(&ctx, ast).plan, PhysicalPlan::Filter(..)));

        let (ast, ctx) = compile_sql_sub_test("SELECT * FROM t WHERE u32 = 1 AND f32 = f32")?;
        assert!(matches!(compile(&ctx, ast).plan, PhysicalPlan::Filter(..)));
        Ok(())
    }

    #[test]
    fn test_joins() -> ResultTest<()> {
        // Check we can do a cross join
        let (ast, ctx) = compile_sql_sub_test("SELECT t.* FROM t JOIN u")?;
        let ast = compile(&ctx, ast).plan;
        let (input, op) = ast.as_project().unwrap();
        let (lhs, rhs) = input.as_nljoin().unwrap();

        assert!(matches!(op, PhysicalExpr::Field(..)));
        assert!(matches!(lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(rhs, PhysicalPlan::TableScan(_)));

        // Check we can do multiple joins
        let (ast, ctx) = compile_sql_sub_test("SELECT t.* FROM t JOIN u JOIN x")?;
        let ast = compile(&ctx, ast).plan;
        let (input, _) = ast.as_project().unwrap();
        let (lhs, rhs) = input.as_nljoin().unwrap();
        assert!(matches!(rhs, PhysicalPlan::TableScan(_)));

        let (lhs, rhs) = lhs.as_nljoin().unwrap();
        assert!(matches!(lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(rhs, PhysicalPlan::TableScan(_)));

        // Check we can do a join with a filter
        let (ast, ctx) = compile_sql_sub_test("SELECT t.* FROM t JOIN u ON t.u32 = u.u32")?;
        let ast = compile(&ctx, ast).plan;

        let (input, _) = ast.as_project().unwrap();
        let (input, op) = input.as_filter().unwrap();
        assert!(matches!(op, PhysicalExpr::BinOp(_, _, _)));

        let (lhs, rhs) = input.as_nljoin().unwrap();
        assert!(matches!(lhs, PhysicalPlan::TableScan(_)));
        assert!(matches!(rhs, PhysicalPlan::TableScan(_)));

        Ok(())
    }
}
