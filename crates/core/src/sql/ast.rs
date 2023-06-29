use spacetimedb_lib::table::{ColumnDef, ProductTypeMeta};
use spacetimedb_lib::ColumnIndexAttribute;
use spacetimedb_sats::auth::*;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductTypeElement};
use sqlparser::ast::{
    Assignment, BinaryOperator, ColumnDef as SqlColumnDef, ColumnOption, ColumnOptionDef, DataType, Expr as SqlExpr,
    HiveDistributionStyle, Ident, JoinConstraint, JoinOperator, ObjectName, ObjectType, Query, Select, SelectItem,
    SetExpr, Statement, TableFactor, TableWithJoins, Value, Values,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;

use crate::db::datastore::traits::{MutTxDatastore, TableId, TableSchema};
use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, PlanError};
use spacetimedb_sats::relation::{extract_table_field, FieldExpr, FieldName, RelationError};
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::expr::{ColumnOp, DbType, Expr};
use spacetimedb_vm::operator::{OpCmp, OpLogic, OpQuery};
use spacetimedb_vm::ops::parse::parse;

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

macro_rules! unsupported{
    ($name:literal,$a:expr)=>{
        let name = stringify!($name);
        let it = stringify!($a);
        if $a.unsupported() {
            return Err(PlanError::Unsupported {
                feature: format!("Unsupported {name} with `{it}` feature."),

            });
        }
    };
    ($name:literal,$a:expr,$b:expr)=>{
        {
            unsupported!($name,$a);
            unsupported!($name,$b);
        }
    };
    ($name:literal, $a:expr,$($b:tt)*)=>{
       {
           unsupported!($name, $a);
           unsupported!($name, $($b)*);
       }
    }
}

pub struct Table {
    pub(crate) name: String,
}

impl Table {
    pub fn new(name: ObjectName) -> Self {
        Self { name: name.to_string() }
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

#[derive(Debug, Clone)]
pub struct Selection {
    pub(crate) clauses: Vec<ColumnOp>,
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

impl Selection {
    pub fn new() -> Self {
        Self { clauses: Vec::new() }
    }

    pub fn with_cmp(self, op: OpQuery, lhs: ColumnOp, rhs: ColumnOp) -> Self {
        let mut x = self;
        let cmp = ColumnOp::cmp(op, lhs, rhs);
        x.clauses.push(cmp);
        x
    }
}

pub struct OnExpr {
    pub op: OpCmp,
    pub lhs: FieldName,
    pub rhs: FieldName,
}

pub enum Join {
    Inner { rhs: TableSchema, on: OnExpr },
}

#[derive(Clone)]
pub struct FromField {
    pub field: FieldName,
    pub column: ColumnDef,
}

pub struct From {
    pub root: TableSchema,
    pub join: Option<Vec<Join>>,
}

impl From {
    pub fn new(root: TableSchema) -> Self {
        Self { root, join: None }
    }

    pub fn with_inner_join(self, rhs: TableSchema, on: OnExpr) -> Self {
        let mut x = self;

        // Check if the field are inverted:
        // FROM t1 JOIN t2 ON t2.id = t1.id
        let on = if on.rhs.table() == x.root.table_name && x.root.get_column_by_field(&on.rhs).is_some() {
            OnExpr {
                op: on.op.reverse(),
                lhs: on.rhs,
                rhs: on.lhs,
            }
        } else {
            on
        };
        if let Some(joins) = &mut x.join {
            joins.push(Join::Inner { rhs, on })
        } else {
            x.join = Some(vec![Join::Inner { rhs, on }])
        }

        x
    }

    pub fn iter_tables(&self) -> impl Iterator<Item = &TableSchema> {
        [&self.root].into_iter().chain(self.join.iter().flat_map(|x| {
            x.iter().map(|t| match t {
                Join::Inner { rhs, .. } => rhs,
            })
        }))
    }

    pub fn table_names(&self) -> Vec<String> {
        self.iter_tables().map(|x| x.table_name.clone()).collect()
    }

    pub fn find_field(&self, f: &str) -> Result<Vec<FromField>, RelationError> {
        let field = extract_table_field(f)?;
        let fields = self.iter_tables().filter_map(|t| {
            let f = t.normalize_field(&field);
            t.get_column_by_field(&f).map(|column| FromField {
                field: f,
                column: column.into(),
            })
        });

        Ok(fields.collect())
    }

    pub fn resolve_field(&self, named: &str) -> Result<FromField, PlanError> {
        let fields = self.find_field(named)?;

        match fields.len() {
            0 => Err(PlanError::UnknownField {
                field: FieldName::named("?", named),
                tables: self.table_names(),
            }),
            1 => Ok(fields[0].clone()),
            _ => Err(PlanError::AmbiguousField {
                field: named.into(),
                found: fields.iter().map(|x| x.field.clone()).collect(),
            }),
        }
    }
}

pub enum SqlAst {
    Select {
        from: From,
        project: Vec<Column>,
        selection: Option<Selection>,
    },
    Insert {
        table: TableSchema,
        columns: Vec<FieldName>,
        values: Vec<Vec<FieldExpr>>,
    },
    Update {
        table: TableSchema,
        assignments: HashMap<FieldName, FieldExpr>,
        selection: Option<Selection>,
    },
    Delete {
        table: TableSchema,
        selection: Option<Selection>,
    },
    CreateTable {
        table: String,
        columns: ProductTypeMeta,
        table_type: StTableType,
        table_access: StAccess,
    },
    Drop {
        name: String,
        kind: DbType,
    },
}

fn extract_field(table: &From, of: &SqlExpr) -> Result<Option<ProductTypeElement>, PlanError> {
    match of {
        SqlExpr::Identifier(x) => {
            let f = table.resolve_field(&x.value)?;
            Ok(Some(f.column.column))
        }
        SqlExpr::CompoundIdentifier(ident) => {
            let col_name = compound_ident(ident);
            let f = table.resolve_field(&col_name)?;
            Ok(Some(f.column.column))
        }
        _ => Ok(None),
    }
}

fn infer_number(field: Option<&ProductTypeElement>, value: &str, is_long: bool) -> Result<AlgebraicValue, ErrorVm> {
    match field {
        None => {
            if value.contains('.') {
                if is_long {
                    parse(value, &AlgebraicType::F64)
                } else {
                    parse(value, &AlgebraicType::F32)
                }
            } else if is_long {
                parse(value, &AlgebraicType::I64)
            } else {
                parse(value, &AlgebraicType::I32)
            }
        }
        Some(f) => parse(value, &f.algebraic_type),
    }
}

fn compile_expr_value(table: &From, field: Option<&ProductTypeElement>, of: SqlExpr) -> Result<ColumnOp, PlanError> {
    Ok(ColumnOp::Field(match of {
        SqlExpr::Identifier(name) => FieldExpr::Name(table.resolve_field(&name.value)?.field),
        SqlExpr::CompoundIdentifier(ident) => {
            let col_name = compound_ident(&ident);
            table.resolve_field(&col_name)?.field.into()
        }
        SqlExpr::Value(x) => FieldExpr::Value(match x {
            Value::Number(value, is_long) => infer_number(field, &value, is_long)?,
            Value::SingleQuotedString(s) => AlgebraicValue::String(s),
            Value::DoubleQuotedString(s) => AlgebraicValue::String(s),
            Value::Boolean(x) => AlgebraicValue::Bool(x),
            Value::Null => AlgebraicValue::OptionNone(),
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Unsupported value: {x}."),
                })
            }
        }),
        SqlExpr::BinaryOp { left, op, right } => {
            let (op, lhs, rhs) = compile_bin_op(table, op, left, right)?;

            return Ok(ColumnOp::cmp(op, lhs, rhs));
        }
        SqlExpr::Nested(x) => {
            return compile_expr_value(table, field, *x);
        }
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("Unsupported expression: {x}"),
            })
        }
    }))
}

fn compile_expr_field(table: &From, field: Option<&ProductTypeElement>, of: SqlExpr) -> Result<FieldExpr, PlanError> {
    match compile_expr_value(table, field, of)? {
        ColumnOp::Field(field) => Ok(field),
        x => todo!("Complex expression {x} on insert..."),
    }
}

fn compile_table_factor(table: TableFactor) -> Result<Table, PlanError> {
    match table {
        TableFactor::Table {
            name,
            alias,
            args,
            with_hints,
        } => {
            unsupported!("TableFactor", alias, args, with_hints);

            Ok(Table::new(name))
        }
        x => Err(PlanError::Unsupported {
            feature: format!("TableFactor with syntax {x:?} not supported"),
        }),
    }
}

fn compile_bin_op(
    table: &From,
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
            })
        }
    };

    let field_lhs = extract_field(table, &lhs)?;
    let field_rhs = extract_field(table, &rhs)?;
    // This inversion is for inferring the type of the right side, like in `inventory.id = 1`,
    // so `1` get the type of `inventory.id`
    let lhs = compile_expr_value(table, field_rhs.as_ref(), *lhs)?;
    let rhs = compile_expr_value(table, field_lhs.as_ref(), *rhs)?;

    Ok((op, lhs, rhs))
}

fn _compile_where(table: &From, filter: SqlExpr, selection: Selection) -> Result<Option<Selection>, PlanError> {
    match filter {
        SqlExpr::BinaryOp { left, op, right } => {
            let (op, lhs, rhs) = compile_bin_op(table, op, left, right)?;

            Ok(Some(selection.with_cmp(op, lhs, rhs)))
        }
        SqlExpr::Nested(x) => _compile_where(table, *x, selection),
        x => Err(PlanError::Unsupported {
            feature: format!("Unsupported in WHERE: {x}."),
        }),
    }
}

fn compile_where(table: &From, filter: Option<SqlExpr>) -> Result<Option<Selection>, PlanError> {
    if let Some(filter) = filter {
        let selection = Selection::new();
        _compile_where(table, filter, selection)
    } else {
        Ok(None)
    }
}

fn find_table(db: &RelationalDB, t: Table) -> Result<TableSchema, PlanError> {
    //TODO: We should thread the `tx` from a upper layer instead...
    db.with_auto_commit(|tx| {
        let table_id = db
            .table_id_from_name(tx, &t.name)?
            .ok_or(PlanError::UnknownTable { table: t.name.clone() })?;
        if !db.inner.table_id_exists(tx, &TableId(table_id)) {
            return Err(PlanError::UnknownTable { table: t.name });
        }
        db.schema_for_table(tx, table_id)
            .map_err(|e| PlanError::DatabaseInternal(Box::new(e)))
    })
}

fn compile_from(db: &RelationalDB, from: &[TableWithJoins]) -> Result<From, PlanError> {
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
    let base = find_table(db, t)?;
    let mut base = From::new(base);

    for join in &root_table.joins {
        match &join.join_operator {
            JoinOperator::Inner(constraint) => {
                let t = compile_table_factor(join.relation.clone())?;
                let join = find_table(db, t)?;

                match constraint {
                    JoinConstraint::On(x) => {
                        let expr = compile_expr_value(&base, None, x.clone())?;
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
                                    _ => {
                                        todo!()
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

                // base.with_inner_join(
                //     rhs,
                //     OnExpr {
                //         op: OpCmp::Eq,
                //         lhs: (),
                //         rhs: (),
                //     },
                // )
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
                let value = compile_expr_value(from, None, expr)?;
                match value {
                    ColumnOp::Field(value) => match value {
                        FieldExpr::Name(_) => {
                            unreachable!("Should not be an identifier in Expr::Value")
                        }
                        FieldExpr::Value(x) => Ok(Column::UnnamedExpr(Expr::Value(x))),
                    },
                    x => {
                        unreachable!("Should not be an {} in Expr::Value", x)
                    }
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
        SelectItem::QualifiedWildcard(ident) => Ok(Column::QualifiedWildcard {
            table: ident.to_string(),
        }),
        SelectItem::Wildcard => Ok(Column::Wildcard),
    }
}

fn compile_select(db: &RelationalDB, select: Select) -> Result<SqlAst, PlanError> {
    let from = compile_from(db, &select.from)?;
    // SELECT ...
    let mut project = Vec::new();
    for select_item in select.projection {
        let col = compile_select_item(&from, select_item)?;
        project.push(col);
    }

    let selection = compile_where(&from, select.selection)?;

    Ok(SqlAst::Select {
        from,
        project,
        selection,
    })
}

fn compile_query(db: &RelationalDB, query: Query) -> Result<SqlAst, PlanError> {
    unsupported!(
        "SELECT",
        query.order_by,
        query.fetch,
        query.limit,
        query.offset,
        query.lock,
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

            compile_select(db, *select)
        }
        SetExpr::Query(_) => Err(PlanError::Unsupported {
            feature: "Query".into(),
        }),
        SetExpr::SetOperation {
            op: _,
            all: _,
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
    }
}

fn compile_insert(
    db: &RelationalDB,
    table_name: ObjectName,
    columns: Vec<Ident>,
    data: Values,
) -> Result<SqlAst, PlanError> {
    let table = find_table(db, Table::new(table_name))?;

    let columns = columns
        .into_iter()
        .map(|x| FieldName::named(&table.table_name, &x.to_string()))
        .collect();

    let table = From::new(table);

    let mut values = Vec::with_capacity(data.0.len());

    for x in data.0 {
        let mut row = Vec::with_capacity(x.len());
        for (pos, v) in x.into_iter().enumerate() {
            let field = table.root.get_column(pos).map(ProductTypeElement::from);
            row.push(compile_expr_field(&table, field.as_ref(), v)?);
        }

        values.push(row);
    }
    Ok(SqlAst::Insert {
        table: table.root,
        columns,
        values,
    })
}

fn compile_update(
    db: &RelationalDB,
    table: Table,
    assignments: Vec<Assignment>,
    selection: Option<SqlExpr>,
) -> Result<SqlAst, PlanError> {
    let table = From::new(find_table(db, table)?);
    let selection = compile_where(&table, selection)?;

    let mut x = HashMap::with_capacity(assignments.len());

    for col in assignments {
        let name: String = col.id.iter().map(|x| x.to_string()).collect();

        let field = table.root.get_column_by_name(&name).map(ProductTypeElement::from);
        let value = compile_expr_field(&table, field.as_ref(), col.value)?;
        x.insert(FieldName::named(&table.root.table_name, &name), value);
    }

    Ok(SqlAst::Update {
        table: table.root,
        assignments: x,
        selection,
    })
}

fn compile_delete(db: &RelationalDB, table: Table, selection: Option<SqlExpr>) -> Result<SqlAst, PlanError> {
    let table = From::new(find_table(db, table)?);
    let selection = compile_where(&table, selection)?;

    Ok(SqlAst::Delete {
        table: table.root,
        selection,
    })
}

fn column_size(column: &SqlColumnDef) -> Option<u64> {
    match column.data_type {
        DataType::Char(x) => x,
        DataType::Varchar(x) => x,
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
        DataType::Decimal(x, _) => x,
        _ => None,
    }
}

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
        DataType::Array(ty) => AlgebraicType::make_array_type(column_def_type(named, false, ty)?),
        DataType::Enum(values) => AlgebraicType::make_simple_enum(values.iter().map(|x| x.as_str())),
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("Column {} of type {}", named, x),
            })
        }
    };

    Ok(if is_null {
        AlgebraicType::make_option_type(ty)
    } else {
        ty
    })
}

fn compile_column_option(options: &[ColumnOptionDef]) -> Result<(bool, ColumnIndexAttribute), PlanError> {
    let mut attr = ColumnIndexAttribute::UnSet;
    let mut is_null = false;

    for x in options {
        match &x.option {
            ColumnOption::Null => {
                is_null = true;
            }
            ColumnOption::NotNull => {
                is_null = false;
            }
            ColumnOption::Unique { is_primary } => {
                if *is_primary {
                    attr = ColumnIndexAttribute::Identity
                } else {
                    attr = ColumnIndexAttribute::Unique
                }
            }
            ColumnOption::Comment(_) => {}
            x => {
                return Err(PlanError::Unsupported {
                    feature: format!("Column option {x}"),
                })
            }
        }
    }
    Ok((is_null, attr))
}

fn compile_create_table(table: Table, cols: Vec<SqlColumnDef>) -> Result<SqlAst, PlanError> {
    let table = table.name;
    let mut columns = ProductTypeMeta::with_capacity(cols.len());

    for col in cols {
        if column_size(&col).is_some() {
            return Err(PlanError::Unsupported {
                feature: format!("Column with a defined size {}", col.name),
            });
        }

        let name = col.name.to_string();
        let (is_null, attr) = compile_column_option(&col.options)?;
        let ty = column_def_type(&name, is_null, &col.data_type)?;

        columns.push(&name, ty, attr);
    }

    Ok(SqlAst::CreateTable {
        table_access: StAccess::for_name(&table),
        table,
        columns,
        table_type: StTableType::User,
    })
}

fn compile_drop(name: &ObjectName, kind: ObjectType) -> Result<SqlAst, PlanError> {
    let kind = match kind {
        ObjectType::Table => DbType::Table,
        ObjectType::Index => DbType::Index,
        x => {
            return Err(PlanError::Unsupported {
                feature: format!("DROP {x}"),
            })
        }
    };

    Ok(SqlAst::Drop {
        name: name.to_string(),
        kind,
    })
}

fn compile_statement(db: &RelationalDB, statement: Statement) -> Result<SqlAst, PlanError> {
    match statement {
        Statement::Query(query) => Ok(compile_query(db, *query)?),
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
        } => {
            unsupported!("INSERT", or, overwrite, partitioned, after_columns, table, on);
            if into {
                let values = match *source.body {
                    SetExpr::Values(values) => values,
                    _ => {
                        return Err(PlanError::Unsupported {
                            feature: "Insert WITHOUT values".into(),
                        })
                    }
                };

                return compile_insert(db, table_name, columns, values);
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
        } => {
            unsupported!("UPDATE", from);

            let table_name = compile_table_factor(table.relation)?;
            compile_update(db, table_name, assignments, selection)
        }
        Statement::Delete {
            table_name,
            using,
            selection,
        } => {
            unsupported!("DELETE", using);

            let table_name = compile_table_factor(table_name)?;
            compile_delete(db, table_name, selection)
        }
        Statement::CreateTable {
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
                on_cluster
            );
            let table = Table::new(name);
            compile_create_table(table, columns)
        }
        Statement::Drop {
            object_type,
            if_exists,
            names,
            cascade,
            purge,
        } => {
            unsupported!("DROP", if_exists, cascade, purge);

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
        Statement::Analyze {
            table_name: _,
            partitions: _,
            for_columns: _,
            columns: _,
            cache_metadata: _,
            noscan: _,
            compute_statistics: _,
        } => Err(PlanError::Unsupported {
            feature: "Analyze".into(),
        }),
        Statement::Truncate {
            table_name: _,
            partitions: _,
        } => Err(PlanError::Unsupported {
            feature: "Truncate".into(),
        }),
        Statement::Msck {
            table_name: _,
            repair: _,
            partition_action: _,
        } => Err(PlanError::Unsupported { feature: "Msck".into() }),

        Statement::Directory {
            overwrite: _,
            local: _,
            path: _,
            file_format: _,
            source: _,
        } => Err(PlanError::Unsupported {
            feature: "Directory".into(),
        }),
        Statement::Copy {
            table_name: _,
            columns: _,
            to: _,
            target: _,
            options: _,
            legacy_options: _,
            values: _,
        } => Err(PlanError::Unsupported { feature: "Copy".into() }),
        Statement::Close { cursor: _ } => Err(PlanError::Unsupported {
            feature: "Close".into(),
        }),
        Statement::CreateView {
            or_replace: _,
            materialized: _,
            name: _,
            columns: _,
            query: _,
            with_options: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateView".into(),
        }),

        Statement::CreateVirtualTable {
            name: _,
            if_not_exists: _,
            module_name: _,
            module_args: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateVirtualTable".into(),
        }),
        Statement::CreateIndex {
            name: _,
            table_name: _,
            columns: _,
            unique: _,
            if_not_exists: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateIndex".into(),
        }),
        Statement::AlterTable { name: _, operation: _ } => Err(PlanError::Unsupported {
            feature: "AlterTable".into(),
        }),
        Statement::Declare {
            name: _,
            binary: _,
            sensitive: _,
            scroll: _,
            hold: _,
            query: _,
        } => Err(PlanError::Unsupported {
            feature: "Declare".into(),
        }),
        Statement::Fetch {
            name: _,
            direction: _,
            into: _,
        } => Err(PlanError::Unsupported {
            feature: "Fetch".into(),
        }),
        Statement::Discard { object_type: _ } => Err(PlanError::Unsupported {
            feature: "Discard".into(),
        }),
        Statement::SetRole {
            local: _,
            session: _,
            role_name: _,
        } => Err(PlanError::Unsupported {
            feature: "SetRole".into(),
        }),
        Statement::SetVariable {
            local: _,
            hivevar: _,
            variable: _,
            value: _,
        } => Err(PlanError::Unsupported {
            feature: "SetVariable".into(),
        }),
        Statement::SetNames {
            charset_name: _,
            collation_name: _,
        } => Err(PlanError::Unsupported {
            feature: "SetNames".into(),
        }),
        Statement::SetNamesDefault {} => Err(PlanError::Unsupported {
            feature: "SetNamesDefault".into(),
        }),
        Statement::ShowVariable { variable: _ } => Err(PlanError::Unsupported {
            feature: "ShowVariable".into(),
        }),
        Statement::ShowVariables { filter: _ } => Err(PlanError::Unsupported {
            feature: "ShowVariables".into(),
        }),
        Statement::ShowCreate {
            obj_type: _,
            obj_name: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowCreate".into(),
        }),
        Statement::ShowColumns {
            extended: _,
            full: _,
            table_name: _,
            filter: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowColumns".into(),
        }),
        Statement::ShowTables {
            extended: _,
            full: _,
            db_name: _,
            filter: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowTables".into(),
        }),
        Statement::ShowCollation { filter: _ } => Err(PlanError::Unsupported {
            feature: "ShowCollation".into(),
        }),
        Statement::Use { db_name: _ } => Err(PlanError::Unsupported { feature: "Use".into() }),
        Statement::StartTransaction { modes: _ } => Err(PlanError::Unsupported {
            feature: "StartTransaction".into(),
        }),
        Statement::SetTransaction {
            modes: _,
            snapshot: _,
            session: _,
        } => Err(PlanError::Unsupported {
            feature: "SetTransaction".into(),
        }),
        Statement::Comment {
            object_type: _,
            object_name: _,
            comment: _,
        } => Err(PlanError::Unsupported {
            feature: "Comment".into(),
        }),
        Statement::Commit { chain: _ } => Err(PlanError::Unsupported {
            feature: "Commit".into(),
        }),
        Statement::Rollback { chain: _ } => Err(PlanError::Unsupported {
            feature: "Rollback".into(),
        }),
        Statement::CreateSchema {
            schema_name: _,
            if_not_exists: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateSchema".into(),
        }),
        Statement::CreateDatabase {
            db_name: _,
            if_not_exists: _,
            location: _,
            managed_location: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateDatabase".into(),
        }),
        Statement::CreateFunction {
            temporary: _,
            name: _,
            class_name: _,
            using: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateFunction".into(),
        }),
        Statement::Assert {
            condition: _,
            message: _,
        } => Err(PlanError::Unsupported {
            feature: "Assert".into(),
        }),
        Statement::Grant {
            privileges: _,
            objects: _,
            grantees: _,
            with_grant_option: _,
            granted_by: _,
        } => Err(PlanError::Unsupported {
            feature: "Grant".into(),
        }),
        Statement::Revoke {
            privileges: _,
            objects: _,
            grantees: _,
            granted_by: _,
            cascade: _,
        } => Err(PlanError::Unsupported {
            feature: "Revoke".into(),
        }),
        Statement::Deallocate { name: _, prepare: _ } => Err(PlanError::Unsupported {
            feature: "Deallocate".into(),
        }),
        Statement::Execute { name: _, parameters: _ } => Err(PlanError::Unsupported {
            feature: "Execute".into(),
        }),
        Statement::Prepare {
            name: _,
            data_types: _,
            statement: _,
        } => Err(PlanError::Unsupported {
            feature: "Prepare".into(),
        }),
        Statement::Kill { modifier: _, id: _ } => Err(PlanError::Unsupported { feature: "Kill".into() }),
        Statement::ExplainTable {
            describe_alias: _,
            table_name: _,
        } => Err(PlanError::Unsupported {
            feature: "ExplainTable".into(),
        }),
        Statement::Explain {
            describe_alias: _,
            analyze: _,
            verbose: _,
            statement: _,
        } => Err(PlanError::Unsupported {
            feature: "Explain".into(),
        }),
        Statement::Savepoint { name: _ } => Err(PlanError::Unsupported {
            feature: "Savepoint".into(),
        }),
        Statement::Merge {
            into: _,
            table: _,
            source: _,
            on: _,
            clauses: _,
        } => Err(PlanError::Unsupported {
            feature: "Merge".into(),
        }),
    }
}

pub(crate) fn compile_to_ast(db: &RelationalDB, sql_text: &str) -> Result<Vec<SqlAst>, DBError> {
    let dialect = PostgreSqlDialect {};
    let ast = Parser::parse_sql(&dialect, sql_text).map_err(|error| DBError::SqlParser {
        sql: sql_text.to_string(),
        error,
    })?;

    let mut results = Vec::new();
    for statement in ast {
        let plan_result = compile_statement(db, statement);
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
