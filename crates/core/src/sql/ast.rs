use crate::config::ReadConfigOption;
use crate::db::engine::{MutTx, DatabaseEngine, Tx};
use crate::error::{DBError, PlanError};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_primitives::{ColList, ConstraintKind, Constraints};
use spacetimedb_sats::db::def::{ColumnDef, ConstraintDef, TableDef, TableSchema};
use spacetimedb_sats::db::error::RelationError;
use spacetimedb_sats::relation::{FieldExpr, FieldName};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::expr::{ColumnOp, DbType, Expr};
use spacetimedb_vm::operator::{OpCmp, OpLogic, OpQuery};
use spacetimedb_vm::ops::parse::{parse, parse_simple_enum};
use sqlparser::ast::{
    Assignment, BinaryOperator, ColumnDef as SqlColumnDef, ColumnOption, DataType, ExactNumberInfo, Expr as SqlExpr,
    GeneratedAs, HiveDistributionStyle, Ident, JoinConstraint, JoinOperator, ObjectName, ObjectType, Query, Select,
    SelectItem, SetExpr, Statement, TableFactor, TableWithJoins, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::str::FromStr;
use std::sync::Arc;

/// Simplify to detect features of the syntax we don't support yet
/// Because we use [PostgreSqlDialect] in the compiler step it already protect against features
/// that are not in the standard SQL-92 but still need to check for completeness
trait Unsupported {
    fn unsupported(&self) -> bool;
}

impl Unsupported for bool {
    fn unsupported(&self) -> bool {
        *self
    }
}

impl<T> Unsupported for Option<T> {
    fn unsupported(&self) -> bool {
        self.is_some()
    }
}

impl<T> Unsupported for Vec<T> {
    fn unsupported(&self) -> bool {
        !self.is_empty()
    }
}

impl Unsupported for HiveDistributionStyle {
    fn unsupported(&self) -> bool {
        !matches!(self, HiveDistributionStyle::NONE)
    }
}

impl Unsupported for sqlparser::ast::GroupByExpr {
    fn unsupported(&self) -> bool {
        match self {
            sqlparser::ast::GroupByExpr::All => true,
            sqlparser::ast::GroupByExpr::Expressions(v) => v.unsupported(),
        }
    }
}

macro_rules! unsupported {
    ($name:literal,$a:expr)=>{{
        let name = stringify!($name);
        let it = stringify!($a);
        if $a.unsupported() {
            return Err(PlanError::Unsupported {
                feature: format!("Unsupported {name} with `{it}` feature."),

            });
        }
    }};
    ($name:literal,$($a:expr),+$(,)?)=> {{
        $(unsupported!($name,$a);)+
    }};
}

/// A convenient wrapper for a table name (that comes from an `ObjectName`).
pub struct Table {
    pub(crate) name: Box<str>,
}

impl Table {
    pub fn new(name: ObjectName) -> Self {
        Self {
            name: name.to_string().into(),
        }
    }
}

#[derive(Debug)]
pub enum Column {
    /// Any expression, not followed by `[ AS ] alias`
    UnnamedExpr(Expr),
    /// An qualified `table.*`
    QualifiedWildcard { table: String },
    /// An unqualified `SELECT *`
    Wildcard,
}

/// The list of expressions for `SELECT expr1, expr2...` determining what data to extract.
#[derive(Debug, Clone)]
pub struct Selection {
    pub(crate) clause: ColumnOp,
}

impl Selection {
    pub fn with_cmp(op: OpQuery, lhs: ColumnOp, rhs: ColumnOp) -> Self {
        let cmp = ColumnOp::new(op, lhs, rhs);
        Selection { clause: cmp }
    }
}

#[derive(Debug)]
pub struct OnExpr {
    pub op: OpCmp,
    pub lhs: FieldName,
    pub rhs: FieldName,
}

/// The `JOIN [INNER] ON join_expr OpCmp join_expr` clause
#[derive(Debug)]
pub enum Join {
    Inner { rhs: Arc<TableSchema>, on: OnExpr },
}

/// The list of tables in `... FROM table1 [JOIN table2] ...`
#[derive(Debug)]
pub struct From {
    pub root: Arc<TableSchema>,
    pub joins: Vec<Join>,
}

impl From {
    pub fn new(root: Arc<TableSchema>) -> Self {
        Self {
            root,
            joins: Vec::new(),
        }
    }

    pub fn with_inner_join(mut self, rhs: Arc<TableSchema>, on: OnExpr) -> Self {
        // Check if the field are inverted:
        // FROM t1 JOIN t2 ON t2.id = t1.id
        let on = if on.rhs.table() == self.root.table_id && self.root.get_column_by_field(on.rhs).is_some() {
            OnExpr {
                op: on.op.reverse(),
                lhs: on.rhs,
                rhs: on.lhs,
            }
        } else {
            on
        };

        self.joins.push(Join::Inner { rhs, on });
        self
    }

    /// Returns all the tables, including the ones inside the joins
    pub fn iter_tables(&self) -> impl Clone + Iterator<Item = &TableSchema> {
        [&*self.root]
            .into_iter()
            .chain(self.joins.iter().map(|Join::Inner { rhs, .. }| &**rhs))
    }

    /// Returns all the table names as a `Vec<String>`, including the ones inside the joins.
    pub fn table_names(&self) -> Vec<Box<str>> {
        self.iter_tables().map(|x| x.table_name.clone()).collect()
    }

    /// Returns the field matching `f` looking in `tables`.
    ///
    /// See [`find_field`] for more details.
    pub(super) fn find_field(&self, f: &str) -> Result<(FieldName, &AlgebraicType), PlanError> {
        find_field(self.iter_tables(), f)
    }
}

/// Returns the field matching `f` looking in `tables`
/// for `{table_name}.{field_name}` (qualified) or `{field_name}`.
///
/// # Errors
///
/// If the field is not fully qualified by the user,
/// it may lead to duplicates, causing ambiguity.
/// For example, in the query `WHERE a = lhs.a AND rhs.a = a`,
/// the fields `['lhs.a', 'rhs.a', 'a']` are ambiguous.
///
/// Returns an error if no fields match `f` (`PlanError::UnknownField`)
/// or if the field is ambiguous due to multiple matches (`PlanError::AmbiguousField`).
pub fn find_field<'a>(
    mut tables: impl Clone + Iterator<Item = &'a TableSchema>,
    f: &str,
) -> Result<(FieldName, &'a AlgebraicType), PlanError> {
    fn extract_table_field(ident: &str) -> Result<(Option<&str>, &str), RelationError> {
        let mut iter = ident.rsplit('.');
        let field = iter.next();
        let table = iter.next();
        let more = iter.next();
        match (field, table, more) {
            (Some(field), table, None) => Ok((table, field)),
            _ => Err(RelationError::FieldPathInvalid(ident.to_string())),
        }
    }

    let (f_table, f_field) = extract_table_field(f)?;

    let tables2 = tables.clone();
    let unknown_field = || {
        let field = match f_table {
            Some(f_table) => format!("{f_table}.{f_field}"),
            None => f_field.into(),
        };
        let tables = tables2.map(|t| t.table_name.clone()).collect();
        Err(PlanError::UnknownField { field, tables })
    };

    if let Some(f_table) = f_table {
        // Qualified field `{f_table}.{f_field}`.
        // Narrow search to first table with name `f_table`.
        return if let Some(col) = tables
            .find(|t| &*t.table_name == f_table)
            .and_then(|t| t.get_column_by_name(f_field))
        {
            Ok((FieldName::new(col.table_id, col.col_pos), &col.col_type))
        } else {
            unknown_field()
        };
    }

    // Unqualified field `{f_field}`.
    // Find all columns with a matching name.
    let mut fields = tables
        .flat_map(|t| t.columns().iter().map(move |col| (t, col)))
        .filter(|(_, col)| &*col.col_name == f_field);

    // When there's a single candidate, we've found our match.
    // Otherwise, if are none or several candidates, error.
    match (fields.next(), fields.next()) {
        (None, _) => unknown_field(),
        (Some((_, col)), None) => Ok((FieldName::new(col.table_id, col.col_pos), &col.col_type)),
        (Some(f1), Some(f2)) => {
            let found = [f1, f2]
                .into_iter()
                .chain(fields)
                .map(|(table, column)| format!("{0}.{1}", &table.table_name, &column.col_name))
                .collect();
            Err(PlanError::AmbiguousField { field: f.into(), found })
        }
    }
}

/// Defines the portions of the `SQL` standard that we support.
#[derive(Debug)]
pub enum SqlAst {
    Select {
        from: From,
        project: Vec<Column>,
        selection: Option<Selection>,
    },
    Insert {
        table: Arc<TableSchema>,
        columns: Vec<FieldName>,
        values: Vec<Vec<FieldExpr>>,
    },
    Update {
        table: Arc<TableSchema>,
        assignments: HashMap<FieldName, FieldExpr>,
        selection: Option<Selection>,
    },
    Delete {
        table: Arc<TableSchema>,
        selection: Option<Selection>,
    },
    CreateTable {
        table: TableDef,
    },
    Drop {
        name: String,
        kind: DbType,
    },
    SetVar {
        name: String,
        value: AlgebraicValue,
    },
    ReadVar {
        name: String,
    },
}

fn extract_field<'a>(
    tables: impl Clone + Iterator<Item = &'a TableSchema>,
    of: &SqlExpr,
) -> Result<Option<&'a AlgebraicType>, PlanError> {
    match of {
        SqlExpr::Identifier(x) => find_field(tables, &x.value).map(|(_, ty)| Some(ty)),
        SqlExpr::CompoundIdentifier(ident) => {
            let col_name = compound_ident(ident);
            find_field(tables, &col_name).map(|(_, ty)| Some(ty))
        }
        _ => Ok(None),
    }
}

/// Parses `value` according to the type of the field, as provided by `field`.
///
/// When `field` is `None`, the type is inferred to an integer or float depending on if a `.` separator is present.
/// The `is_long` parameter decides whether to parse as a 64-bit type or a 32-bit one.
fn infer_number(field: Option<&AlgebraicType>, value: &str, is_long: bool) -> Result<AlgebraicValue, ErrorVm> {
    match field {
        None => {
            let ty = if value.contains('.') {
                if is_long {
                    AlgebraicType::F64
                } else {
                    AlgebraicType::F32
                }
            } else if is_long {
                AlgebraicType::I64
            } else {
                AlgebraicType::I32
            };
            parse(value, &ty)
        }
        Some(f) => parse(value, f),
    }
}

/// `Enums` in `sql` are simple strings like `Player` that must be inferred by their type.
///
/// If `field` is a `simple enum` it looks for the `tag` specified by `value`, else it should be a plain `String`.
fn infer_str_or_enum(field: Option<&AlgebraicType>, value: String) -> Result<AlgebraicValue, ErrorVm> {
    if let Some(sum) = field.and_then(|x| x.as_sum()) {
        parse_simple_enum(sum, &value)
    } else {
        Ok(AlgebraicValue::String(value.into()))
    }
}

/// Parses `name` as a [ReadConfigOption] and then parse the numeric value.
fn infer_config(name: &str, value: &str, is_long: bool) -> Result<AlgebraicValue, ErrorVm> {
    let config = ReadConfigOption::from_str(name)?;
    infer_number(Some(&config.type_of()), value, is_long)
}

/// Compiles a [SqlExpr] expression into a [ColumnOp]
fn compile_expr_value<'a>(
    tables: impl Clone + Iterator<Item = &'a TableSchema>,
    field: Option<&'a AlgebraicType>,
    of: SqlExpr,
) -> Result<ColumnOp, PlanError> {
    Ok(ColumnOp::Field(match of {
        SqlExpr::Identifier(name) => FieldExpr::Name(find_field(tables, &name.value)?.0),
        SqlExpr::CompoundIdentifier(ident) => {
            let col_name = compound_ident(&ident);
            FieldExpr::Name(find_field(tables, &col_name)?.0)
        }
        SqlExpr::Value(x) => FieldExpr::Value(match x {
            Value::Number(value, is_long) => infer_number(field, &value, is_long)?,
            Value::SingleQuotedString(s) => infer_str_or_enum(field, s)?,
            Value::DoubleQuotedString(s) => AlgebraicValue::String(s.into()),
            Value::HexStringLiteral(s) => infer_number(field, &s, false)?,
            Value::Boolean(x) => AlgebraicValue::Bool(x),
            Value::Null => AlgebraicValue::OptionNone(),
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Unsupported value: {x}."),
                });
            }
        }),
        SqlExpr::BinaryOp { left, op, right } => {
            let (op, lhs, rhs) = compile_bin_op(tables, op, left, right)?;

            return Ok(ColumnOp::new(op, lhs, rhs));
        }
        SqlExpr::Nested(x) => {
            return compile_expr_value(tables, field, *x);
        }
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("Unsupported expression: {x}"),
            });
        }
    }))
}

fn compile_expr_field(table: &From, field: Option<&AlgebraicType>, of: SqlExpr) -> Result<FieldExpr, PlanError> {
    match compile_expr_value(table.iter_tables(), field, of)? {
        ColumnOp::Field(field) => Ok(field),
        x => Err(PlanError::Unsupported {
            feature: format!("Complex expression {x} on insert..."),
        }),
    }
}

/// Compiles the [Table] from a section of `SQL` that describes a table clause.
fn compile_table_factor(table: TableFactor) -> Result<Table, PlanError> {
    match table {
        TableFactor::Table {
            name,
            alias,
            args,
            with_hints,
            version,
            partitions,
        } => {
            unsupported!("TableFactor", alias, args, with_hints, version, partitions);

            Ok(Table::new(name))
        }
        x => Err(PlanError::Unsupported {
            feature: format!("TableFactor with syntax {x:?} not supported"),
        }),
    }
}

/// Compiles a binary operation like `field > 1`
fn compile_bin_op<'a>(
    tables: impl Clone + Iterator<Item = &'a TableSchema>,
    op: BinaryOperator,
    lhs: Box<sqlparser::ast::Expr>,
    rhs: Box<sqlparser::ast::Expr>,
) -> Result<(OpQuery, ColumnOp, ColumnOp), PlanError> {
    let op: OpQuery = match op {
        BinaryOperator::Gt => OpCmp::Gt.into(),
        BinaryOperator::Lt => OpCmp::Lt.into(),
        BinaryOperator::GtEq => OpCmp::GtEq.into(),
        BinaryOperator::LtEq => OpCmp::LtEq.into(),
        BinaryOperator::Eq => OpCmp::Eq.into(),
        BinaryOperator::NotEq => OpCmp::NotEq.into(),
        BinaryOperator::And => OpLogic::And.into(),
        BinaryOperator::Or => OpLogic::Or.into(),
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("BinaryOperator not supported in WHERE: {x}."),
            });
        }
    };

    let field_lhs = extract_field(tables.clone(), &lhs)?;
    let field_rhs = extract_field(tables.clone(), &rhs)?;
    // This inversion is for inferring the type of the right side, like in `inventory.id = 1`,
    // so `1` get the type of `inventory.id`
    let lhs = compile_expr_value(tables.clone(), field_rhs, *lhs)?;
    let rhs = compile_expr_value(tables, field_lhs, *rhs)?;

    Ok((op, lhs, rhs))
}

fn _compile_where(table: &From, filter: SqlExpr) -> Result<Option<Selection>, PlanError> {
    match filter {
        SqlExpr::BinaryOp { left, op, right } => {
            let (op, lhs, rhs) = compile_bin_op(table.iter_tables(), op, left, right)?;

            Ok(Some(Selection::with_cmp(op, lhs, rhs)))
        }
        SqlExpr::Nested(x) => _compile_where(table, *x),
        x => Err(PlanError::Unsupported {
            feature: format!("Unsupported in WHERE: {x}."),
        }),
    }
}

/// Compiles the `WHERE` clause
fn compile_where(table: &From, filter: Option<SqlExpr>) -> Result<Option<Selection>, PlanError> {
    if let Some(filter) = filter {
        _compile_where(table, filter)
    } else {
        Ok(None)
    }
}

pub trait TableSchemaView {
    fn find_table(&self, db: &DatabaseEngine, t: Table) -> Result<Arc<TableSchema>, PlanError>;
}

impl TableSchemaView for Tx {
    fn find_table(&self, db: &DatabaseEngine, t: Table) -> Result<Arc<TableSchema>, PlanError> {
        let table_id = db
            .table_id_from_name(self, &t.name)?
            .ok_or(PlanError::UnknownTable { table: t.name.clone() })?;
        if !db.table_id_exists(self, &table_id) {
            return Err(PlanError::UnknownTable { table: t.name });
        }
        db.schema_for_table(self, table_id)
            .map_err(move |e| PlanError::DatabaseInternal(Box::new(e)))
    }
}

impl TableSchemaView for MutTx {
    fn find_table(&self, db: &DatabaseEngine, t: Table) -> Result<Arc<TableSchema>, PlanError> {
        let table_id = db
            .table_id_from_name_mut(self, &t.name)?
            .ok_or(PlanError::UnknownTable { table: t.name.clone() })?;
        if !db.table_id_exists_mut(self, &table_id) {
            return Err(PlanError::UnknownTable { table: t.name });
        }
        db.schema_for_table_mut(self, table_id)
            .map_err(|e| PlanError::DatabaseInternal(Box::new(e)))
    }
}

/// Compiles the `FROM` clause
fn compile_from<T: TableSchemaView>(db: &DatabaseEngine, tx: &T, from: &[TableWithJoins]) -> Result<From, PlanError> {
    if from.len() > 1 {
        return Err(PlanError::Unsupported {
            feature: "Multiple tables in `FROM`.".into(),
        });
    }

    let root_table = match from.first() {
        Some(root_table) => root_table,
        None => {
            return Err(PlanError::Unstructured("Missing `FROM` expression.".into()));
        }
    };

    let t = compile_table_factor(root_table.relation.clone())?;
    let base = tx.find_table(db, t)?;
    let mut base = From::new(base);

    for join in &root_table.joins {
        match &join.join_operator {
            JoinOperator::Inner(constraint) => {
                let t = compile_table_factor(join.relation.clone())?;
                let join = tx.find_table(db, t)?;

                match constraint {
                    JoinConstraint::On(x) => {
                        let tables = base.iter_tables().chain([&*join]);
                        let expr = compile_expr_value(tables, None, x.clone())?;
                        match expr {
                            ColumnOp::Field(_) => {}
                            ColumnOp::Cmp { op, lhs, rhs } => {
                                let op = match op {
                                    OpQuery::Cmp(op) => op,
                                    OpQuery::Logic(op) => {
                                        return Err(PlanError::Unsupported {
                                            feature: format!("Can't use operator {op} on JOIN clause"),
                                        });
                                    }
                                };
                                let (lhs, rhs) = match (*lhs, *rhs) {
                                    (ColumnOp::Field(FieldExpr::Name(lhs)), ColumnOp::Field(FieldExpr::Name(rhs))) => {
                                        (lhs, rhs)
                                    }
                                    (lhs, rhs) => {
                                        return Err(PlanError::Unsupported {
                                            feature: format!(
                                                "Can't compare non-field expressions {lhs} and {rhs} in JOIN clause"
                                            ),
                                        });
                                    }
                                };

                                base = base.with_inner_join(join, OnExpr { op, lhs, rhs })
                            }
                        }
                    }
                    x => {
                        return Err(PlanError::Unsupported {
                            feature: format!("JOIN constrain {x:?} is not valid, can be only on the form Table.Field [Cmp] Table.Field"),
                        });
                    }
                }
            }
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Unsupported JOIN operator: `{x:?}`"),
                });
            }
        }
    }

    Ok(base)
}

fn compound_ident(ident: &[Ident]) -> String {
    ident.iter().map(ToString::to_string).collect::<Vec<_>>().join(".")
}

fn compile_select_item(from: &From, select_item: SelectItem) -> Result<Column, PlanError> {
    match select_item {
        SelectItem::UnnamedExpr(expr) => match expr {
            sqlparser::ast::Expr::Identifier(ident) => {
                let col_name = ident.to_string();

                Ok(Column::UnnamedExpr(Expr::Ident(col_name)))
            }
            sqlparser::ast::Expr::CompoundIdentifier(ident) => {
                let col_name = compound_ident(&ident);

                Ok(Column::UnnamedExpr(Expr::Ident(col_name)))
            }
            sqlparser::ast::Expr::Value(_) => {
                let value = compile_expr_value(from.iter_tables(), None, expr)?;
                match value {
                    ColumnOp::Field(value) => match value {
                        FieldExpr::Name(_) => Err(PlanError::Unsupported {
                            feature: "Should not be an identifier in Expr::Value".to_string(),
                        }),
                        FieldExpr::Value(x) => Ok(Column::UnnamedExpr(Expr::Value(x))),
                    },
                    x => Err(PlanError::Unsupported {
                        feature: format!("Should not be an {x} in Expr::Value"),
                    }),
                }
            }
            sqlparser::ast::Expr::Nested(x) => compile_select_item(from, SelectItem::UnnamedExpr(*x)),
            _ => Err(PlanError::Unsupported {
                feature: "Only columns names & scalars are supported.".into(),
            }),
        },
        SelectItem::ExprWithAlias { expr: _, alias: _ } => Err(PlanError::Unsupported {
            feature: "ExprWithAlias".into(),
        }),
        SelectItem::QualifiedWildcard(ident, _) => Ok(Column::QualifiedWildcard {
            table: ident.to_string(),
        }),
        SelectItem::Wildcard(_) => Ok(Column::Wildcard),
    }
}

/// Compiles the `SELECT ...` clause
fn compile_select<T: TableSchemaView>(db: &DatabaseEngine, tx: &T, select: Select) -> Result<SqlAst, PlanError> {
    let from = compile_from(db, tx, &select.from)?;
    // SELECT ...
    let mut project = Vec::with_capacity(select.projection.len());
    for select_item in select.projection {
        project.push(compile_select_item(&from, select_item)?);
    }

    let selection = compile_where(&from, select.selection)?;

    Ok(SqlAst::Select {
        from,
        project,
        selection,
    })
}

/// Compiles any `query` clause (currently only `SELECT...`)
fn compile_query<T: TableSchemaView>(db: &DatabaseEngine, tx: &T, query: Query) -> Result<SqlAst, PlanError> {
    unsupported!(
        "SELECT",
        query.order_by,
        query.fetch,
        query.limit,
        query.offset,
        query.locks,
        query.with
    );

    match *query.body {
        SetExpr::Select(select) => {
            unsupported!(
                "SELECT",
                select.distinct,
                select.top,
                select.into,
                select.lateral_views,
                select.group_by,
                select.having,
                select.sort_by
            );

            compile_select(db, tx, *select)
        }
        SetExpr::Query(_) => Err(PlanError::Unsupported {
            feature: "Query".into(),
        }),
        SetExpr::SetOperation {
            op: _,
            set_quantifier: _,
            left: _,
            right: _,
        } => Err(PlanError::Unsupported {
            feature: "SetOperation".into(),
        }),
        SetExpr::Values(_) => Err(PlanError::Unsupported {
            feature: "Values".into(),
        }),
        SetExpr::Insert(_) => Err(PlanError::Unsupported {
            feature: "SetExpr::Insert".into(),
        }),
        SetExpr::Update(_) => Err(PlanError::Unsupported {
            feature: "SetExpr::Update".into(),
        }),
        SetExpr::Table(_) => Err(PlanError::Unsupported {
            feature: "SetExpr::Table".into(),
        }),
    }
}

/// Compiles the `INSERT ...` clause
fn compile_insert<T: TableSchemaView>(
    db: &DatabaseEngine,
    tx: &T,
    table_name: ObjectName,
    columns: Vec<Ident>,
    data: &Values,
) -> Result<SqlAst, PlanError> {
    let table = tx.find_table(db, Table::new(table_name))?;

    let table = From::new(table);

    let columns = columns
        .into_iter()
        .map(|x| {
            table
                .find_field(&format!("{}.{}", &table.root.table_name, x))
                .map(|(f, _)| f)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut values = Vec::with_capacity(data.rows.len());

    for x in &data.rows {
        let mut row = Vec::with_capacity(x.len());
        for (pos, v) in x.iter().enumerate() {
            let field_ty = table.root.get_column(pos).map(|col| &col.col_type);
            row.push(compile_expr_field(&table, field_ty, v.clone())?);
        }

        values.push(row);
    }
    Ok(SqlAst::Insert {
        table: table.root,
        columns,
        values,
    })
}

/// Compiles the `UPDATE ...` clause
fn compile_update<T: TableSchemaView>(
    db: &DatabaseEngine,
    tx: &T,
    table: Table,
    assignments: Vec<Assignment>,
    selection: Option<SqlExpr>,
) -> Result<SqlAst, PlanError> {
    let table = From::new(tx.find_table(db, table)?);
    let selection = compile_where(&table, selection)?;

    let mut x = HashMap::with_capacity(assignments.len());

    for col in assignments {
        let name: String = col.id.iter().map(|x| x.to_string()).collect();
        let (field_name, field_ty) = table.find_field(&name)?;

        let value = compile_expr_field(&table, Some(field_ty), col.value)?;
        x.insert(field_name, value);
    }

    Ok(SqlAst::Update {
        table: table.root,
        assignments: x,
        selection,
    })
}

/// Compiles the `DELETE ...` clause
fn compile_delete<T: TableSchemaView>(
    db: &DatabaseEngine,
    tx: &T,
    table: Table,
    selection: Option<SqlExpr>,
) -> Result<SqlAst, PlanError> {
    let table = From::new(tx.find_table(db, table)?);
    let selection = compile_where(&table, selection)?;

    Ok(SqlAst::Delete {
        table: table.root,
        selection,
    })
}

/// Infer the column `size` from the [SqlColumnDef]
fn column_size(column: &SqlColumnDef) -> Option<u64> {
    match column.data_type {
        DataType::Char(x) => x.map(|x| x.length),
        DataType::Varchar(x) => x.map(|x| x.length),
        DataType::Nvarchar(x) => x,
        DataType::Float(x) => x,
        DataType::TinyInt(x) => x,
        DataType::UnsignedTinyInt(x) => x,
        DataType::SmallInt(x) => x,
        DataType::UnsignedSmallInt(x) => x,
        DataType::Int(x) => x,
        DataType::Integer(x) => x,
        DataType::UnsignedInt(x) => x,
        DataType::UnsignedInteger(x) => x,
        DataType::BigInt(x) => x,
        DataType::UnsignedBigInt(x) => x,
        DataType::Decimal(x) => match x {
            ExactNumberInfo::None => None,
            ExactNumberInfo::Precision(x) => Some(x),
            ExactNumberInfo::PrecisionAndScale(x, _) => Some(x),
        },
        _ => None,
    }
}

/// Infer the column [AlgebraicType] from the [DataType] + `is_null` definition
//NOTE: We don't support `SERIAL` as recommended in https://wiki.postgresql.org/wiki/Don%27t_Do_This#Don.27t_use_serial
fn column_def_type(named: &String, is_null: bool, data_type: &DataType) -> Result<AlgebraicType, PlanError> {
    let ty = match data_type {
        DataType::Char(_) | DataType::Varchar(_) | DataType::Nvarchar(_) | DataType::Text | DataType::String => {
            AlgebraicType::String
        }
        DataType::Float(_) => AlgebraicType::F64,
        DataType::TinyInt(_) => AlgebraicType::I8,
        DataType::UnsignedTinyInt(_) => AlgebraicType::U8,
        DataType::SmallInt(_) => AlgebraicType::I16,
        DataType::UnsignedSmallInt(_) => AlgebraicType::U16,
        DataType::Int(_) => AlgebraicType::I32,
        DataType::Integer(_) => AlgebraicType::I32,
        DataType::UnsignedInt(_) => AlgebraicType::U32,
        DataType::UnsignedInteger(_) => AlgebraicType::U32,
        DataType::BigInt(_) => AlgebraicType::I64,
        DataType::UnsignedBigInt(_) => AlgebraicType::U64,
        DataType::Real => AlgebraicType::F32,
        DataType::Double => AlgebraicType::F64,
        DataType::Boolean => AlgebraicType::Bool,
        DataType::Array(Some(ty)) => AlgebraicType::array(column_def_type(named, false, ty)?),
        DataType::Enum(values) => AlgebraicType::simple_enum(values.iter().map(|x| x.as_str())),
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("Column {} of type {}", named, x),
            });
        }
    };

    Ok(if is_null { AlgebraicType::option(ty) } else { ty })
}

/// Extract the column attributes into [ColumnAttribute]
fn compile_column_option(col: &SqlColumnDef) -> Result<(bool, Constraints), PlanError> {
    let mut attr = Constraints::unset();
    let mut is_null = false;

    for x in &col.options {
        match &x.option {
            ColumnOption::Null => {
                is_null = true;
            }
            ColumnOption::NotNull => {
                is_null = false;
            }
            ColumnOption::Unique { is_primary } => {
                attr = attr.push(if *is_primary {
                    Constraints::primary_key()
                } else {
                    Constraints::unique()
                });
            }
            ColumnOption::Generated {
                generated_as,
                sequence_options: _,
                generation_expr,
            } => {
                unsupported!("IDENTITY options", generation_expr);

                match generated_as {
                    GeneratedAs::ByDefault => {
                        attr = attr.push(Constraints::identity());
                    }
                    x => {
                        return Err(PlanError::Unsupported {
                            feature: format!("IDENTITY option {x:?}"),
                        });
                    }
                }
            }
            ColumnOption::Comment(_) => {}
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Column option {x}"),
                });
            }
        }
    }
    Ok((is_null, attr))
}

/// Compiles the `CREATE TABLE ...` clause
fn compile_create_table(table: Table, cols: Vec<SqlColumnDef>) -> Result<SqlAst, PlanError> {
    let mut constraints = Vec::new();

    let mut columns = Vec::with_capacity(cols.len());
    for (col_pos, col) in cols.into_iter().enumerate() {
        if column_size(&col).is_some() {
            return Err(PlanError::Unsupported {
                feature: format!("Column with a defined size {}", col.name),
            });
        }

        let name = col.name.to_string();
        let (is_null, attr) = compile_column_option(&col)?;

        if attr.kind() != ConstraintKind::UNSET {
            constraints.push(ConstraintDef::for_column(
                &table.name,
                &name,
                attr,
                ColList::new(col_pos.into()),
            ));
        }

        let ty = column_def_type(&name, is_null, &col.data_type)?;
        columns.push(ColumnDef {
            col_name: name.into(),
            col_type: ty,
        });
    }

    let table = TableDef::new(table.name, columns).with_constraints(constraints);

    Ok(SqlAst::CreateTable { table })
}

/// Compiles the `DROP ...` clause
fn compile_drop(name: &ObjectName, kind: ObjectType) -> Result<SqlAst, PlanError> {
    let kind = match kind {
        ObjectType::Table => DbType::Table,
        ObjectType::Index => DbType::Index,
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("DROP {x}"),
            });
        }
    };

    let name = name.to_string();
    Ok(SqlAst::Drop { name, kind })
}

/// Compiles the equivalent of `SET key = value`
fn compile_set_config(name: ObjectName, value: Vec<SqlExpr>) -> Result<SqlAst, PlanError> {
    let name = name.to_string();

    let value = match value.as_slice() {
        [first] => first.clone(),
        _ => {
            return Err(PlanError::Unsupported {
                feature: format!("Invalid value for config: {name} => {value:?}."),
            });
        }
    };

    let value = match value {
        SqlExpr::Value(x) => match x {
            Value::Number(value, is_long) => infer_config(&name, &value, is_long)?,
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Unsupported value for config: {x}."),
                });
            }
        },
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("Unsupported expression for config: {x}"),
            });
        }
    };

    Ok(SqlAst::SetVar { name, value })
}

/// Compiles the equivalent of `SHOW key`
fn compile_read_config(name: Vec<Ident>) -> Result<SqlAst, PlanError> {
    let name = match name.as_slice() {
        [first] => first.to_string(),
        _ => {
            return Err(PlanError::Unsupported {
                feature: format!("Invalid name for config: {name:?}"),
            });
        }
    };
    Ok(SqlAst::ReadVar { name })
}

/// Compiles a `SQL` clause
fn compile_statement<T: TableSchemaView>(db: &DatabaseEngine, tx: &T, statement: Statement) -> Result<SqlAst, PlanError> {
    match statement {
        Statement::Query(query) => Ok(compile_query(db, tx, *query)?),
        Statement::Insert {
            or,
            into,
            table_name,
            columns,
            overwrite,
            source,
            partitioned,
            after_columns,
            table,
            on,
            returning,
        } => {
            unsupported!(
                "INSERT",
                or,
                overwrite,
                partitioned,
                after_columns,
                table,
                on,
                returning
            );
            if into {
                let values = match &*source.body {
                    SetExpr::Values(values) => values,
                    _ => {
                        return Err(PlanError::Unsupported {
                            feature: "Insert WITHOUT values".into(),
                        });
                    }
                };

                return compile_insert(db, tx, table_name, columns, values);
            };

            Err(PlanError::Unsupported {
                feature: "INSERT without INTO".into(),
            })
        }
        Statement::Update {
            table,
            assignments,
            from,
            selection,
            returning,
        } => {
            unsupported!("UPDATE", from, returning);

            let table_name = compile_table_factor(table.relation)?;
            compile_update(db, tx, table_name, assignments, selection)
        }
        Statement::Delete {
            tables,
            from,
            using,
            selection,
            returning,
        } => {
            unsupported!("DELETE", using, returning, tables);
            if from.len() != 1 {
                unsupported!("DELETE (multiple tables)", tables);
            }

            let table = from.first().unwrap().clone();
            let table_name = compile_table_factor(table.relation)?;
            compile_delete(db, tx, table_name, selection)
        }
        Statement::CreateTable {
            transient,
            columns,
            constraints,
            //not supported
            or_replace,
            temporary,
            external,
            global,
            if_not_exists,
            name,
            hive_distribution,
            hive_formats,
            table_properties,
            with_options,
            file_format,
            location,
            query,
            without_rowid,
            like,
            clone,
            engine,
            default_charset,
            collation,
            on_commit,
            on_cluster,
            order_by,
            comment,
            auto_increment_offset,
            strict,
        } => {
            if let Some(x) = &hive_formats {
                if x.row_format
                    .as_ref()
                    .and(x.location.as_ref())
                    .and(x.storage.as_ref())
                    .is_some()
                {
                    unsupported!("CREATE TABLE", hive_formats);
                }
            }
            unsupported!(
                "CREATE TABLE",
                transient,
                or_replace,
                temporary,
                external,
                global,
                if_not_exists,
                constraints,
                hive_distribution,
                table_properties,
                with_options,
                file_format,
                location,
                query,
                without_rowid,
                like,
                clone,
                engine,
                default_charset,
                collation,
                on_commit,
                on_cluster,
                order_by,
                comment,
                auto_increment_offset,
                strict,
            );
            let table = Table::new(name);
            compile_create_table(table, columns)
        }
        Statement::Drop {
            object_type,
            if_exists,
            names,
            cascade,
            restrict,
            purge,
            temporary,
        } => {
            unsupported!("DROP", if_exists, cascade, purge, restrict, temporary);

            if names.len() > 1 {
                return Err(PlanError::Unsupported {
                    feature: "DROP with more than 1 name".into(),
                });
            }
            let name = if let Some(name) = names.first() {
                name
            } else {
                return Err(PlanError::Unsupported {
                    feature: "DROP without names".into(),
                });
            };
            compile_drop(name, object_type)
        }
        Statement::SetVariable {
            local,
            hivevar,
            variable,
            value,
        } => {
            unsupported!("SET", local, hivevar);
            compile_set_config(variable, value)
        }
        Statement::ShowVariable { variable } => compile_read_config(variable),
        x => Err(PlanError::Unsupported {
            feature: format!("Syntax {x}"),
        }),
    }
}

/// Compiles a `sql` string into a `Vec<SqlAst>` using a SQL parser with [PostgreSqlDialect]
pub(crate) fn compile_to_ast<T: TableSchemaView>(
    db: &DatabaseEngine,
    tx: &T,
    sql_text: &str,
) -> Result<Vec<SqlAst>, DBError> {
    let dialect = PostgreSqlDialect {};
    let ast = Parser::parse_sql(&dialect, sql_text).map_err(|error| DBError::SqlParser {
        sql: sql_text.to_string(),
        error,
    })?;

    let mut results = Vec::new();
    for statement in ast {
        let plan_result = compile_statement(db, tx, statement);
        let query = match plan_result {
            Ok(plan) => plan,
            Err(error) => {
                return Err(DBError::Plan {
                    sql: sql_text.to_string(),
                    error,
                });
            }
        };
        results.push(query);
    }
    Ok(results)
}
