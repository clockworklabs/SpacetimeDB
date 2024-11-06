use crate::plan::{IndexJoin, IndexOp, IndexScan, IndexSemiJoin, PhysicalCtx, PhysicalExpr, PhysicalPlan};
use spacetimedb_expr::ty::TyId;
use spacetimedb_lib::{AlgebraicType, AlgebraicValue};
use spacetimedb_primitives::TableId;
use spacetimedb_schema::schema::{ColumnSchema, IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::BinOp;
use std::collections::HashMap;
use std::fmt;

struct FieldOwned {
    name: String,
    ty: AlgebraicType,
    table_name: Box<str>,
    tyid: TyId,
}

struct Field<'a> {
    name: &'a str,
    ty: &'a AlgebraicType,
}

struct Fields<'a> {
    table_name: &'a str,
    fields: Vec<Field<'a>>,
}

impl<'a> Fields<'a> {
    pub fn new(table: &'a TableSchema) -> Self {
        let fields = table
            .columns()
            .iter()
            .map(|col| Field {
                name: col.col_name.as_ref(),
                ty: &col.col_type,
            })
            .collect();
        Self {
            table_name: &table.table_name,
            fields,
        }
    }
}

enum Expr<'a> {
    /// A binary expression
    BinOp(BinOp, Box<Expr<'a>>, Box<Expr<'a>>),
    /// A tuple expression
    Tuple(Vec<Expr<'a>>, TyId),
    /// A constant algebraic value
    Value(&'a AlgebraicValue, TyId),
    /// A field projection expression
    Field(FieldOwned),
    /// The input tuple to a relop
    Input(TyId),
    /// A star expression
    Star(Box<str>),
    IndexScan(&'a str, &'a ColumnSchema, &'a IndexOp),
}

impl<'a> Expr<'a> {
    pub fn new(plan: &PhysicalPlan, expr: &'a PhysicalExpr, data: &PrintBuilder<'_>) -> Self {
        match expr {
            PhysicalExpr::BinOp(op, lhs, rhs) => {
                let lhs = Expr::new(plan, lhs, data);
                let rhs = Expr::new(plan, rhs, data);
                Expr::BinOp(*op, Box::new(lhs), Box::new(rhs))
            }
            PhysicalExpr::Tuple(exprs, ty) => {
                let exprs = exprs.iter().map(|expr| Expr::new(plan, expr, data)).collect();
                Expr::Tuple(exprs, *ty)
            }
            PhysicalExpr::Value(value, ty) => Expr::Value(value, *ty),
            PhysicalExpr::Field(expr, pos, ty) => {
                if let Some(table) = data.table_by_ty(*ty) {
                    return Expr::Star(table.table_name.clone());
                }
                let table = plan.table_schema().lhs();

                let field = &table.columns()[*pos];
                let field = FieldOwned {
                    table_name: table.table_name.clone(),
                    name: field.col_name.to_string(),
                    ty: field.col_type.clone(),
                    tyid: *ty,
                };
                Expr::Field(field)
            }
            PhysicalExpr::Input(ty) => {
                if let Some(table) = data.table_by_ty(*ty) {
                    Expr::Star(table.table_name.clone())
                } else {
                    Expr::Input(*ty)
                }
            }
        }
    }
}

enum Line<'a> {
    TableScan {
        table_id: TableId,
        ty: TyId,
        ident: u16,
    },
    IndexScan {
        idx: &'a IndexScan,
        index: &'a IndexSchema,
        expr: Expr<'a>,
        ident: u16,
    },
    IndexJoin {
        idx: &'a IndexJoin,
        ident: u16,
    },
    IndexSemiJoin {
        idx: &'a IndexSemiJoin,
        ident: u16,
    },
    Filter {
        expr: Expr<'a>,
        ident: u16,
    },
    CrossJoin {
        ident: u16,
    },
    Project {
        expr: Expr<'a>,
        ident: u16,
    },
}

struct PlanData<'a> {
    lines: Vec<Line<'a>>,
    tables: HashMap<TableId, Fields<'a>>,
}

struct PrintBuilder<'a> {
    tables: HashMap<TableId, &'a TableSchema>,
    ty: HashMap<TyId, TableId>,
    lines: Vec<Line<'a>>,
}

impl<'a> PrintBuilder<'a> {
    fn table_by_ty(&self, ty: TyId) -> Option<&TableSchema> {
        self.ty.get(&ty).and_then(|table_id| self.tables.get(table_id).copied())
    }
}

pub struct PrintPlan<'a> {
    pub(crate) plan: &'a PhysicalPlan,
    pub(crate) show_tables: bool,
}

impl<'a> PrintPlan<'a> {
    pub(crate) fn print(&self) {
        println!("{}", self);
    }
}

impl<'a> PrintPlan<'a> {
    pub(crate) fn new(plan: &'a PhysicalPlan) -> Self {
        Self {
            plan,
            show_tables: false,
        }
    }

    pub(crate) fn show_tables(mut self) -> Self {
        self.show_tables = true;
        self
    }

    fn collect_tables(&self, data: &mut PrintBuilder<'a>) {
        match &self.plan {
            PhysicalPlan::TableScan(t, ty) => {
                data.tables.insert(t.table_id, t);
                data.ty.insert(*ty, t.table_id);
            }
            PhysicalPlan::IndexScan(idx) => {
                data.tables.insert(idx.table_schema.table_id, &idx.table_schema);
            }
            PhysicalPlan::IndexJoin(idx) => {
                data.tables.insert(idx.table.table_id, &idx.table);
            }
            PhysicalPlan::IndexSemiJoin(idx) => {
                data.tables.insert(idx.table.table_id, &idx.table);
            }
            PhysicalPlan::CrossJoin(join) => {
                PrintPlan::new(&join.lhs).collect_tables(data);
                PrintPlan::new(&join.rhs).collect_tables(data);
            }
            PhysicalPlan::Filter(filter) => {
                PrintPlan::new(&filter.input).collect_tables(data);
            }
            PhysicalPlan::Project(project) => {
                PrintPlan::new(&project.input).collect_tables(data);
            }
        }
    }

    fn _eval_plan(&self, data: &mut PrintBuilder<'a>, ident: u16) {
        match &self.plan {
            PhysicalPlan::TableScan(t, ty) => {
                data.lines.push(Line::TableScan {
                    table_id: t.table_id,
                    ty: *ty,
                    ident,
                });
            }
            PhysicalPlan::IndexScan(idx) => {
                let table = data.tables.get(&idx.table_schema.table_id).unwrap();
                let col = table.columns().iter().find(|c| c.col_pos == idx.col).unwrap();
                let index = table.indexes.iter().find(|i| i.index_id == idx.index_id).unwrap();
                let expr = Expr::IndexScan(&table.table_name, col, &idx.op);
                data.lines.push(Line::IndexScan {
                    idx,
                    expr,
                    index,
                    ident,
                });
            }
            PhysicalPlan::IndexJoin(idx) => {
                data.lines.push(Line::IndexJoin { idx, ident });
            }
            PhysicalPlan::IndexSemiJoin(idx) => {
                data.lines.push(Line::IndexSemiJoin { idx, ident });
            }
            PhysicalPlan::CrossJoin(join) => {
                data.lines.push(Line::CrossJoin { ident });
                PrintPlan::new(&join.lhs)._eval_plan(data, ident + 1);
                PrintPlan::new(&join.rhs)._eval_plan(data, ident + 1);
            }
            PhysicalPlan::Filter(filter) => {
                let expr = Expr::new(&filter.input, &filter.op, data);
                data.lines.push(Line::Filter { expr, ident });
                PrintPlan::new(&filter.input)._eval_plan(data, ident + 1);
            }
            PhysicalPlan::Project(project) => {
                data.lines.push(Line::Project {
                    expr: Expr::new(&project.input, &project.op, data),
                    ident,
                });
                PrintPlan::new(&project.input)._eval_plan(data, ident + 1);
            }
        }
    }

    fn eval_plan(&self, ident: u16) -> PlanData<'a> {
        let mut data = PrintBuilder {
            tables: Default::default(),
            ty: Default::default(),
            lines: Vec::new(),
        };

        self.collect_tables(&mut data);
        self._eval_plan(&mut data, ident);

        PlanData {
            lines: data.lines,
            tables: data
                .tables
                .iter()
                .map(|(table_id, v)| (*table_id, Fields::new(v)))
                .collect(),
        }
    }
}

impl<'a> fmt::Display for Expr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::BinOp(op, lhs, rhs) => {
                write!(f, "({} {} {})", lhs, op, rhs)
            }
            Expr::Tuple(exprs, ty) => {
                for (pos, expr) in exprs.iter().enumerate() {
                    write!(f, "{}", expr)?;
                    if pos < exprs.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            Expr::Value(value, ty) => {
                write!(f, "{:?}:{:?}", value, ty)
            }
            Expr::Field(field) => {
                write!(f, "{}.{}:{:?}/{}", field.table_name, field.name, field.ty, field.tyid)
            }
            Expr::Input(ty) => {
                write!(f, "{:?}", ty)
            }
            Expr::Star(name) => {
                write!(f, "{name}.*")
            }
            Expr::IndexScan(table_name, col, op) => {
                write!(f, "{table_name}.{}:{:?}", col.col_name, col.col_type)?;
                match op {
                    IndexOp::Eq(expr, ty) => {
                        write!(f, " = {:?}:{:?}", expr, ty)
                    }
                    IndexOp::Range(start, end, ty) => {
                        write!(f, " BETWEEN {:?} AND {:?}:{:?}", start, end, ty)
                    }
                }
            }
        }
    }
}

impl<'a> fmt::Display for PrintPlan<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data = self.eval_plan(0);
        for line in &data.lines {
            match line {
                Line::TableScan { table_id, ty, ident } => {
                    let table = data.tables.get(table_id).unwrap();
                    writeln!(
                        f,
                        "{:indent$}TableScan({}:{ty})",
                        "",
                        table.table_name,
                        indent = *ident as usize * 2
                    )?;
                }
                Line::Filter { expr, ident } => {
                    writeln!(f, "{:indent$}Filter: {expr}", "", indent = *ident as usize * 2)?;
                }
                Line::CrossJoin { ident } => {
                    writeln!(f, "{:indent$}CrossJoin:", "", indent = *ident as usize * 2)?;
                }
                Line::Project { expr, ident } => {
                    writeln!(f, "{:indent$}Project: [{expr}]", "", indent = *ident as usize * 2)?;
                }
                Line::IndexScan {
                    idx,
                    index,
                    expr,
                    ident,
                } => {
                    writeln!(
                        f,
                        "{:indent$}IndexScan({table}: {index}): {expr}",
                        "",
                        table = idx.table_schema.table_name,
                        indent = *ident as usize * 2,
                        index = index.index_name
                    )?;
                }
                Line::IndexJoin { idx, ident } => {
                    let table = data.tables.get(&idx.table.table_id).unwrap();
                    writeln!(
                        f,
                        "{:indent$}IndexJoin({})",
                        "",
                        table.table_name,
                        indent = *ident as usize * 2
                    )?;
                }
                Line::IndexSemiJoin { idx, ident } => {
                    let table = data.tables.get(&idx.table.table_id).unwrap();
                    writeln!(
                        f,
                        "{:indent$}IndexSemiJoin({})",
                        "",
                        table.table_name,
                        indent = *ident as usize * 2
                    )?;
                }
            }
        }

        if self.show_tables && !data.lines.is_empty() {
            writeln!(f, "------")?;
            // Show the tables with their columns
            for (table_pos, table) in data.tables.values().enumerate() {
                let cols = table.fields.len();
                write!(f, "{}: [", table.table_name)?;
                for (pos, col) in table.fields.iter().enumerate() {
                    write!(f, "{}:{:?}", col.name, col.ty)?;
                    if pos < cols - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "];")?;
                if table_pos < data.tables.len() - 1 {
                    writeln!(f, "")?;
                }
            }
        }
        Ok(())
    }
}

impl<'a> fmt::Display for PhysicalCtx<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:?}({})\n{}",
            self.source,
            self.sql,
            PrintPlan::new(&self.plan).show_tables()
        )
    }
}
