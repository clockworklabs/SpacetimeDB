//! Lowering from the logical plan to the physical plan.

use std::collections::HashMap;

use crate::plan::{IxJoin, NameId, PhysFieldProj, PhysicalCtx, PhysicalExpr, PhysicalPlan, PhysicalProjPlan};
use spacetimedb_expr::expr::{Expr, FieldProject, LeftDeepJoin, Project, RelExpr};
use spacetimedb_expr::statement::Statement;
use spacetimedb_expr::StatementCtx;
use spacetimedb_sql_parser::ast::BinOp;

pub trait IdGen {
    fn gen(&mut self, name: &str) -> NameId;
}

fn compile_expr(expr: Expr, id: &mut impl IdGen) -> PhysicalExpr {
    match expr {
        Expr::BinOp(op, lhs, rhs) => {
            PhysicalExpr::BinOp(op, Box::new(compile_expr(*lhs, id)), Box::new(compile_expr(*rhs, id)))
        }
        Expr::Value(value, _) => PhysicalExpr::Value(value),
        Expr::Field(expr) => PhysicalExpr::Field(compile_field_project(id, expr)),
    }
}

fn compile_project(id: &mut impl IdGen, expr: Project) -> PhysicalProjPlan {
    match expr {
        Project::NoProj(input) => PhysicalProjPlan::NoProj(compile_rel_expr(id, input)),
        Project::RelVar(input, var) => PhysicalProjPlan::Relvar(compile_rel_expr(id, input), id.gen(var.as_ref())),
        Project::Fields(input, exprs) => PhysicalProjPlan::Fields(
            compile_rel_expr(id, input),
            exprs
                .into_iter()
                .map(|(alias, expr)| (alias, compile_field_project(id, expr)))
                .collect(),
        ),
    }
}

fn compile_field_project(id: &mut impl IdGen, expr: FieldProject) -> PhysFieldProj {
    PhysFieldProj(id.gen(&expr.table), expr.field)
}

fn compile_rel_expr(id: &mut impl IdGen, ast: RelExpr) -> PhysicalPlan {
    match ast {
        RelExpr::RelVar(table, var) => PhysicalPlan::TableScan(table, id.gen(var.as_ref())),
        RelExpr::Select(input, expr) => {
            PhysicalPlan::Filter(Box::new(compile_rel_expr(id, *input)), compile_expr(expr, id))
        }
        RelExpr::EqJoin(
            LeftDeepJoin { lhs, rhs, var },
            FieldProject { table: u, field: a, .. },
            FieldProject { table: v, field: b, .. },
        )
        | RelExpr::EqJoin(
            LeftDeepJoin { lhs, rhs, var },
            FieldProject { table: u, field: a, .. },
            FieldProject { table: v, field: b, .. },
        ) if lhs.has_field(&u) && var == v => {
            let u = id.gen(u.as_ref());
            let v = id.gen(v.as_ref());
            let var = id.gen(var.as_ref());
            let lhs = Box::new(compile_rel_expr(id, *lhs));
            if let Some(schema) = rhs.indexes.iter().find(|schema| {
                let cols = schema.index_algorithm.columns();
                cols.len() == 1 && cols.contains(b.into())
            }) {
                return PhysicalPlan::IxJoin(IxJoin {
                    lhs,
                    rhs: rhs.clone(),
                    var,
                    index_id: schema.index_id,
                    index_cols: schema.index_algorithm.columns().clone(),
                    unique: false,
                    index_key_expr: PhysicalExpr::Field(PhysFieldProj(u, a)),
                });
            }
            PhysicalPlan::Filter(
                Box::new(PhysicalPlan::NLJoin(lhs, Box::new(PhysicalPlan::TableScan(rhs, var)))),
                PhysicalExpr::BinOp(
                    BinOp::Eq,
                    Box::new(PhysicalExpr::Field(PhysFieldProj(u, a))),
                    Box::new(PhysicalExpr::Field(PhysFieldProj(v, b))),
                ),
            )
        }
        RelExpr::EqJoin(
            LeftDeepJoin { lhs, rhs, var },
            FieldProject { table: u, field: a, .. },
            FieldProject { table: v, field: b, .. },
        ) => {
            let u = id.gen(u.as_ref());
            let v = id.gen(v.as_ref());
            let var = id.gen(var.as_ref());
            let lhs = Box::new(compile_rel_expr(id, *lhs));
            PhysicalPlan::Filter(
                Box::new(PhysicalPlan::NLJoin(lhs, Box::new(PhysicalPlan::TableScan(rhs, var)))),
                PhysicalExpr::BinOp(
                    BinOp::Eq,
                    Box::new(PhysicalExpr::Field(PhysFieldProj(u, a))),
                    Box::new(PhysicalExpr::Field(PhysFieldProj(v, b))),
                ),
            )
        }
        RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs, var }) => PhysicalPlan::NLJoin(
            Box::new(compile_rel_expr(id, *lhs)),
            Box::new(PhysicalPlan::TableScan(rhs, id.gen(var.as_ref()))),
        ),
    }
}

/// Compile a SQL statement into a physical plan.
///
/// The input [Statement] is assumed to be valid so the lowering is not expected to fail.
///
/// **NOTE:** It does not optimize the plan.
pub fn compile(ast: StatementCtx<'_>) -> PhysicalCtx<'_> {
    struct Interner {
        next: usize,
        names: HashMap<String, usize>,
    }
    impl IdGen for Interner {
        fn gen(&mut self, name: &str) -> NameId {
            if let Some(id) = self.names.get(name) {
                return NameId(*id);
            }
            self.next += 1;
            self.names.insert(name.to_owned(), self.next);
            self.next.into()
        }
    }
    let plan = match ast.statement {
        Statement::Select(expr) => compile_project(
            &mut Interner {
                next: 0,
                names: HashMap::new(),
            },
            expr,
        ),
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
