//! Lowering from the logical plan to the physical plan.

use std::collections::HashMap;

use crate::plan::{HashJoin, Label, PhysicalCtx, PhysicalExpr, PhysicalPlan, PhysicalProject, ProjectField, Semi};
use spacetimedb_expr::expr::{Expr, FieldProject, LeftDeepJoin, Project, RelExpr};
use spacetimedb_expr::statement::Statement;
use spacetimedb_expr::StatementCtx;

pub trait VarLabel {
    fn label(&mut self, name: &str) -> Label;
}

fn compile_expr(expr: Expr, var: &mut impl VarLabel) -> PhysicalExpr {
    match expr {
        Expr::LogOp(op, a, b) => PhysicalExpr::LogOp(op, vec![compile_expr(*a, var), compile_expr(*b, var)]),
        Expr::BinOp(op, a, b) => {
            let a = Box::new(compile_expr(*a, var));
            let b = Box::new(compile_expr(*b, var));
            PhysicalExpr::BinOp(op, a, b)
        }
        Expr::Value(v, _) => PhysicalExpr::Value(v),
        Expr::Field(proj) => PhysicalExpr::Field(compile_field_project(var, proj)),
    }
}

fn compile_project(var: &mut impl VarLabel, expr: Project) -> PhysicalProject {
    match expr {
        Project::None(input) => PhysicalProject::None(compile_rel_expr(var, input)),
        Project::Relvar(input, name) => PhysicalProject::Relvar(compile_rel_expr(var, input), var.label(&name)),
        Project::Fields(input, exprs) => PhysicalProject::Fields(
            compile_rel_expr(var, input),
            exprs
                .into_iter()
                .map(|(alias, expr)| (alias, compile_field_project(var, expr)))
                .collect(),
        ),
    }
}

fn compile_field_project(var: &mut impl VarLabel, expr: FieldProject) -> ProjectField {
    ProjectField {
        var: var.label(&expr.table),
        pos: expr.field,
    }
}

fn compile_rel_expr(var: &mut impl VarLabel, ast: RelExpr) -> PhysicalPlan {
    match ast {
        RelExpr::RelVar(table, name) => {
            let label = var.label(name.as_ref());
            PhysicalPlan::TableScan(table, label)
        }
        RelExpr::Select(input, expr) => {
            let input = compile_rel_expr(var, *input);
            let input = Box::new(input);
            PhysicalPlan::Filter(input, compile_expr(expr, var))
        }
        RelExpr::EqJoin(join, FieldProject { table: u, field: a, .. }, FieldProject { table: v, field: b, .. }) => {
            PhysicalPlan::HashJoin(
                HashJoin {
                    lhs: Box::new(compile_rel_expr(var, *join.lhs)),
                    rhs: Box::new(PhysicalPlan::TableScan(join.rhs, var.label(&join.var))),
                    lhs_field: ProjectField {
                        var: var.label(u.as_ref()),
                        pos: a,
                    },
                    rhs_field: ProjectField {
                        var: var.label(v.as_ref()),
                        pos: b,
                    },
                    unique: false,
                },
                Semi::All,
            )
        }
        RelExpr::LeftDeepJoin(LeftDeepJoin {
            lhs,
            rhs,
            var: rhs_name,
        }) => {
            let lhs = compile_rel_expr(var, *lhs);
            let rhs = PhysicalPlan::TableScan(rhs, var.label(rhs_name.as_ref()));
            let lhs = Box::new(lhs);
            let rhs = Box::new(rhs);
            PhysicalPlan::NLJoin(lhs, rhs)
        }
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
    impl VarLabel for Interner {
        fn label(&mut self, name: &str) -> Label {
            if let Some(id) = self.names.get(name) {
                return Label(*id);
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
