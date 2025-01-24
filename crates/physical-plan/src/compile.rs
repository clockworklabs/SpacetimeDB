//! Lowering from the logical plan to the physical plan.

use std::collections::HashMap;

use crate::plan::{
    HashJoin, Label, PhysicalCtx, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Semi, TupleField,
};

use spacetimedb_expr::expr::{Expr, FieldProject, LeftDeepJoin, ProjectList, ProjectName, RelExpr, Relvar};
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

fn compile_project_list(var: &mut impl VarLabel, expr: ProjectList) -> ProjectListPlan {
    match expr {
        ProjectList::Name(proj) => ProjectListPlan::Name(compile_project_name(var, proj)),
        ProjectList::List(proj, fields) => ProjectListPlan::List(
            compile_rel_expr(var, proj),
            fields
                .into_iter()
                .map(|(alias, expr)| (alias, compile_field_project(var, expr)))
                .collect(),
        ),
    }
}

fn compile_project_name(var: &mut impl VarLabel, proj: ProjectName) -> ProjectPlan {
    match proj {
        ProjectName::None(input) => ProjectPlan::None(compile_rel_expr(var, input)),
        ProjectName::Some(input, name) => ProjectPlan::Name(compile_rel_expr(var, input), var.label(&name), None),
    }
}

fn compile_field_project(var: &mut impl VarLabel, expr: FieldProject) -> TupleField {
    TupleField {
        label: var.label(&expr.table),
        label_pos: None,
        field_pos: expr.field,
    }
}

fn compile_rel_expr(var: &mut impl VarLabel, ast: RelExpr) -> PhysicalPlan {
    match ast {
        RelExpr::RelVar(Relvar { schema, alias, delta }) => {
            let label = var.label(alias.as_ref());
            PhysicalPlan::TableScan(schema, label, delta)
        }
        RelExpr::Select(input, expr) => {
            let input = compile_rel_expr(var, *input);
            let input = Box::new(input);
            PhysicalPlan::Filter(input, compile_expr(expr, var))
        }
        RelExpr::EqJoin(
            LeftDeepJoin {
                lhs,
                rhs:
                    Relvar {
                        schema: rhs_schema,
                        alias: rhs_alias,
                        delta,
                        ..
                    },
            },
            FieldProject { table: u, field: a, .. },
            FieldProject { table: v, field: b, .. },
        ) => PhysicalPlan::HashJoin(
            HashJoin {
                lhs: Box::new(compile_rel_expr(var, *lhs)),
                rhs: Box::new(PhysicalPlan::TableScan(rhs_schema, var.label(&rhs_alias), delta)),
                lhs_field: TupleField {
                    label: var.label(u.as_ref()),
                    label_pos: None,
                    field_pos: a,
                },
                rhs_field: TupleField {
                    label: var.label(v.as_ref()),
                    label_pos: None,
                    field_pos: b,
                },
                unique: false,
            },
            Semi::All,
        ),
        RelExpr::LeftDeepJoin(LeftDeepJoin {
            lhs,
            rhs:
                Relvar {
                    schema: rhs_schema,
                    alias: rhs_alias,
                    delta,
                    ..
                },
        }) => {
            let lhs = compile_rel_expr(var, *lhs);
            let rhs = PhysicalPlan::TableScan(rhs_schema, var.label(&rhs_alias), delta);
            let lhs = Box::new(lhs);
            let rhs = Box::new(rhs);
            PhysicalPlan::NLJoin(lhs, rhs)
        }
    }
}

/// Compile a logical subscribe expression
pub fn compile_project_plan(project: ProjectName) -> ProjectPlan {
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
    compile_project_name(
        &mut Interner {
            next: 0,
            names: HashMap::new(),
        },
        project,
    )
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
        Statement::Select(expr) => compile_project_list(
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
