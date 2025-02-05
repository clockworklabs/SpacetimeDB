//! Lowering from the logical plan to the physical plan.

use std::collections::HashMap;

use crate::dml::{DeletePlan, MutationPlan, UpdatePlan};
use crate::plan::{
    HashJoin, Label, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Semi, TableScan, TupleField,
};

use crate::{PhysicalCtx, PlanCtx};
use spacetimedb_expr::expr::{Expr, FieldProject, LeftDeepJoin, ProjectList, ProjectName, RelExpr, Relvar};
use spacetimedb_expr::statement::{Statement, DML};
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
        ProjectList::Name(proj) => {
            ProjectListPlan::Name(proj.into_iter().map(|proj| compile_project_name(var, proj)).collect())
        }
        ProjectList::Limit(input, n) => ProjectListPlan::Limit(Box::new(compile_project_list(var, *input)), n),
        ProjectList::Agg(expr, agg, ..) => {
            ProjectListPlan::Agg(expr.into_iter().map(|expr| compile_rel_expr(var, expr)).collect(), agg)
        }
        ProjectList::List(proj, fields) => ProjectListPlan::List(
            proj.into_iter().map(|proj| compile_rel_expr(var, proj)).collect(),
            fields
                .into_iter()
                .map(|(_, expr)| compile_field_project(var, expr))
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
            PhysicalPlan::TableScan(
                TableScan {
                    schema,
                    limit: None,
                    delta,
                },
                label,
            )
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
                rhs: Box::new(PhysicalPlan::TableScan(
                    TableScan {
                        schema: rhs_schema,
                        limit: None,
                        delta,
                    },
                    var.label(&rhs_alias),
                )),
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
            let rhs = PhysicalPlan::TableScan(
                TableScan {
                    schema: rhs_schema,
                    limit: None,
                    delta,
                },
                var.label(&rhs_alias),
            );
            let lhs = Box::new(lhs);
            let rhs = Box::new(rhs);
            PhysicalPlan::NLJoin(lhs, rhs)
        }
    }
}

/// Generates unique ids for named entities in a query plan
#[derive(Default)]
struct NamesToIds {
    next_id: usize,
    map: HashMap<String, usize>,
}

impl NamesToIds {
    fn into_map(self) -> HashMap<String, usize> {
        self.map
    }
}

impl VarLabel for NamesToIds {
    fn label(&mut self, name: &str) -> Label {
        if let Some(id) = self.map.get(name) {
            return Label(*id);
        }
        self.next_id += 1;
        self.map.insert(name.to_owned(), self.next_id);
        self.next_id.into()
    }
}

/// Converts a logical selection into a physical plan.
/// Note, this utility is specific to subscriptions,
/// in that it does not support explicit column projections.
pub fn compile_select(project: ProjectName) -> ProjectPlan {
    compile_project_name(&mut NamesToIds::default(), project)
}

/// Converts a logical selection into a physical plan.
/// Note, this utility is applicable to a generic selections.
/// In particular, it supports explicit column projections.
pub fn compile_select_list(project: ProjectList) -> ProjectListPlan {
    compile_select_list_raw(&mut NamesToIds::default(), project)
}

pub fn compile_select_list_raw(var: &mut impl VarLabel, project: ProjectList) -> ProjectListPlan {
    compile_project_list(var, project)
}

/// Converts a logical DML statement into a physical plan,
/// but does not optimize it.
pub fn compile_dml_plan(stmt: DML) -> MutationPlan {
    match stmt {
        DML::Insert(insert) => MutationPlan::Insert(insert.into()),
        DML::Delete(delete) => MutationPlan::Delete(DeletePlan::compile(delete)),
        DML::Update(update) => MutationPlan::Update(UpdatePlan::compile(update)),
    }
}

pub fn compile(ast: StatementCtx<'_>) -> PhysicalCtx<'_> {
    let mut vars = NamesToIds::default();
    let plan = match ast.statement {
        Statement::Select(project) => PlanCtx::ProjectList(compile_select_list_raw(&mut vars, project)),
        Statement::DML(stmt) => PlanCtx::DML(compile_dml_plan(stmt)),
    };

    PhysicalCtx {
        plan,
        sql: ast.sql,
        vars: vars.into_map(),
        source: ast.source,
        planning_time: None,
    }
}
