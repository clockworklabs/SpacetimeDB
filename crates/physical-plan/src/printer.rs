use crate::dml::{InsertPlan, MutationPlan, UpdatePlan};
use crate::plan::{
    IxJoin, IxScan, Label, PhysicalExpr, PhysicalPlan, ProjectListPlan, Sarg, Semi, TableScan, TupleField,
};
use crate::{PhysicalCtx, PlanCtx};
use itertools::Itertools;
use spacetimedb_expr::expr::AggType;
use spacetimedb_expr::StatementSource;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColId, ColList, ConstraintId, IndexId};
use spacetimedb_schema::def::ConstraintData;
use spacetimedb_schema::schema::{IndexSchema, TableSchema};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
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
    pub(crate) fn new() -> Self {
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
#[derive(Debug, Clone)]
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
    fn new() -> Self {
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

/// A pretty printer for objects with their name (if available) or their id
#[derive(Debug)]
enum PrintName<'a> {
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
#[derive(Debug)]
struct PrintIndex<'a> {
    name: PrintName<'a>,
    cols: Vec<Field<'a>>,
    table: &'a str,
    unique: bool,
}

impl<'a> PrintIndex<'a> {
    fn new(idx: &'a IndexSchema, table: &'a TableSchema, cols: ColList) -> Self {
        let unique = table.is_unique(&cols);

        let cols = cols
            .iter()
            .map(|x| Field {
                table: table.table_name.as_ref(),
                field: table.get_column(x.idx()).unwrap().col_name.as_ref(),
            })
            .collect_vec();

        Self {
            name: PrintName::index(idx.index_id, &idx.index_name),
            cols,
            table: &table.table_name,
            unique,
        }
    }

    fn from_index(idx: &'a IndexSchema, table: &'a TableSchema) -> Self {
        let cols = ColList::from_iter(idx.index_algorithm.columns().iter());
        Self::new(idx, table, cols)
    }

    fn from_scan(idx: &'a IxScan, index: &'a IndexSchema, table: &'a TableSchema) -> Self {
        let start = match idx.arg {
            Sarg::Eq(lhs, _) => lhs,
            Sarg::Range(lhs, _, _) => lhs,
        };
        let cols = ColList::from_iter(std::iter::once(start).chain(idx.prefix.iter().map(|x| x.0)));
        Self::new(index, table, cols)
    }
}

/// A line of output, representing a step in the plan
#[derive(Debug)]
enum Line<'a> {
    TableScan {
        table: &'a str,
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
        index: PrintIndex<'a>,
        ident: u16,
    },
    IxJoin {
        semi: Semi,
        rhs: String,
        ident: u16,
    },
    HashJoin {
        semi: Semi,
        ident: u16,
    },
    HashBuild {
        cond: Field<'a>,
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
    Union {
        ident: u16,
    },
    Project {
        output: Output<'a>,
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
    Output {
        output: Output<'a>,
        ident: u16,
    },
}

impl Line<'_> {
    fn ident(&self) -> usize {
        let ident = match self {
            Line::TableScan { ident, .. } => *ident,
            Line::Filter { ident, .. } => *ident,
            Line::FilterIxScan { ident, .. } => *ident,
            Line::IxScan { ident, .. } => *ident,
            Line::IxJoin { ident, .. } => *ident,
            Line::HashJoin { ident, .. } => *ident,
            Line::HashBuild { ident, .. } => *ident,
            Line::NlJoin { ident, .. } => *ident,
            Line::JoinExpr { ident, .. } => *ident,
            Line::Limit { ident, .. } => *ident,
            Line::Count { ident, .. } => *ident,
            Line::Insert { ident, .. } => *ident,
            Line::Update { ident, .. } => *ident,
            Line::Delete { ident, .. } => *ident,
            Line::Output { ident, .. } => *ident,
            Line::Project { ident, .. } => *ident,
            Line::Union { ident, .. } => *ident,
        };
        ident as usize
    }
}

/// A `field` in a `table`
#[derive(Debug, Clone)]
struct Field<'a> {
    table: &'a str,
    field: &'a str,
}

/// The output of the plan, aka the projected columns
#[derive(Debug, Clone)]
enum Output<'a> {
    Fields(Vec<Field<'a>>),
    Alias(&'a str),
    Aliases(Vec<&'a str>),
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

    fn merge(self, rhs: Output<'a>) -> Output<'a> {
        match (self, rhs) {
            (Output::Fields(lhs), Output::Fields(rhs)) => {
                Output::Fields(lhs.iter().chain(rhs.iter()).cloned().collect())
            }
            (Output::Alias(lhs), Output::Alias(rhs)) => Output::Aliases(vec![lhs, rhs]),
            (Output::Aliases(lhs), Output::Alias(rhs)) => {
                let mut lhs = lhs;
                lhs.push(rhs);
                Output::Aliases(lhs)
            }
            (Output::Aliases(lhs), Output::Aliases(rhs)) => {
                let mut lhs = lhs;
                lhs.extend(rhs);
                Output::Aliases(lhs)
            }
            (Output::Empty, Output::Empty) => Output::Empty,
            (Output::Empty, x) => x,
            (x, Output::Empty) => x,
            _ => {
                unreachable!()
            }
        }
    }
}

/// A list of lines to print
struct Lines<'a> {
    lines: Vec<Line<'a>>,
    labels: Labels<'a>,
    /// A map of label to table name or alias
    vars: HashMap<usize, &'a str>,
}

impl<'a> Lines<'a> {
    fn new(vars: HashMap<usize, &'a str>) -> Self {
        Self {
            lines: Vec::new(),
            labels: Labels::new(),
            vars,
        }
    }

    fn add(&mut self, line: Line<'a>) {
        self.lines.push(line)
    }

    /// Resolve the label to the [`TableSchema`], and add it to the list of labels
    fn add_table(&mut self, label: Label, table: &'a TableSchema) {
        let name = self.vars.get(&label.0).copied().unwrap_or(table.table_name.as_ref());
        self.labels.insert(label, table, name);
    }
}

/// Determine the output of a join using the direction of the [`Semi`] join
fn output_join<'a>(lines: &Lines<'a>, semi: Semi, label_lhs: Label, label_rhs: Label) -> Output<'a> {
    match semi {
        Semi::Lhs => {
            let schema = lines.labels.table_by_label(&label_lhs).unwrap();
            Output::Fields(Output::fields(schema))
        }
        Semi::Rhs => {
            let schema = lines.labels.table_by_label(&label_rhs).unwrap();
            Output::Fields(Output::fields(schema))
        }
        Semi::All => {
            let schema = lines.labels.table_by_label(&label_lhs).unwrap();
            let lhs = Output::fields(schema);
            let schema = lines.labels.table_by_label(&label_rhs).unwrap();
            let rhs = Output::fields(schema);

            Output::Fields(lhs.iter().chain(rhs.iter()).cloned().collect())
        }
    }
}

/// The physical plan to print, with their schemas resolved
enum PrinterPlan<'a> {
    TableScan {
        scan: &'a TableScan,
        schema: Schema<'a>,
        output: Output<'a>,
    },
    IxScan {
        idx: &'a IxScan,
        label: Label,
        index: &'a IndexSchema,
        schema: Schema<'a>,
        output: Output<'a>,
    },
    IxJoin {
        idx: &'a IxJoin,
        plan: Box<PrinterPlan<'a>>,
        semi: Semi,
        lhs: Field<'a>,
        rhs: Field<'a>,
        output: Output<'a>,
    },
    HashJoin {
        semi: Semi,
        lhs: Box<PrinterPlan<'a>>,
        rhs: Box<PrinterPlan<'a>>,
        lhs_field: Field<'a>,
        rhs_field: Field<'a>,
        unique: bool,
        output: Output<'a>,
    },
    NLJoin {
        lhs: Box<PrinterPlan<'a>>,
        rhs: Box<PrinterPlan<'a>>,
        output: Output<'a>,
    },
    Filter {
        plan: Box<PrinterPlan<'a>>,
        expr: &'a PhysicalExpr,
        output: Output<'a>,
    },
    Project {
        plan: Box<PrinterPlan<'a>>,
        output: Output<'a>,
    },
    Limit {
        plan: Box<PrinterPlan<'a>>,
        limit: u64,
        output: Output<'a>,
    },
    Agg {
        plan: Box<PrinterPlan<'a>>,
        agg: &'a AggType,
        output: Output<'a>,
    },
    Insert {
        plan: &'a InsertPlan,
        output: Output<'a>,
    },
    Update {
        plan: &'a UpdatePlan,
        filter: Box<PrinterPlan<'a>>,
        output: Output<'a>,
    },
    Delete {
        table: &'a TableSchema,
        filter: Box<PrinterPlan<'a>>,
        output: Output<'a>,
    },
    Union {
        plans: Vec<PrinterPlan<'a>>,
        output: Output<'a>,
    },
}

impl<'a> PrinterPlan<'a> {
    fn output(&self) -> Output<'a> {
        match self {
            Self::TableScan { output, .. } => output,
            Self::IxScan { output, .. } => output,
            Self::IxJoin { output, .. } => output,
            Self::HashJoin { output, .. } => output,
            Self::NLJoin { output, .. } => output,
            Self::Filter { output, .. } => output,
            Self::Project { output, .. } => output,
            Self::Limit { output, .. } => output,
            Self::Agg { output, .. } => output,
            Self::Insert { output, .. } => output,
            Self::Update { output, .. } => output,
            Self::Delete { output, .. } => output,
            Self::Union { output, .. } => output,
        }
        .clone()
    }
}

/// Resolve the schemas and labels of the physical plan, so we can print them before the children
fn scan_tables<'a>(lines: &mut Lines<'a>, plan: &'a PhysicalPlan) -> PrinterPlan<'a> {
    match plan {
        PhysicalPlan::TableScan(scan, label) => {
            lines.add_table(*label, &scan.schema);
            let schema = lines.labels.table_by_label(label).unwrap();
            let output = Output::Fields(Output::fields(schema));
            PrinterPlan::TableScan {
                scan,
                schema: schema.clone(),
                output,
            }
        }
        PhysicalPlan::IxScan(idx, label) => {
            lines.add_table(*label, &idx.schema);
            let schema = lines.labels.table_by_label(label).unwrap();
            let output = Output::Fields(Output::fields(schema));

            let index = idx.schema.indexes.iter().find(|x| x.index_id == idx.index_id).unwrap();

            PrinterPlan::IxScan {
                idx,
                label: *label,
                index,
                schema: schema.clone(),
                output,
            }
        }
        PhysicalPlan::IxJoin(idx, semi) => {
            lines.add_table(idx.rhs_label, &idx.rhs);
            let plan = scan_tables(lines, &idx.lhs);
            let output = output_join(lines, *semi, idx.lhs_field.label, idx.rhs_label);

            let lhs = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs = lines.labels.label(&idx.rhs_label, idx.rhs_field).unwrap();

            PrinterPlan::IxJoin {
                idx,
                plan: Box::new(plan),
                semi: *semi,
                lhs,
                rhs,
                output,
            }
        }
        PhysicalPlan::HashJoin(idx, semi) => {
            let lhs = scan_tables(lines, &idx.lhs).into();
            let rhs = scan_tables(lines, &idx.rhs).into();
            let output = output_join(lines, *semi, idx.lhs_field.label, idx.rhs_field.label);

            let lhs_field = lines.labels.field(&idx.lhs_field).unwrap();
            let rhs_field = lines.labels.field(&idx.rhs_field).unwrap();

            PrinterPlan::HashJoin {
                semi: *semi,
                lhs,
                rhs,
                lhs_field,
                rhs_field,
                unique: idx.unique,
                output,
            }
        }
        PhysicalPlan::NLJoin(lhs, rhs) => {
            let lhs = scan_tables(lines, lhs);
            let output = lhs.output();
            let rhs = scan_tables(lines, rhs);
            let output = output.merge(rhs.output());
            PrinterPlan::NLJoin {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                output,
            }
        }
        PhysicalPlan::Filter(plan, expr) => {
            let plan = Box::new(scan_tables(lines, plan));
            let output = plan.output();
            PrinterPlan::Filter { plan, expr, output }
        }
    }
}

fn scan_tables_project<'a>(lines: &mut Lines<'a>, plan: &'a ProjectListPlan) -> PrinterPlan<'a> {
    match plan {
        ProjectListPlan::Name(plans) => {
            let plans: Vec<_> = plans.iter().map(|plan| scan_tables(lines, plan)).collect();
            PrinterPlan::Union {
                output: plans.last().map(|x| x.output()).unwrap_or(Output::Empty),
                plans,
            }
        }
        ProjectListPlan::List(plans, fields) => {
            let plans = plans.iter().map(|plan| scan_tables(lines, plan)).collect_vec();
            let plan = PrinterPlan::Union {
                output: Output::Fields(Output::tuples(fields, lines)),
                plans,
            };
            PrinterPlan::Project {
                plan: Box::new(plan),
                output: Output::Fields(Output::tuples(fields, lines)),
            }
        }
        ProjectListPlan::Limit(plan, limit) => {
            let plan = scan_tables_project(lines, plan);
            let output = plan.output();
            PrinterPlan::Limit {
                plan: Box::new(plan),
                limit: *limit,
                output,
            }
        }
        ProjectListPlan::Agg(plans, agg) => {
            let plans = plans.iter().map(|plan| scan_tables(lines, plan)).collect_vec();

            let plan = PrinterPlan::Union {
                output: plans.last().map(|x| x.output()).unwrap_or(Output::Empty),
                plans,
            };
            let output = plan.output();
            PrinterPlan::Agg {
                plan: Box::new(plan),
                agg,
                output,
            }
        }
    }
}

/// Resolve the schemas and labels of the physical plan, so we can print them before the children
fn scan_tables_dml<'a>(lines: &mut Lines<'a>, plan: &'a MutationPlan) -> PrinterPlan<'a> {
    match plan {
        MutationPlan::Insert(plan) => PrinterPlan::Insert {
            plan,
            output: Output::Empty,
        },
        MutationPlan::Delete(plan) => {
            let filter = scan_tables(lines, &plan.filter);

            PrinterPlan::Delete {
                table: &plan.table,
                filter: Box::new(filter),
                output: Output::Empty,
            }
        }
        MutationPlan::Update(plan) => {
            let filter = scan_tables(lines, &plan.filter);

            PrinterPlan::Update {
                plan,
                filter: Box::new(filter),
                output: Output::Empty,
            }
        }
    }
}

fn add_limit<'a>(lines: &mut Lines<'a>, output: Output<'a>, limit: Option<u64>, ident: u16) -> u16 {
    if let Some(limit) = limit {
        lines.add(Line::Limit { limit, ident: 0 });
        lines.add(Line::Output {
            output: output.clone(),
            ident: ident + 2,
        });
        ident + 2
    } else {
        ident
    }
}
fn eval_plan<'a>(lines: &mut Lines<'a>, plan: PrinterPlan<'a>, ident: u16) {
    match plan {
        PrinterPlan::TableScan { scan, schema, output } => {
            let ident = add_limit(lines, output.clone(), scan.limit, ident);

            lines.add(Line::TableScan {
                table: schema.name,
                ident,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });
        }
        PrinterPlan::IxScan {
            idx,
            label,
            index,
            schema,
            output,
        } => {
            let ident = add_limit(lines, output.clone(), idx.limit, ident);

            let index = PrintIndex::from_scan(idx, index, schema.table);

            lines.add(Line::IxScan { index, ident });

            lines.add(Line::FilterIxScan {
                idx,
                label,
                ident: ident + 2,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });
        }
        PrinterPlan::IxJoin {
            idx,
            plan,
            semi,
            lhs,
            rhs,
            output,
        } => {
            let schema = lines.labels.table_by_label(&idx.rhs_label).unwrap();
            let rhs_name = schema.name.to_string();

            lines.add(Line::IxJoin {
                semi,
                ident,
                rhs: rhs_name,
            });

            lines.add(Line::JoinExpr {
                unique: idx.unique,
                lhs,
                rhs,
                ident: ident + 2,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *plan, ident + 2);
        }
        PrinterPlan::HashJoin {
            semi,
            lhs,
            rhs,
            lhs_field,
            rhs_field,
            unique,
            output,
        } => {
            lines.add(Line::HashJoin { semi, ident });
            lines.add(Line::JoinExpr {
                unique,
                lhs: lhs_field,
                rhs: rhs_field.clone(),
                ident: ident + 2,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *lhs, ident + 2);

            lines.add(Line::HashBuild {
                // The physical plan build the hash with the rhs
                // TODO: This should be a explicit step in the plan
                cond: rhs_field.clone(),
                ident: ident + 2,
            });

            eval_plan(lines, *rhs, ident + 4);
        }
        PrinterPlan::NLJoin { lhs, rhs, output } => {
            lines.add(Line::NlJoin { ident });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *lhs, ident + 2);
            eval_plan(lines, *rhs, ident + 2);
        }
        PrinterPlan::Filter { plan, expr, output: _ } => {
            eval_plan(lines, *plan, ident);
            lines.add(Line::Filter { expr, ident: ident + 2 });
        }
        PrinterPlan::Project { plan, output } => {
            lines.add(Line::Project {
                output: output.clone(),
                ident,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *plan, ident + 2);
        }
        PrinterPlan::Limit { plan, limit, output } => {
            let ident = add_limit(lines, output.clone(), Some(limit), ident);

            eval_plan(lines, *plan, ident);
        }
        PrinterPlan::Agg { plan, agg, output: _ } => {
            match agg {
                AggType::Count { alias } => {
                    lines.add(Line::Count { ident });
                    lines.add(Line::Output {
                        output: Output::Alias(alias),
                        ident: ident + 2,
                    });
                }
            }

            eval_plan(lines, *plan, ident + 2);
        }

        PrinterPlan::Insert { plan, output } => {
            let schema = &plan.table;

            lines.add(Line::Insert {
                table_name: &schema.table_name,
                ident,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });
        }
        PrinterPlan::Update { plan, filter, output } => {
            let schema = &plan.table;

            lines.add(Line::Update {
                table_name: &schema.table_name,
                columns: Output::fields_update(schema, &plan.columns),
                ident,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *filter, ident + 2);
        }
        PrinterPlan::Delete { table, filter, output } => {
            let schema = table;

            lines.add(Line::Delete {
                table_name: &schema.table_name,
                ident,
            });

            lines.add(Line::Output {
                output,
                ident: ident + 2,
            });

            eval_plan(lines, *filter, ident + 2);
        }
        PrinterPlan::Union { mut plans, output } => match plans.len() {
            1 => {
                let plan = plans.drain(..).next().unwrap();
                eval_plan(lines, plan, ident);
            }
            _ => {
                lines.add(Line::Union { ident });

                for plan in plans {
                    eval_plan(lines, plan, ident + 2);
                }

                lines.add(Line::Output {
                    output,
                    ident: ident + 2,
                });
            }
        },
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
    options: ExplainOptions,
}

impl<'a> Explain<'a> {
    pub fn new(ctx: &'a PhysicalCtx<'a>) -> Self {
        Self {
            ctx,
            lines: Vec::new(),
            labels: Labels::new(),
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
        let plan = match &self.ctx.plan {
            PlanCtx::ProjectList(plan) => scan_tables_project(&mut lines, plan),
            PlanCtx::DML(plan) => scan_tables_dml(&mut lines, plan),
        };
        eval_plan(&mut lines, plan, 0);

        lines
    }

    /// Build the `Explain` output
    pub fn build(self) -> Self {
        let lines = self.lines();
        Self {
            lines: lines.lines,
            labels: lines.labels,
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
        let op = self.expr.to_op();
        let value = self.expr.to_value();
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
            Sarg::Range(col, _, _) => {
                let col = self.labels.label(&self.label, *col).unwrap();
                write!(f, "{col} {op} {:?}", value)
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
        write!(
            f,
            "{} {}({}) on {}",
            self.name,
            if self.unique { "Unique" } else { "" },
            self.cols.iter().join(", "),
            self.table,
        )
    }
}

impl fmt::Display for Output<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Output::Fields(fields) => write!(f, "{}", fields.iter().join(", ")),
            Output::Alias(alias) => write!(f, "{}", alias),
            Output::Aliases(aliases) => write!(f, "{}", aliases.iter().join(", ")),
            Output::Empty => write!(f, "void"),
        }
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

        for (pos, line) in self.lines.iter().enumerate() {
            let ident = line.ident();
            // To properly align the arrows, we need to determine the level of the ident
            // 0=0, 2=1, 4=2, 8=3, ..., then we subtract the extra space from the arrow
            let level = ident / 2;
            let ident = ident + level;
            let ident = if ident >= 2 { ident - 1 } else { ident };

            let pad = " ".repeat(ident);
            let arrow = if ident >= 2 { format!("{}-> ", pad) } else { pad.clone() };

            match line {
                Line::TableScan { table, ident: _ } => {
                    write!(f, "{arrow}Seq Scan on {table}")?;
                }
                Line::IxScan { index, ident: _ } => {
                    write!(f, "{arrow}Index Scan using {index}")?;
                }
                Line::Filter { expr, ident: _ } => {
                    write!(f, "{arrow}Filter: ")?;
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
                    write!(f, "{arrow}Index Join: {semi:?} on {rhs}")?;
                }
                Line::HashJoin { semi, ident: _ } => match semi {
                    Semi::All => {
                        write!(f, "{arrow}Hash Join")?;
                    }
                    semi => {
                        write!(f, "{arrow}Hash Join: {semi:?}")?;
                    }
                },
                Line::HashBuild { cond, ident: _ } => {
                    write!(f, "{arrow}Hash Build: {cond}")?;
                }
                Line::NlJoin { ident: _ } => {
                    write!(f, "{arrow}Nested Loop")?;
                }
                Line::JoinExpr {
                    unique,
                    lhs,
                    rhs,
                    ident: _,
                } => {
                    writeln!(f, "{pad}Inner Unique: {unique}")?;
                    write!(f, "{pad}Join Cond: ({} = {})", lhs, rhs)?;
                }
                Line::Union { ident: _ } => {
                    write!(f, "{arrow}Union")?;
                }
                Line::Limit { limit, ident: _ } => {
                    write!(f, "{arrow}Limit: {limit}")?;
                }
                Line::Count { ident: _ } => {
                    write!(f, "{arrow}Count")?;
                }
                Line::Insert { table_name, ident: _ } => {
                    write!(f, "{arrow}Insert on {table_name}")?;
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
                    write!(f, "{arrow}Update on {table_name} SET ({columns })")?;
                }
                Line::Delete { table_name, ident: _ } => {
                    write!(f, "{arrow}Delete on {table_name}")?;
                }
                Line::Output {
                    output: columns,
                    ident: _,
                } => {
                    write!(f, "{pad}Output: {columns}")?;
                }
                Line::Project { output, ident: _ } => {
                    write!(f, "{arrow}Project: {output}")?;
                }
            }

            if self.options.show_timings || self.options.show_schema || pos < self.lines.len() - 1 {
                writeln!(f)?;
            }
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
                        .map(|x| PrintIndex::from_index(x, schema.table))
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
