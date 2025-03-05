use crate::dml::MutationPlan;
use crate::plan::{IxScan, Label, PhysicalExpr, PhysicalPlan, ProjectListPlan, ProjectPlan, Sarg, Semi, TupleField};
use crate::{PhysicalCtx, PlanCtx};
use itertools::Itertools;
use spacetimedb_expr::expr::AggType;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ConstraintId, IndexId};
use spacetimedb_schema::def::ConstraintData;
use spacetimedb_schema::schema::{IndexSchema, TableSchema};
use spacetimedb_sql_parser::ast::BinOp;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::Bound;

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

/// A pretty printer for objects that could have a empty name
pub enum PrintName<'a> {
    Named { object: &'a str, name: &'a str },
    Id { object: &'a str, id: usize },
}

impl<'a> PrintName<'a> {
    fn new(object: &'a str, id: usize, name: &'a str) -> Self {
        if name.is_empty() {
            Self::Id { object, id }
        } else {
            Self::Named { object, name }
        }
    }

    fn index(index_id: IndexId, index_name: &'a str) -> Self {
        Self::new("Index", index_id.idx(), index_name)
    }

    fn constraint(constraint_id: ConstraintId, constraint_name: &'a str) -> Self {
        Self::new("Constraint", constraint_id.idx(), constraint_name)
    }
}

/// A pretty printer for indexes
pub struct PrintIndex<'a> {
    name: PrintName<'a>,
    cols: Vec<Field<'a>>,
}

impl<'a> PrintIndex<'a> {
    pub fn new(idx: &'a IndexSchema, table: &'a TableSchema) -> Self {
        let cols = idx
            .index_algorithm
            .columns()
            .iter()
            .map(|x| Field {
                table: table.table_name.as_ref(),
                field: table.get_column(x.idx()).unwrap().col_name.as_ref(),
            })
            .collect_vec();

        Self {
            name: PrintName::index(idx.index_id, &idx.index_name),
            cols,
        }
    }
}

/// A formated line of output
pub enum Line<'a> {
    TableScan {
        table: &'a str,
        label: usize,
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
        index: PrintName<'a>,
        ident: u16,
    },
    IxJoin {
        semi: &'a Semi,
        rhs: String,
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
        unique: bool,
        lhs: Field<'a>,
        rhs: Field<'a>,
        ident: u16,
    },
    Limit {
        limit: u64,
        ident: u16,
    },
    Count {
        ident: u16,
    },
    Insert {
        table_name: &'a str,
        ident: u16,
    },
    Update {
        table_name: &'a str,
        columns: Vec<(Field<'a>, &'a AlgebraicValue)>,
        ident: u16,
    },
    Delete {
        table_name: &'a str,
        ident: u16,
    },
}

impl Line<'_> {
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
            Line::Limit { ident, .. } => *ident,
            Line::Count { ident, .. } => *ident,
            Line::Insert { ident, .. } => *ident,
            Line::Update { ident, .. } => *ident,
            Line::Delete { ident, .. } => *ident,
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
    Alias(&'a str),
    Empty,
}

impl<'a> Output<'a> {
    fn tuples(fields: &[TupleField], lines: &Lines<'a>) -> Vec<Field<'a>> {
        fields.iter().map(|field| lines.labels.field(field).unwrap()).collect()
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

    fn fields_update(
        schema: &'a TableSchema,
        fields: &'a [(ColId, AlgebraicValue)],
    ) -> Vec<(Field<'a>, &'a AlgebraicValue)> {
        fields
            .iter()
            .map(|(col, value)| {
                let field = Field {
                    table: schema.table_name.as_ref(),
                    field: &schema.get_column(col.idx()).unwrap().col_name,
                };
                (field, value)
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
        PhysicalPlan::TableScan(scan, label) => {
            lines.add_table(*label, &scan.schema);

            let schema = lines.labels.table_by_label(label).unwrap();
            let table = schema.name;
            lines.output = Output::Star(Output::fields(schema));

            let ident = if let Some(limit) = scan.limit {
                lines.add(Line::Limit { limit, ident });
                ident + 2
            } else {
                ident
            };

            lines.add(Line::TableScan {
                table,
                label: label.0,
                ident,
            });
        }
        PhysicalPlan::IxScan(idx, label) => {
            lines.add_table(*label, &idx.schema);
            let schema = lines.labels.table_by_label(label).unwrap();
            lines.output = Output::Star(Output::fields(schema));

            let index = idx.schema.indexes.iter().find(|x| x.index_id == idx.index_id).unwrap();

            let ident = if let Some(limit) = idx.limit {
                lines.add(Line::Limit { limit, ident });
                ident + 2
            } else {
                ident
            };

            lines.add(Line::IxScan {
                table_name: &idx.schema.table_name,
                index: PrintName::index(idx.index_id, &index.index_name),
                ident,
            });

            lines.add(Line::FilterIxScan {
                idx,
                label: *label,
                ident: ident + 4,
            });
        }
        PhysicalPlan::IxJoin(idx, semi) => {
            lines.add_table(idx.rhs_label, &idx.rhs);
            //let lhs = lines.labels.table_by_label(&idx.lhs_field.label).unwrap();
            let rhs = lines.labels.table_by_label(&idx.rhs_label).unwrap();
            lines.add(Line::IxJoin {
                semi,
                ident,
                rhs: rhs.name.to_string(),
            });

            eval_plan(lines, &idx.lhs, ident + 2);

            lines.output = output_join(lines, semi, idx.lhs_field.label, idx.rhs_label);

            let lhs = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs = lines.labels.label(&idx.rhs_label, idx.rhs_field).unwrap();

            lines.add(Line::JoinExpr {
                unique: idx.unique,
                lhs,
                rhs,
                ident: ident + 2,
            });
        }
        PhysicalPlan::HashJoin(idx, semi) => {
            lines.add(Line::HashJoin { semi, ident });

            eval_plan(lines, &idx.lhs, ident + 2);
            eval_plan(lines, &idx.rhs, ident + 2);

            lines.output = output_join(lines, semi, idx.lhs_field.label, idx.rhs_field.label);

            let lhs = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs = lines.labels.field(&idx.rhs_field).unwrap();

            lines.add(Line::JoinExpr {
                unique: idx.unique,
                lhs,
                rhs,
                ident: ident + 2,
            });
        }
        PhysicalPlan::NLJoin(lhs, rhs) => {
            lines.add(Line::NlJoin { ident });

            eval_plan(lines, lhs, ident + 2);
            eval_plan(lines, rhs, ident + 2);
        }
        PhysicalPlan::Filter(plan, filter) => {
            eval_plan(lines, plan, ident);
            eval_expr(lines, filter, ident + 2);
        }
    }
}

fn eval_dml_plan<'a>(lines: &mut Lines<'a>, plan: &'a MutationPlan, ident: u16) {
    match plan {
        MutationPlan::Insert(plan) => {
            let schema = &plan.table;

            lines.add(Line::Insert {
                table_name: &schema.table_name,
                ident,
            });
        }

        MutationPlan::Delete(plan) => {
            let schema = &plan.table;

            lines.add(Line::Delete {
                table_name: &schema.table_name,
                ident,
            });
            eval_plan(lines, &plan.filter, ident + 2);
        }
        MutationPlan::Update(plan) => {
            let schema = &plan.table;

            lines.add(Line::Update {
                table_name: &schema.table_name,
                columns: Output::fields_update(schema, &plan.columns),
                ident,
            });
            eval_plan(lines, &plan.filter, ident + 2);
        }
    }
    lines.output = Output::Empty;
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
            PlanCtx::ProjectList(plan) => match plan {
                ProjectListPlan::Name(plan) => {
                    //eval_plan(&mut lines, plan, 0);
                    todo!()
                }
                // ProjectListPlan::Name(ProjectPlan::Name(plan, label, _count)) => {
                //     eval_plan(&mut lines, plan, 0);
                //     let schema = lines.labels.table_by_label(label).unwrap();
                //     lines.output = Output::Star(Output::fields(schema));
                // }
                ProjectListPlan::List(plan, fields) => {
                    // eval_plan(&mut lines, plan, 0);
                    lines.output = Output::Fields(Output::tuples(fields, &lines));
                }
                ProjectListPlan::Limit(plan, limit) => {
                    lines.add(Line::Limit {
                        limit: *limit,
                        ident: 0,
                    });
                    eval_plan(&mut lines, plan, 2);
                }
                ProjectListPlan::Agg(plan, agg) => {
                    match agg {
                        AggType::Count { alias } => {
                            lines.add(Line::Count { ident: 0 });
                            lines.output = Output::Alias(alias)
                        }
                    }
                    eval_plan(&mut lines, plan, 2);
                }
            },
            PlanCtx::DML(plan) => {
                eval_dml_plan(&mut lines, plan, 0);
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

impl fmt::Display for PrintExpr<'_> {
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

impl fmt::Display for PrintSarg<'_> {
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

impl fmt::Display for Field<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.table, self.field)
    }
}

impl fmt::Display for PrintName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrintName::Named { object, name } => write!(f, "{} {}", object, name),
            PrintName::Id { object, id } => write!(f, "{} id {}", object, id),
        }
    }
}

impl fmt::Display for PrintIndex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.name)?;
        write!(f, "({})", self.cols.iter().join(", "))
    }
}

impl fmt::Display for Explain<'_> {
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
            let arrow = if ident > 0 { "-> " } else { "" };

            match line {
                Line::TableScan { table, label, ident: _ } => {
                    if self.options.show_schema {
                        write!(f, "{:ident$}{arrow}Seq Scan on {table}:{label}", "")?;
                    } else {
                        write!(f, "{:ident$}{arrow}Seq Scan on {table}", "")?;
                    }
                }
                Line::IxScan {
                    table_name,
                    index,
                    ident: _,
                } => {
                    write!(f, "{:ident$}{arrow}Index Scan using {index} on {table_name}", "")?;
                }
                Line::Filter { expr, ident: _ } => {
                    write!(f, "{:ident$}Filter: ", "")?;
                    write!(
                        f,
                        "({})",
                        PrintExpr {
                            expr,
                            labels: &self.labels,
                        }
                    )?;
                }
                Line::FilterIxScan { idx, label, ident: _ } => {
                    write!(f, "{:ident$}Index Cond: ", "")?;
                    write!(
                        f,
                        "({})",
                        PrintSarg {
                            expr: &idx.arg,
                            prefix: &idx.prefix,
                            labels: &self.labels,
                            label: *label,
                        },
                    )?;
                }

                Line::IxJoin { semi, rhs, ident: _ } => {
                    write!(f, "{:ident$}{arrow}Index Join: {semi:?} on {rhs}", "")?;
                }
                Line::HashJoin { semi, ident: _ } => match semi {
                    Semi::All => {
                        write!(f, "{:ident$}{arrow}Hash Join", "")?;
                    }
                    semi => {
                        write!(f, "{:ident$}{arrow}Hash Join: {semi:?}", "")?;
                    }
                },
                Line::NlJoin { ident: _ } => {
                    write!(f, "{:ident$}{arrow}Nested Loop", "")?;
                }
                Line::JoinExpr {
                    unique,
                    lhs,
                    rhs,
                    ident: _,
                } => {
                    writeln!(f, "{:ident$}Inner Unique: {unique}", "")?;
                    write!(f, "{:ident$}Join Cond: ({} = {})", "", lhs, rhs)?;
                }
                Line::Limit { limit, ident: _ } => {
                    write!(f, "{:ident$}{arrow}Limit: {limit}", "")?;
                }
                Line::Count { ident: _ } => {
                    write!(f, "{:ident$}{arrow}Count", "")?;
                }
                Line::Insert { table_name, ident: _ } => {
                    write!(f, "{:ident$}{arrow}Insert on {table_name}", "")?;
                }
                Line::Update {
                    table_name,
                    columns,
                    ident: _,
                } => {
                    let columns = columns
                        .iter()
                        .map(|(field, value)| format!("{} = {:?}", field, value))
                        .join(", ");
                    write!(f, "{:ident$}{arrow}Update on {table_name} SET ({columns })", "")?;
                }
                Line::Delete { table_name, ident: _ } => {
                    write!(f, "{:ident$}{arrow}Delete on {table_name}", "")?;
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
            Output::Alias(alias) => Some(alias.to_string()),
            Output::Empty => Some("void".to_string()),
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
            for (pos, (_label, schema)) in self.labels.labels.iter().enumerate() {
                writeln!(f, "Label: {}, TableId:{}", schema.name, schema.table.table_id)?;
                let columns = schema.table.columns().iter().map(|x| &x.col_name).join(", ");
                writeln!(f, "  Columns: {columns}")?;

                writeln!(
                    f,
                    "  Indexes: {}",
                    schema
                        .table
                        .indexes
                        .iter()
                        .map(|x| PrintIndex::new(x, schema.table))
                        .join(", ")
                )?;

                write!(
                    f,
                    "  Constraints: {}",
                    schema
                        .table
                        .constraints
                        .iter()
                        .map(|x| {
                            match &x.data {
                                ConstraintData::Unique(idx) => format!(
                                    "{}: Unique({})",
                                    PrintName::constraint(x.constraint_id, &x.constraint_name),
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
