use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::Bound;

use crate::plan::{
    IxScan, Label, PhysicalCtx, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Sarg, Semi, TupleField,
};
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, IndexId};
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

/// The options for the printer
///
/// By default:
///
/// * `show_source: false`
/// * `show_schema: false`
/// * `show_timings: false`
///
/// * `optimize: true`
#[derive(Debug, Copy, Clone)]
pub struct ExplainOptions {
    pub show_source: bool,
    pub show_schema: bool,
    pub show_timings: bool,
    pub optimize: bool,
}

impl ExplainOptions {
    pub fn new() -> Self {
        Self {
            show_source: false,
            show_schema: false,
            show_timings: false,
            optimize: true,
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

    pub fn optimize(mut self, optimize: bool) -> Self {
        self.optimize = optimize;
        self
    }
}

impl Default for ExplainOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// The name or alias of the `table` with his schema
struct Schema<'a> {
    /// The table schema
    table: &'a TableSchema,
    /// The table name *OR* alias
    name: &'a str,
}

/// A map of labels to table schema and name (that is potentially an alias)
struct Labels<'a> {
    // To keep the output consistent between runs...
    labels: BTreeMap<usize, Schema<'a>>,
}

impl<'a> Labels<'a> {
    pub fn new() -> Self {
        Self {
            labels: Default::default(),
        }
    }

    /// Insert a new label with the [`TableSchema`] and `name`
    fn insert(&mut self, idx: Label, table: &'a TableSchema, name: &'a str) {
        self.labels.entry(idx.0).or_insert(Schema { table, name });
    }

    /// Get the table schema by [`Label`]
    fn table_by_label(&self, label: &Label) -> Option<&Schema<'a>> {
        self.labels.get(&label.0)
    }

    fn _field(&self, label: &Label, col: usize) -> Option<Field<'a>> {
        if let Some(schema) = self.table_by_label(label) {
            if let Some(field) = schema.table.get_column(col) {
                return Some(Field {
                    table: schema.name,
                    field: field.col_name.as_ref(),
                });
            }
        }
        None
    }

    /// Get the field by [`Label`] of the `table` and the column index
    fn label(&self, label: &Label, col: ColId) -> Option<Field<'a>> {
        self._field(label, col.idx())
    }

    /// Get the field by [`TupleField`]
    fn field(&self, field: &TupleField) -> Option<Field<'a>> {
        self._field(&field.label, field.field_pos)
    }
}

/// A pretty printer for physical expressions
struct PrintExpr<'a> {
    expr: &'a PhysicalExpr,
    labels: &'a Labels<'a>,
}

/// A pretty printer for sargable expressions
struct PrintSarg<'a> {
    expr: &'a Sarg,
    label: Label,
    labels: &'a Labels<'a>,
    prefix: &'a [(ColId, AlgebraicValue)],
}

/// A pretty printer for indexes
pub enum PrintIndex<'a> {
    Named(&'a str, Vec<&'a ColumnSchema>),
    Id(IndexId, Vec<&'a ColumnSchema>),
}

impl<'a> PrintIndex<'a> {
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
        index: PrintIndex<'a>,
        label: Label,
        ident: u16,
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

/// A `field` in a `table`
#[derive(Debug, Clone)]
pub struct Field<'a> {
    table: &'a str,
    field: &'a str,
}

/// The output of the plan, aka the projected columns
enum Output<'a> {
    Unknown,
    Star(Vec<Field<'a>>),
    Fields(Vec<Field<'a>>),
}

impl<'a> Output<'a> {
    fn tuples(fields: &[(Box<str>, TupleField)], lines: &Lines<'a>) -> Vec<Field<'a>> {
        fields
            .iter()
            .map(|(_, field)| lines.labels.field(field).unwrap())
            .collect()
    }

    fn fields(schema: &Schema<'a>) -> Vec<Field<'a>> {
        schema
            .table
            .columns()
            .iter()
            .map(|x| Field {
                table: schema.name,
                field: &x.col_name,
            })
            .collect()
    }
}

/// A list of lines to print
struct Lines<'a> {
    lines: Vec<Line<'a>>,
    labels: Labels<'a>,
    output: Output<'a>,
    /// A map of label to table name or alias
    vars: HashMap<usize, &'a str>,
}

impl<'a> Lines<'a> {
    pub fn new(vars: HashMap<usize, &'a str>) -> Self {
        Self {
            lines: Vec::new(),
            labels: Labels::new(),
            output: Output::Unknown,
            vars,
        }
    }

    pub fn add(&mut self, line: Line<'a>) {
        self.lines.push(line);
    }

    /// Resolve the label to the [`TableSchema`], and add it to the list of labels
    pub fn add_table(&mut self, label: Label, table: &'a TableSchema) {
        let name = self.vars.get(&label.0).copied().unwrap_or(table.table_name.as_ref());
        self.labels.insert(label, table, name);
    }
}

fn eval_expr<'a>(lines: &mut Lines<'a>, expr: &'a PhysicalExpr, ident: u16) {
    lines.add(Line::Filter { expr, ident });
}

/// Determine the output of a join using the direction of the [`Semi`] join
fn output_join<'a>(lines: &Lines<'a>, semi: &'a Semi, label_lhs: Label, label_rhs: Label) -> Output<'a> {
    match semi {
        Semi::Lhs => {
            let schema = lines.labels.table_by_label(&label_lhs).unwrap();
            Output::Star(Output::fields(schema))
        }
        Semi::Rhs => {
            let schema = lines.labels.table_by_label(&label_rhs).unwrap();
            Output::Star(Output::fields(schema))
        }
        Semi::All => {
            let schema = lines.labels.table_by_label(&label_lhs).unwrap();
            let lhs = Output::fields(schema);
            let schema = lines.labels.table_by_label(&label_rhs).unwrap();
            let rhs = Output::fields(schema);

            Output::Star(lhs.iter().chain(rhs.iter()).cloned().collect())
        }
    }
}

fn eval_plan<'a>(lines: &mut Lines<'a>, plan: &'a PhysicalPlan, ident: u16) {
    match plan {
        PhysicalPlan::TableScan(schema, label, _delta) => {
            lines.add_table(*label, schema);

            let schema = lines.labels.table_by_label(label).unwrap();

            lines.output = Output::Star(Output::fields(schema));

            lines.add(Line::TableScan {
                table: schema.name,
                label: *label,
                ident,
            });
        }
        PhysicalPlan::IxScan(idx, label) => {
            lines.add_table(*label, &idx.schema);
            let schema = lines.labels.table_by_label(label).unwrap();
            lines.output = Output::Star(Output::fields(schema));

            let index = idx.schema.indexes.iter().find(|x| x.index_id == idx.index_id).unwrap();

            lines.add(Line::IxScan {
                table_name: &idx.schema.table_name,
                index: PrintIndex::new(index, &idx.schema),
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

            lines.output = output_join(lines, semi, idx.lhs_field.label, idx.rhs_label);

            let lhs = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs = lines.labels.label(&idx.rhs_label, idx.rhs_field).unwrap();

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

            lines.output = output_join(lines, semi, idx.lhs_field.label, idx.rhs_field.label);

            let lhs = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs = lines.labels.field(&idx.rhs_field).unwrap();

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

/// A pretty printer for physical plans
///
/// The printer will format the plan in a human-readable format, suitable for the `EXPLAIN` command.
///
/// It also supports:
///
/// - Showing the source SQL statement
/// - Showing the schema of the tables
/// - Showing the planning time
pub struct Explain<'a> {
    ctx: &'a PhysicalCtx<'a>,
    lines: Vec<Line<'a>>,
    labels: Labels<'a>,
    output: Output<'a>,
    options: ExplainOptions,
}

impl<'a> Explain<'a> {
    pub fn new(ctx: &'a PhysicalCtx<'a>) -> Self {
        Self {
            ctx,
            lines: Vec::new(),
            labels: Labels::new(),
            output: Output::Unknown,
            options: ExplainOptions::new(),
        }
    }

    /// Set the options for the printer
    pub fn with_options(mut self, options: ExplainOptions) -> Self {
        self.options = options;
        self
    }

    /// Evaluate the plan and build the lines to print
    fn lines(&self) -> Lines<'a> {
        let mut lines = Lines::new(self.ctx.vars.iter().map(|(x, y)| (*y, x.as_str())).collect());
        match &self.ctx.plan {
            ProjectListPlan::Name(ProjectPlan::None(plan)) => {
                eval_plan(&mut lines, plan, 0);
            }
            ProjectListPlan::Name(ProjectPlan::Name(plan, label, _count)) => {
                eval_plan(&mut lines, plan, 0);
                let schema = lines.labels.table_by_label(label).unwrap();
                lines.output = Output::Star(Output::fields(schema));
            }
            ProjectListPlan::List(plan, fields) => {
                eval_plan(&mut lines, plan, 0);
                lines.output = Output::Fields(Output::tuples(fields, &lines));
            }
        }

        lines
    }

    /// Build the `Explain` output
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
                let col = self.labels.field(field).unwrap();
                write!(f, "{col}")
            }
        }
    }
}

impl<'a> fmt::Display for PrintSarg<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expr {
            Sarg::Eq(lhs, rhs) => {
                let col = self.labels.label(&self.label, *lhs).unwrap();
                write!(f, "{col} = {:?}", rhs)?;
                for (col, val) in self.prefix {
                    let col = self.labels.label(&self.label, *col).unwrap();
                    write!(f, ", {col} = {:?}", val)?;
                }
                Ok(())
            }
            Sarg::Range(col, lower, upper) => {
                let col = self.labels.label(&self.label, *col).unwrap();
                let op = range_to_op(lower, upper);
                write!(f, "{col} {:?} {op}{:?}", lower, upper)
            }
        }
    }
}

impl<'a> fmt::Display for Field<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table, self.field)
    }
}
impl<'a> fmt::Display for PrintIndex<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cols = match self {
            PrintIndex::Named(name, cols) => {
                write!(f, "Index {name}: ")?;
                cols
            }
            PrintIndex::Id(id, cols) => {
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

        if self.options.show_source {
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
                    if self.options.show_schema {
                        write!(f, "Seq Scan on {}:{}", table, label.0)?;
                    } else {
                        write!(f, "Seq Scan on {}", table)?;
                    }
                }
                Line::IxScan {
                    table_name,
                    index,
                    label,
                    ident: _,
                } => {
                    if self.options.show_schema {
                        write!(f, "Index Scan using {index} on {table_name}:{}", label.0)?;
                    } else {
                        write!(f, "Index Scan using {index} on {table_name}")?;
                    }
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
                            prefix: &idx.prefix,
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
            Output::Star(fields) => {
                let columns = fields.iter().map(|x| format!("{}", x)).join(", ");
                Some(columns)
            }
            Output::Fields(fields) => {
                let columns = fields.iter().map(|x| format!("{}", x)).join(", ");
                Some(columns)
            }
        };
        let end = if self.options.show_timings || self.options.show_schema {
            "\n"
        } else {
            ""
        };
        if let Some(columns) = columns {
            write!(f, "  Output: {columns}{end}")?;
        } else {
            write!(f, "  Output: ?{end}")?;
        }

        if self.options.show_timings {
            let end = if self.options.show_schema { "\n" } else { "" };
            write!(f, "Planning Time: {:?}{end}", ctx.planning_time)?;
        }

        if self.options.show_schema {
            writeln!(f, "-------")?;
            writeln!(f, "Schema:")?;
            writeln!(f)?;
            for (pos, (label, schema)) in self.labels.labels.iter().enumerate() {
                writeln!(f, "Label {}: {}", schema.name, label)?;
                let columns = schema.table.columns().iter().map(|x| &x.col_name).join(", ");
                writeln!(f, "  Columns: {columns}")?;

                write!(
                    f,
                    "  Indexes: {}",
                    schema
                        .table
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
                                                table: schema.name,
                                                field: &schema.table.columns()[x.idx()].col_name,
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
