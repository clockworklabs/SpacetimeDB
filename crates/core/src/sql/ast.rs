use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::datastore::system_tables::{StRowLevelSecurityFields, ST_ROW_LEVEL_SECURITY_ID};
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::{DBError, PlanError};
use anyhow::Context;
use spacetimedb_data_structures::map::{HashCollectionExt as _, IntMap};
use spacetimedb_expr::check::SchemaView;
use spacetimedb_expr::statement::compile_sql_stmt;
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::db::error::RelationError;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{ColExpr, FieldName};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::expr::{Expr, FieldExpr, FieldOp};
use spacetimedb_vm::operator::{OpCmp, OpLogic, OpQuery};
use spacetimedb_vm::ops::parse::{parse, parse_simple_enum};
use sqlparser::ast::{
    Assignment, BinaryOperator, Expr as SqlExpr, HiveDistributionStyle, Ident, JoinConstraint, JoinOperator,
    ObjectName, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::ops::Deref;
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
    pub(crate) clause: FieldOp,
}

impl Selection {
    pub fn with_cmp(op: OpQuery, lhs: FieldOp, rhs: FieldOp) -> Self {
        let cmp = FieldOp::new(op, lhs, rhs);
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

    /// Returns the name of the table,
    /// together with the column definition at position `field.col`,
    /// for table `field.table_id`.
    pub(super) fn find_field_name(&self, field: FieldName) -> Option<(&str, &ColumnSchema)> {
        self.iter_tables().find_map(|t| {
            if t.table_id == field.table() {
                t.get_column_by_field(field).map(|c| (&*t.table_name, c))
            } else {
                None
            }
        })
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
        project: Box<[Column]>,
        selection: Option<Selection>,
    },
    Insert {
        table: Arc<TableSchema>,
        columns: Box<[ColId]>,
        values: Box<[Box<[ColExpr]>]>,
    },
    Update {
        table: Arc<TableSchema>,
        assignments: IntMap<ColId, ColExpr>,
        selection: Option<Selection>,
    },
    Delete {
        table: Arc<TableSchema>,
        selection: Option<Selection>,
    },
    SetVar {
        name: String,
        literal: String,
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

/// Compiles a [SqlExpr] expression into a [ColumnOp]
fn compile_expr_value<'a>(
    tables: impl Clone + Iterator<Item = &'a TableSchema>,
    field: Option<&'a AlgebraicType>,
    of: SqlExpr,
) -> Result<FieldOp, PlanError> {
    Ok(FieldOp::Field(match of {
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

            return Ok(FieldOp::new(op, lhs, rhs));
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
        FieldOp::Field(field) => Ok(field),
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
) -> Result<(OpQuery, FieldOp, FieldOp), PlanError> {
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

pub struct SchemaViewer<'a, T> {
    tx: &'a T,
    auth: &'a AuthCtx,
}

impl<T> Deref for SchemaViewer<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.tx
    }
}

impl<T: StateView> SchemaView for SchemaViewer<'_, T> {
    fn table_id(&self, name: &str) -> Option<TableId> {
        let AuthCtx { owner, caller } = self.auth;
        // Get the schema from the in-memory state instead of fetching from the database for speed
        self.tx
            .table_id_from_name(name)
            .ok()
            .flatten()
            .and_then(|table_id| self.schema_for_table(table_id))
            .filter(|schema| schema.table_access == StAccess::Public || caller == owner)
            .map(|schema| schema.table_id)
    }

    fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>> {
        let AuthCtx { owner, caller } = self.auth;
        self.tx
            .get_schema(table_id)
            .filter(|schema| schema.table_access == StAccess::Public || caller == owner)
            .cloned()
    }

    fn rls_rules_for_table(&self, table_id: TableId) -> anyhow::Result<Vec<Box<str>>> {
        self.tx
            .iter_by_col_eq(
                ST_ROW_LEVEL_SECURITY_ID,
                StRowLevelSecurityFields::TableId,
                &AlgebraicValue::from(table_id),
            )?
            .map(|row| {
                row.read_col::<AlgebraicValue>(StRowLevelSecurityFields::Sql)
                    .with_context(|| {
                        format!(
                            "Failed to read value from the `{}` column of `{}` for table_id `{}`",
                            "sql", "st_row_level_security", table_id
                        )
                    })
                    .and_then(|sql| {
                        sql.into_string().map_err(|_| {
                            anyhow::anyhow!(format!(
                                "Failed to read value from the `{}` column of `{}` for table_id `{}`",
                                "sql", "st_row_level_security", table_id
                            ))
                        })
                    })
            })
            .collect::<anyhow::Result<_>>()
    }
}

impl<'a, T> SchemaViewer<'a, T> {
    pub fn new(tx: &'a T, auth: &'a AuthCtx) -> Self {
        Self { tx, auth }
    }
}

pub trait TableSchemaView {
    fn find_table(&self, db: &RelationalDB, t: Table) -> Result<Arc<TableSchema>, PlanError>;
}

impl TableSchemaView for Tx {
    fn find_table(&self, db: &RelationalDB, t: Table) -> Result<Arc<TableSchema>, PlanError> {
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
    fn find_table(&self, db: &RelationalDB, t: Table) -> Result<Arc<TableSchema>, PlanError> {
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
fn compile_from<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    tx: &T,
    from: &[TableWithJoins],
) -> Result<From, PlanError> {
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
                            FieldOp::Field(_) => {}
                            FieldOp::Cmp { op, lhs, rhs } => {
                                let op = match op {
                                    OpQuery::Cmp(op) => op,
                                    OpQuery::Logic(op) => {
                                        return Err(PlanError::Unsupported {
                                            feature: format!("Can't use operator {op} on JOIN clause"),
                                        });
                                    }
                                };
                                let (lhs, rhs) = match (*lhs, *rhs) {
                                    (FieldOp::Field(FieldExpr::Name(lhs)), FieldOp::Field(FieldExpr::Name(rhs))) => {
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
                    FieldOp::Field(value) => match value {
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
fn compile_select<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    tx: &T,
    select: Select,
) -> Result<SqlAst, PlanError> {
    let from = compile_from(db, tx, &select.from)?;

    // SELECT ...
    let mut project = Vec::with_capacity(select.projection.len());
    for select_item in select.projection {
        project.push(compile_select_item(&from, select_item)?);
    }
    let project = project.into();

    let selection = compile_where(&from, select.selection)?;

    Ok(SqlAst::Select {
        from,
        project,
        selection,
    })
}

/// Compiles any `query` clause (currently only `SELECT...`)
fn compile_query<T: TableSchemaView + StateView>(db: &RelationalDB, tx: &T, query: Query) -> Result<SqlAst, PlanError> {
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
fn compile_insert<T: TableSchemaView + StateView>(
    db: &RelationalDB,
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
                .map(|(f, _)| f.col)
        })
        .collect::<Result<Box<[_]>, _>>()?;

    let mut values = Vec::with_capacity(data.rows.len());
    for x in &data.rows {
        let mut row = Vec::with_capacity(x.len());
        for (pos, v) in x.iter().enumerate() {
            let field_ty = table.root.get_column(pos).map(|col| &col.col_type);
            row.push(compile_expr_field(&table, field_ty, v.clone())?.strip_table());
        }
        values.push(row.into());
    }
    let values = values.into();

    Ok(SqlAst::Insert {
        table: table.root,
        columns,
        values,
    })
}

/// Compiles the `UPDATE ...` clause
fn compile_update<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    tx: &T,
    table: Table,
    assignments: Vec<Assignment>,
    selection: Option<SqlExpr>,
) -> Result<SqlAst, PlanError> {
    let table = From::new(tx.find_table(db, table)?);
    let selection = compile_where(&table, selection)?;

    let mut assigns = IntMap::with_capacity(assignments.len());
    for col in assignments {
        let name: String = col.id.iter().map(|x| x.to_string()).collect();
        let (field_name, field_ty) = table.find_field(&name)?;
        let col_id = field_name.col;

        let value = compile_expr_field(&table, Some(field_ty), col.value)?.strip_table();
        assigns.insert(col_id, value);
    }

    Ok(SqlAst::Update {
        table: table.root,
        assignments: assigns,
        selection,
    })
}

/// Compiles the `DELETE ...` clause
fn compile_delete<T: TableSchemaView + StateView>(
    db: &RelationalDB,
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

// Compiles the equivalent of `SET key = value`
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

    let literal = match value {
        SqlExpr::Value(x) => match x {
            Value::Number(value, _) => value,
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

    Ok(SqlAst::SetVar { name, literal })
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
fn compile_statement<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    tx: &T,
    statement: Statement,
) -> Result<SqlAst, PlanError> {
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
pub(crate) fn compile_to_ast<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    auth: &AuthCtx,
    tx: &T,
    sql_text: &str,
) -> Result<Vec<SqlAst>, DBError> {
    // NOTE: The following ensures compliance with the 1.0 sql api.
    // Come 1.0, it will have replaced the current compilation stack.
    compile_sql_stmt(sql_text, &SchemaViewer::new(tx, auth), auth, false)?;

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
