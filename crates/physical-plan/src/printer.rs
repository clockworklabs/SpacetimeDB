use itertools::Itertools;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Bound;

use crate::plan::{
    IxScan, Label, PhysicalCtx, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Sarg, Semi, TupleField,
};
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::IndexId;
use spacetimedb_schema::def::ConstraintData;
use spacetimedb_schema::schema::{ColumnSchema, IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::BinOp;

fn range_to_op(lower: &Bound<AlgebraicValue>, upper: &Bound<AlgebraicValue>) -> BinOp {
    match (lower, upper) {
        (Bound::Included(_), Bound::Included(_)) => BinOp::Lte,
        (Bound::Included(_), Bound::Excluded(_)) => BinOp::Lt,
        (Bound::Excluded(_), Bound::Included(_)) => BinOp::Gt,
        (Bound::Excluded(_), Bound::Excluded(_)) => BinOp::Gte,
        (Bound::Unbounded, Bound::Included(_)) => BinOp::Lte,
        (Bound::Unbounded, Bound::Excluded(_)) => BinOp::Lt,
        (Bound::Included(_), Bound::Unbounded) => BinOp::Gte,
        (Bound::Excluded(_), Bound::Unbounded) => BinOp::Gt,
        (Bound::Unbounded, Bound::Unbounded) => BinOp::Eq,
    }
}

struct Labels<'a> {
    // To keep the output consistent between runs...
    labels: BTreeMap<usize, &'a TableSchema>,
}

impl<'a> Labels<'a> {
    fn insert(&mut self, idx: usize, schema: &'a TableSchema) {
        self.labels.insert(idx, schema);
    }

    fn field(&self, label: &Label, pos: usize) -> Option<Field<'a>> {
        if let Some(table) = self.labels.get(&label.0) {
            if let Some(field) = table.get_column(pos) {
                return Some(Field { table, field });
            }
        };
        None
    }
    fn field_tuple(&self, field: &TupleField) -> Option<Field<'a>> {
        self.field(&field.label, field.field_pos)
    }
}

impl<'a> Labels<'a> {
    pub fn new() -> Self {
        Self {
            labels: Default::default(),
        }
    }
}

struct PrintExpr<'a> {
    expr: &'a PhysicalExpr,
    labels: &'a Labels<'a>,
}

struct PrintSarg<'a> {
    expr: &'a Sarg,
    label: Label,
    labels: &'a Labels<'a>,
}

pub enum Index<'a> {
    Named(&'a str, Vec<&'a ColumnSchema>),
    Id(IndexId, Vec<&'a ColumnSchema>),
}

impl<'a> Index<'a> {
    pub fn new(idx: &'a IndexSchema, table: &'a TableSchema) -> Self {
        let cols = idx
            .index_algorithm
            .columns()
            .iter()
            .map(|x| table.get_column(x.idx()).unwrap())
            .collect_vec();

        if idx.index_name.is_empty() {
            Self::Id(idx.index_id, cols)
        } else {
            Self::Named(&idx.index_name, cols)
        }
    }
}

pub enum JoinKind {
    IxJoin,
    HashJoin,
    NlJoin,
}

/// A formated line of output
pub enum Line<'a> {
    TableScan {
        table: &'a str,
        label: Label,
        ident: u16,
    },
    Filter {
        expr: &'a PhysicalExpr,
        ident: u16,
    },
    FilterIxScan {
        idx: &'a IxScan,
        label: Label,
        ident: u16,
    },
    IxScan {
        table_name: &'a str,
        index: Index<'a>,
        ident: u16,
        label: Label,
    },
    IxJoin {
        semi: &'a Semi,
        ident: u16,
    },
    HashJoin {
        semi: &'a Semi,
        ident: u16,
    },
    NlJoin {
        ident: u16,
    },
    JoinExpr {
        kind: JoinKind,
        unique: bool,
        lhs: Field<'a>,
        rhs: Field<'a>,
        ident: u16,
    },
}

impl<'a> Line<'a> {
    pub fn ident(&self) -> usize {
        let ident = match self {
            Line::TableScan { ident, .. } => *ident,
            Line::Filter { ident, .. } => *ident,
            Line::FilterIxScan { ident, .. } => *ident,
            Line::IxScan { ident, .. } => *ident,
            Line::IxJoin { ident, .. } => *ident,
            Line::HashJoin { ident, .. } => *ident,
            Line::NlJoin { ident, .. } => *ident,
            Line::JoinExpr { ident, .. } => *ident,
        };
        ident as usize
    }
}

pub struct Field<'a> {
    table: &'a TableSchema,
    field: &'a ColumnSchema,
}

enum Output<'a> {
    Unknown,
    Star(&'a TableSchema),
    Fields(Vec<Field<'a>>),
}

struct Lines<'a> {
    lines: Vec<Line<'a>>,
    labels: Labels<'a>,
    output: Output<'a>,
}

impl<'a> Lines<'a> {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            labels: Labels::new(),
            output: Output::Unknown,
        }
    }

    pub fn add(&mut self, line: Line<'a>) {
        self.lines.push(line);
    }

    pub fn add_table(&mut self, label: Label, table: &'a TableSchema) {
        //TODO: Need PR to fix this
        // assert_eq!(
        //     label.0, table.table_id.0 as usize,
        //     "Label mismatch: {:?}",
        //     &table.table_name
        // );
        self.labels.insert(label.0, table);
    }
}

fn eval_expr<'a>(lines: &mut Lines<'a>, expr: &'a PhysicalExpr, ident: u16) {
    lines.add(Line::Filter { expr, ident });
}

fn eval_plan<'a>(lines: &mut Lines<'a>, plan: &'a PhysicalPlan, ident: u16) {
    match plan {
        PhysicalPlan::TableScan(schema, label, _delta) => {
            lines.output = Output::Star(schema);
            lines.add_table(*label, schema);
            lines.add(Line::TableScan {
                table: &schema.table_name,
                label: *label,
                ident,
            });
        }
        PhysicalPlan::IxScan(idx, label) => {
            lines.output = Output::Star(&idx.schema);
            lines.add_table(*label, &idx.schema);
            let index = idx.schema.indexes.iter().find(|x| x.index_id == idx.index_id).unwrap();
            lines.add(Line::IxScan {
                table_name: &idx.schema.table_name,
                index: Index::new(index, &idx.schema),
                label: *label,
                ident,
            });
            lines.add(Line::FilterIxScan {
                idx,
                label: *label,
                ident: ident + 2,
            });
        }
        PhysicalPlan::IxJoin(idx, semi) => {
            lines.add_table(idx.rhs_label, &idx.rhs);
            lines.add(Line::IxJoin { semi, ident });
            eval_plan(lines, &idx.lhs, ident + 4);

            let lhs = lines
                .labels
                .field(&idx.lhs_field.label, idx.lhs_field.field_pos)
                .unwrap();
            let rhs = lines.labels.field(&idx.rhs_label, idx.rhs_field.idx()).unwrap();
            lines.add(Line::JoinExpr {
                kind: JoinKind::IxJoin,
                unique: idx.unique,
                lhs,
                rhs,
                ident: ident + 2,
            });
        }
        PhysicalPlan::HashJoin(idx, semi) => {
            lines.add(Line::HashJoin { semi, ident });
            eval_plan(lines, &idx.lhs, ident + 4);
            eval_plan(lines, &idx.rhs, ident + 4);
            let lhs = lines.labels.field_tuple(&idx.lhs_field).unwrap();
            let rhs = lines.labels.field_tuple(&idx.rhs_field).unwrap();
            lines.add(Line::JoinExpr {
                kind: JoinKind::HashJoin,
                unique: idx.unique,
                lhs,
                rhs,
                ident: ident + 2,
            });
        }
        PhysicalPlan::NLJoin(lhs, rhs) => {
            lines.add(Line::NlJoin { ident });
            eval_plan(lines, lhs, ident + 4);
            eval_plan(lines, rhs, ident + 4);
        }
        PhysicalPlan::Filter(plan, filter) => {
            eval_plan(lines, plan, ident);
            eval_expr(lines, filter, ident + 2);
        }
    }
}

pub struct Explain<'a> {
    ctx: &'a PhysicalCtx<'a>,
    lines: Vec<Line<'a>>,
    labels: Labels<'a>,
    output: Output<'a>,
    show_source: bool,
    show_schema: bool,
    show_timings: bool,
}

impl<'a> Explain<'a> {
    pub fn new(ctx: &'a PhysicalCtx<'a>) -> Self {
        Self {
            ctx,
            lines: Vec::new(),
            labels: Labels::new(),
            output: Output::Unknown,
            show_source: false,
            show_schema: false,
            show_timings: false,
        }
    }

    pub fn with_source(mut self) -> Self {
        self.show_source = true;
        self
    }

    pub fn with_schema(mut self) -> Self {
        self.show_schema = true;
        self
    }

    pub fn with_timings(mut self) -> Self {
        self.show_timings = true;
        self
    }

    fn lines(&self) -> Lines<'a> {
        let mut lines = Lines::new();
        match &self.ctx.plan {
            ProjectListPlan::Name(plan) => match plan {
                ProjectPlan::None(plan) => {
                    eval_plan(&mut lines, plan, 0);
                }
                ProjectPlan::Name(plan, _label, _) => {
                    eval_plan(&mut lines, plan, 0);
                }
            },
            ProjectListPlan::List(plan, fields) => {
                eval_plan(&mut lines, plan, 0);
                lines.output = Output::Fields(
                    fields
                        .iter()
                        .map(|(_, field)| {
                            let field = lines.labels.field_tuple(field).unwrap();
                            field
                        })
                        .collect(),
                );
            }
        }

        lines
    }

    pub fn build(self) -> Self {
        let lines = self.lines();
        Self {
            lines: lines.lines,
            labels: lines.labels,
            output: lines.output,
            ..self
        }
    }
}

impl<'a> fmt::Display for PrintExpr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expr {
            PhysicalExpr::LogOp(op, expr) => {
                write!(
                    f,
                    "{}",
                    expr.iter()
                        .map(|expr| PrintExpr {
                            expr,
                            labels: self.labels
                        })
                        .join(&format!(" {} ", op))
                )
            }
            PhysicalExpr::BinOp(op, lhs, rhs) => {
                write!(
                    f,
                    "{} {} {}",
                    PrintExpr {
                        expr: lhs,
                        labels: self.labels
                    },
                    op,
                    PrintExpr {
                        expr: rhs,
                        labels: self.labels
                    }
                )
            }
            PhysicalExpr::Value(val) => {
                write!(f, "{:?}", val)
            }
            PhysicalExpr::Field(field) => {
                let col = self.labels.field_tuple(field).unwrap();
                write!(f, "{col}")
            }
        }
    }
}

impl<'a> fmt::Display for PrintSarg<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expr {
            Sarg::Eq(lhs, rhs) => {
                let col = self.labels.field(&self.label, lhs.idx()).unwrap();
                write!(f, "{col} = {:?}", rhs)
            }
            Sarg::Range(col, lower, upper) => {
                let col = self.labels.field(&self.label, col.idx()).unwrap();
                let op = range_to_op(lower, upper);
                write!(f, "{col} {:?} {op}{:?}", lower, upper)
            }
        }
    }
}

impl<'a> fmt::Display for Field<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table.table_name, self.field.col_name)
    }
}
impl<'a> fmt::Display for Index<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cols = match self {
            Index::Named(name, cols) => {
                write!(f, "Index {name}: ")?;
                cols
            }
            Index::Id(id, cols) => {
                write!(f, "Index id {id}: ")?;
                cols
            }
        };
        write!(f, "({})", cols.iter().map(|x| &x.col_name).join(", "))
    }
}

impl<'a> fmt::Display for Explain<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ctx = self.ctx;

        if self.show_source {
            match ctx.source {
                StatementSource::Subscription => write!(f, "Subscription: {}", ctx.sql)?,
                StatementSource::Query => write!(f, "Query: {}", ctx.sql)?,
            }

            writeln!(f)?;
        }

        for line in &self.lines {
            let ident = line.ident();
            let (ident, arrow) = if ident > 2 { (ident - 2, "-> ") } else { (ident, "") };
            write!(f, "{:ident$}{arrow}", "")?;
            match line {
                Line::TableScan { table, label, ident: _ } => {
                    write!(f, "Seq Scan on {}:{}", table, label.0)?;
                }
                Line::IxScan {
                    table_name,
                    index,
                    label,
                    ident: _,
                } => {
                    write!(f, "Index Scan using {index} on {table_name}:{}", label.0)?;
                }
                Line::Filter { expr, ident: _ } => {
                    write!(
                        f,
                        "Filter: ({})",
                        PrintExpr {
                            expr,
                            labels: &self.labels,
                        },
                    )?;
                }
                Line::FilterIxScan { idx, label, ident: _ } => {
                    write!(
                        f,
                        "Index Cond: ({})",
                        PrintSarg {
                            expr: &idx.arg,
                            labels: &self.labels,
                            label: *label,
                        },
                    )?;
                }

                Line::IxJoin { semi, ident: _ } => {
                    write!(f, "Index Join: {semi:?}")?;
                }
                Line::HashJoin { semi, ident: _ } => {
                    write!(f, "Hash Join: {semi:?}")?;
                }
                Line::NlJoin { ident: _ } => {
                    write!(f, "Nested Loop")?;
                }
                Line::JoinExpr {
                    kind,
                    unique,
                    lhs,
                    rhs,
                    ident: _,
                } => {
                    let kind = match kind {
                        JoinKind::IxJoin => "Index Cond",
                        JoinKind::HashJoin => "Hash Cond",
                        JoinKind::NlJoin => "Loop Cond",
                    };
                    writeln!(f, "Inner Unique: {unique}")?;
                    write!(f, "{:ident$}{arrow}{kind}: ({} = {})", "", lhs, rhs)?;
                }
            }
            writeln!(f)?;
        }

        let columns = match &self.output {
            Output::Unknown => None,
            Output::Star(t) => {
                let columns = t.columns().iter().map(|x| &x.col_name).join(", ");
                Some(columns)
            }
            Output::Fields(fields) => {
                let columns = fields.iter().map(|x| format!("{}", x)).join(", ");
                Some(columns)
            }
        };
        let end = if self.show_timings || self.show_schema {
            "\n"
        } else {
            ""
        };
        if let Some(columns) = columns {
            write!(f, "  Output: {columns}{end}")?;
        } else {
            write!(f, "  Output: ?{end}")?;
        }

        if self.show_timings {
            write!(f, "Planning Time: {:?}", ctx.planning_time)?;
        }

        if self.show_schema {
            writeln!(f, "-------")?;
            writeln!(f, "Schema:")?;
            writeln!(f)?;
            for (pos, (label, schema)) in self.labels.labels.iter().enumerate() {
                writeln!(f, "Label {}: {}", schema.table_name, label)?;
                let columns = schema.columns().iter().map(|x| &x.col_name).join(", ");
                writeln!(f, "  Columns: {columns}")?;

                write!(
                    f,
                    "  Indexes: {}",
                    schema
                        .constraints
                        .iter()
                        .map(|x| {
                            match &x.data {
                                ConstraintData::Unique(idx) => format!(
                                    "Unique({})",
                                    idx.columns
                                        .iter()
                                        .map(|x| {
                                            Field {
                                                table: schema,
                                                field: &schema.columns()[x.idx()],
                                            }
                                        })
                                        .join(", ")
                                ),
                                _ => "".to_string(),
                            }
                        })
                        .join(", ")
                )?;

                if pos < self.labels.labels.len() - 1 {
                    writeln!(f)?;
                }
            }
        }

        Ok(())
    }
}
