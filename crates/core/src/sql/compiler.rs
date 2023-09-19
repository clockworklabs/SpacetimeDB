use std::collections::HashMap;

use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::{IndexSchema, TableSchema};
use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, PlanError};
use crate::sql::ast::{compile_to_ast, Column, From, Join, Selection, SqlAst};
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_lib::relation::{self, DbTable, FieldExpr, FieldName, Header};
use spacetimedb_lib::table::ProductTypeMeta;
use spacetimedb_sats::{AlgebraicValue, ProductType};
use spacetimedb_vm::dsl::{db_table, db_table_raw, query};
use spacetimedb_vm::expr::{ColumnOp, CrudExpr, DbType, Expr, QueryExpr, SourceExpr};
use spacetimedb_vm::operator::OpCmp;

/// Compile the `SQL` expression into a `ast`
#[tracing::instrument(skip(db, tx))]
pub fn compile_sql(db: &RelationalDB, tx: &MutTxId, sql_text: &str) -> Result<Vec<CrudExpr>, DBError> {
    let ast = compile_to_ast(db, tx, sql_text)?;

    let mut results = Vec::with_capacity(ast.len());

    for sql in ast {
        results.push(compile_statement(sql).map_err(|error| DBError::Plan {
            sql: sql_text.to_string(),
            error,
        })?);
    }

    Ok(results)
}

fn expr_for_projection(table: &From, of: Expr) -> Result<FieldExpr, PlanError> {
    match of {
        Expr::Ident(x) => {
            let f = table.resolve_field(&x)?;

            Ok(FieldExpr::Name(f.field))
        }
        Expr::Value(x) => Ok(FieldExpr::Value(x)),
        x => unreachable!("Wrong expression in SQL query {:?}", x),
    }
}

fn check_field(table: &From, field: &FieldExpr) -> Result<(), PlanError> {
    if let FieldExpr::Name(field) = field {
        table.resolve_field(&field.to_string())?;
    }
    Ok(())
}

fn check_field_column(table: &From, field: &ColumnOp) -> Result<(), PlanError> {
    if let ColumnOp::Field(field) = field {
        check_field(table, field)?;
    }
    Ok(())
}

/// Verify the `fields` inside the `expr` are valid
fn check_cmp_expr(table: &From, expr: &ColumnOp) -> Result<(), PlanError> {
    match expr {
        ColumnOp::Field(field) => check_field(table, field)?,
        ColumnOp::Cmp { op: _, lhs, rhs } => {
            check_field_column(table, lhs)?;
            check_field_column(table, rhs)?;
        }
    }

    Ok(())
}

/// Compiles a `WHERE ...` clause
fn compile_where(mut q: QueryExpr, table: &From, filter: Selection) -> Result<QueryExpr, PlanError> {
    check_cmp_expr(table, &filter.clause)?;
    let schema = &table.root;
    for op in filter.clause.to_vec() {
        match is_sargable(table, &op) {
            Some(IndexArgument::Eq { col_id, value }) => {
                q = q.with_index_eq(schema.into(), col_id, value);
            }
            Some(IndexArgument::LowerBound {
                col_id,
                value,
                inclusive,
            }) => {
                q = q.with_index_lower_bound(schema.into(), col_id, value, inclusive);
            }
            Some(IndexArgument::UpperBound {
                col_id,
                value,
                inclusive,
            }) => {
                q = q.with_index_upper_bound(schema.into(), col_id, value, inclusive);
            }
            None => {
                q = q.with_select(op);
            }
        }
    }
    Ok(q)
}

// IndexArgument represents an equality or range predicate that can be answered
// using an index.
pub enum IndexArgument {
    Eq {
        col_id: u32,
        value: AlgebraicValue,
    },
    LowerBound {
        col_id: u32,
        value: AlgebraicValue,
        inclusive: bool,
    },
    UpperBound {
        col_id: u32,
        value: AlgebraicValue,
        inclusive: bool,
    },
}

// Sargable stands for Search ARGument ABLE.
// A sargable predicate is one that can be answered using an index.
fn is_sargable(table: &From, op: &ColumnOp) -> Option<IndexArgument> {
    if let ColumnOp::Cmp {
        op: OpQuery::Cmp(op),
        lhs,
        rhs,
    } = op
    {
        // lhs must be a field
        let ColumnOp::Field(FieldExpr::Name(ref name)) = **lhs else {
            return None;
        };
        // rhs must be a value
        let ColumnOp::Field(FieldExpr::Value(ref value)) = **rhs else {
            return None;
        };
        // lhs field must exist
        let column = table.root.get_column_by_field(name)?;
        // lhs field must have an index
        let index =
            table.root.indexes.iter().find(|IndexSchema { table_id, col_id, .. }| {
                *table_id == column.table_id && *col_id == column.col_id
            })?;
        match op {
            OpCmp::Eq => Some(IndexArgument::Eq {
                col_id: index.col_id,
                value: value.clone(),
            }),
            // a < 5 => exclusive upper bound
            OpCmp::Lt => Some(IndexArgument::UpperBound {
                col_id: index.col_id,
                value: value.clone(),
                inclusive: false,
            }),
            // a > 5 => exclusive lower bound
            OpCmp::Gt => Some(IndexArgument::LowerBound {
                col_id: index.col_id,
                value: value.clone(),
                inclusive: false,
            }),
            // a <= 5 => inclusive upper bound
            OpCmp::LtEq => Some(IndexArgument::UpperBound {
                col_id: index.col_id,
                value: value.clone(),
                inclusive: true,
            }),
            // a >= 5 => inclusive lower bound
            OpCmp::GtEq => Some(IndexArgument::LowerBound {
                col_id: index.col_id,
                value: value.clone(),
                inclusive: true,
            }),
            OpCmp::NotEq => None,
        }
    } else {
        None
    }
}

/// Compiles a `SELECT ...` clause
fn compile_select(table: From, project: Vec<Column>, selection: Option<Selection>) -> Result<QueryExpr, PlanError> {
    let mut not_found = Vec::with_capacity(project.len());
    let mut col_ids = Vec::new();
    //Match columns to their tables...
    for select_item in project {
        match select_item {
            Column::UnnamedExpr(x) => match expr_for_projection(&table, x) {
                Ok(field) => col_ids.push(field),
                Err(PlanError::UnknownField { field, tables: _ }) => not_found.push(field),
                Err(err) => return Err(err),
            },
            Column::QualifiedWildcard { table: name } => {
                if let Some(t) = table.iter_tables().find(|x| x.table_name == name) {
                    for c in t.columns.iter() {
                        col_ids.push(FieldName::named(&t.table_name, &c.col_name).into());
                    }
                } else {
                    return Err(PlanError::TableNotFoundQualified { expect: name });
                }
            }
            Column::Wildcard => {}
        }
    }

    if !not_found.is_empty() {
        return Err(PlanError::UnknownFields {
            fields: not_found,
            tables: table.table_names(),
        });
    }

    let mut q = query(db_table_raw(
        ProductType::from(&table.root),
        &table.root.table_name,
        table.root.table_id,
        table.root.table_type,
        table.root.table_access,
    ));

    if let Some(ref joins) = table.join {
        for join in joins {
            match join {
                Join::Inner { rhs, on } => {
                    let t = db_table(rhs.into(), &rhs.table_name, rhs.table_id);
                    match on.op {
                        OpCmp::Eq => {}
                        x => unreachable!("Unsupported operator `{x}` for joins"),
                    }
                    q = q.with_join_inner(t, on.lhs.clone(), on.rhs.clone());
                }
            }
        }
    };

    if let Some(filter) = selection {
        q = compile_where(q, &table, filter)?;
    }
    //Is important to project at the end, so joins, filters see fields that are not projected
    q = q.with_project(&col_ids);

    Ok(q)
}

/// Builds the schema description [DbTable] from the [TableSchema] and their list of columns
fn compile_columns(table: &TableSchema, columns: Vec<FieldName>) -> DbTable {
    let mut new = Vec::with_capacity(columns.len());

    for col in columns.into_iter() {
        if let Some(x) = table.get_column_by_field(&col) {
            let field = FieldName::named(&table.table_name, &x.col_name);
            new.push(relation::Column::new(field, x.col_type.clone()));
        }
    }

    DbTable::new(
        &Header::new(&table.table_name, &new),
        table.table_id,
        table.table_type,
        table.table_access,
    )
}

/// Compiles a `INSERT ...` clause
fn compile_insert(
    table: TableSchema,
    columns: Vec<FieldName>,
    values: Vec<Vec<FieldExpr>>,
) -> Result<CrudExpr, PlanError> {
    let db_table = compile_columns(&table, columns);

    Ok(CrudExpr::Insert {
        source: SourceExpr::DbTable(db_table),
        rows: values,
    })
}

/// Compiles a `DELETE ...` clause
fn compile_delete(table: TableSchema, selection: Option<Selection>) -> Result<CrudExpr, PlanError> {
    let query = if let Some(filter) = selection {
        let query = QueryExpr::new(&table);
        compile_where(query, &From::new(table), filter)?
    } else {
        QueryExpr::new(&table)
    };
    Ok(CrudExpr::Delete { query })
}

/// Compiles a `UPDATE ...` clause
fn compile_update(
    table: TableSchema,
    assignments: HashMap<FieldName, FieldExpr>,
    selection: Option<Selection>,
) -> Result<CrudExpr, PlanError> {
    let table = From::new(table);
    let delete = if let Some(filter) = selection.clone() {
        let query = QueryExpr::new(&table.root);
        compile_where(query, &table, filter)?
    } else {
        QueryExpr::new(&table.root)
    };
    let mut cols = Vec::with_capacity(table.root.columns.len());

    for field in table.root.columns.iter() {
        let field = FieldName::named(&table.root.table_name, &field.col_name);
        if let Some(f) = assignments.get(&field) {
            cols.push(f.clone());
        } else {
            cols.push(FieldExpr::Name(field));
        }
    }

    let insert = QueryExpr::new(&table.root).with_project(&cols);
    let insert = if let Some(filter) = selection {
        compile_where(insert, &table, filter)?
    } else {
        insert
    };
    Ok(CrudExpr::Update { insert, delete })
}

/// Compiles a `CREATE TABLE ...` clause
fn compile_create_table(
    name: String,
    columns: ProductTypeMeta,
    table_type: StTableType,
    table_access: StAccess,
) -> Result<CrudExpr, PlanError> {
    Ok(CrudExpr::CreateTable {
        name,
        columns,
        table_type,
        table_access,
    })
}

/// Compiles a `DROP ...` clause
fn compile_drop(name: String, kind: DbType, table_access: StAccess) -> Result<CrudExpr, PlanError> {
    Ok(CrudExpr::Drop {
        name,
        kind,
        table_access,
    })
}

/// Compiles a `SQL` clause
fn compile_statement(statement: SqlAst) -> Result<CrudExpr, PlanError> {
    let q = match statement {
        SqlAst::Select {
            from,
            project,
            selection,
        } => CrudExpr::Query(compile_select(from, project, selection)?),
        SqlAst::Insert { table, columns, values } => compile_insert(table, columns, values)?,
        SqlAst::Update {
            table,
            assignments,
            selection,
        } => compile_update(table, assignments, selection)?,
        SqlAst::Delete { table, selection } => compile_delete(table, selection)?,
        SqlAst::CreateTable {
            table,
            columns,
            table_type,
            table_access: schema,
        } => compile_create_table(table, columns, table_type, schema)?,
        SqlAst::Drop {
            name,
            kind,
            table_access,
        } => compile_drop(name, kind, table_access)?,
    };

    Ok(q)
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use super::*;
    use itertools::Itertools;
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        error::ResultTest,
    };
    use spacetimedb_sats::AlgebraicType;
    use spacetimedb_vm::expr::{IndexScan, Query};

    use crate::db::{
        datastore::traits::{ColumnDef, IndexDef, TableDef},
        relational_db::tests_utils::make_test_db,
    };

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTxId,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(u32, &str)],
    ) -> ResultTest<()> {
        let table_name = name.to_string();
        let table_type = StTableType::User;
        let table_access = StAccess::Public;

        let columns = schema
            .iter()
            .map(|(col_name, col_type)| ColumnDef {
                col_name: col_name.to_string(),
                col_type: col_type.clone(),
                is_autoinc: false,
            })
            .collect_vec();

        let indexes = indexes
            .iter()
            .map(|(col_id, index_name)| IndexDef {
                table_id: 0,
                col_id: *col_id,
                name: index_name.to_string(),
                is_unique: false,
            })
            .collect_vec();

        let schema = TableDef {
            table_name,
            columns,
            indexes,
            table_type,
            table_access,
        };

        db.create_table(tx, schema)?;
        Ok(())
    }

    #[test]
    fn compile_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] without any indexes
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Compile query
        let sql = "select * from test where a = 1";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert no index scan
        let Query::Select(_) = ops.remove(0) else {
            panic!("Expected Select");
        };
        Ok(())
    }

    #[test]
    fn compile_index_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with index on [a]
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[(0, "a")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Compile query
        let sql = "select * from test where a = 1";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert index scan
        let Query::IndexScan(IndexScan {
            table: _,
            col_id,
            lower_bound: Bound::Included(u),
            upper_bound: Bound::Included(v),
        }) = ops.remove(0)
        else {
            panic!("Expected IndexScan");
        };
        assert_eq!(u, v);
        assert_eq!(col_id, 0);
        assert_eq!(v, AlgebraicValue::U64(1));
        Ok(())
    }

    #[test]
    fn compile_eq_and_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Note, order matters - the sargable predicate occurs last which means
        // no index scan will be generated.
        let sql = "select * from test where a = 1 and b = 2";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert no index scan
        let Query::Select(_) = ops.remove(0) else {
            panic!("Expected Select");
        };
        Ok(())
    }

    #[test]
    fn compile_index_eq_and_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Note, order matters - the sargable predicate occurs first which
        // means an index scan will be generated.
        let sql = "select * from test where b = 2 and a = 1";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(2, ops.len());

        // Assert index scan
        let Query::IndexScan(IndexScan {
            table: _,
            col_id,
            lower_bound: Bound::Included(u),
            upper_bound: Bound::Included(v),
        }) = ops.remove(0)
        else {
            panic!("Expected IndexScan");
        };
        assert_eq!(u, v);
        assert_eq!(col_id, 1);
        assert_eq!(v, AlgebraicValue::U64(2));
        Ok(())
    }

    #[test]
    fn compile_eq_or_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0, "a"), (1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Compile query
        let sql = "select * from test where a = 1 or b = 2";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert no index scan because OR is not sargable
        let Query::Select(_) = ops.remove(0) else {
            panic!("Expected Select");
        };
        Ok(())
    }

    #[test]
    fn compile_index_range_open() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Compile query
        let sql = "select * from test where b > 2";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert index scan
        let Query::IndexScan(IndexScan {
            table: _,
            col_id: 1,
            lower_bound: Bound::Excluded(value),
            upper_bound: Bound::Unbounded,
        }) = ops.remove(0)
        else {
            panic!("Expected IndexScan");
        };
        assert_eq!(value, AlgebraicValue::U64(2));
        Ok(())
    }

    #[test]
    fn compile_index_range_closed() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Compile query
        let sql = "select * from test where b > 2 and b < 5";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert index scan
        let Query::IndexScan(IndexScan {
            table: _,
            col_id: 1,
            lower_bound: Bound::Excluded(u),
            upper_bound: Bound::Excluded(v),
        }) = ops.remove(0)
        else {
            panic!("Expected IndexScan");
        };
        assert_eq!(u, AlgebraicValue::U64(2));
        assert_eq!(v, AlgebraicValue::U64(5));
        Ok(())
    }

    #[test]
    fn compile_index_eq_select_range() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_tx();

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0, "a"), (1, "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;

        // Note, order matters - the equality condition occurs first which
        // means an index scan will be generated it rather than the range
        // condition.
        let sql = "select * from test where a = 3 and b > 2 and b < 5";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(2, ops.len());

        // Assert index scan
        let Query::IndexScan(IndexScan {
            table: _,
            col_id: 0,
            lower_bound: Bound::Included(u),
            upper_bound: Bound::Included(v),
        }) = ops.remove(0)
        else {
            panic!("Expected IndexScan");
        };

        assert_eq!(u, v);

        let Query::Select(_) = ops.remove(0) else {
            panic!("Expected Select");
        };
        Ok(())
    }
}
