use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, PlanError};
use crate::sql::ast::{compile_to_ast, Column, From, Join, Selection, SqlAst};
use spacetimedb_primitives::*;
use spacetimedb_sats::db::auth::StAccess;
use spacetimedb_sats::db::def::{TableDef, TableSchema};
use spacetimedb_sats::relation::{self, DbTable, FieldExpr, FieldName, Header};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_vm::dsl::{db_table, db_table_raw, query};
use spacetimedb_vm::expr::{ColumnOp, CrudExpr, DbType, Expr, QueryExpr, SourceExpr};
use spacetimedb_vm::operator::OpCmp;
use std::collections::HashMap;

use super::ast::TableSchemaView;

/// Compile the `SQL` expression into a `ast`
#[tracing::instrument(skip_all)]
pub fn compile_sql<T: TableSchemaView>(db: &RelationalDB, tx: &T, sql_text: &str) -> Result<Vec<CrudExpr>, DBError> {
    tracing::trace!(sql = sql_text);
    let ast = compile_to_ast(db, tx, sql_text)?;

    let mut results = Vec::with_capacity(ast.len());

    for sql in ast {
        results.push(compile_statement(db, sql).map_err(|error| DBError::Plan {
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

            Ok(FieldExpr::Name(f.into()))
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
    for op in filter.clause.to_vec() {
        q = q.with_select(op);
    }
    Ok(q)
}

// IndexArgument represents an equality or range predicate that can be answered
// using an index.
pub enum IndexArgument {
    Eq {
        columns: ColList,
        value: AlgebraicValue,
    },
    LowerBound {
        columns: ColList,
        value: AlgebraicValue,
        inclusive: bool,
    },
    UpperBound {
        columns: ColList,
        value: AlgebraicValue,
        inclusive: bool,
    },
}

/// Compiles a `SELECT ...` clause
fn compile_select(table: From, project: Vec<Column>, selection: Option<Selection>) -> Result<QueryExpr, PlanError> {
    let mut not_found = Vec::with_capacity(project.len());
    let mut col_ids = Vec::new();
    let mut qualified_wildcards = Vec::new();
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
                    for c in t.columns().iter() {
                        col_ids.push(FieldName::named(&t.table_name, &c.col_name).into());
                    }
                    qualified_wildcards.push(t.table_id);
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
        &table.root,
        table.root.table_id,
        table.root.table_type,
        table.root.table_access,
    ));

    if let Some(ref joins) = table.join {
        for join in joins {
            match join {
                Join::Inner { rhs, on } => {
                    let t = db_table(rhs, rhs.table_id);
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
    // It is important to project at the end.
    // This is so joins and filters see fields that are not projected.
    // It is also important to identify a wildcard project of the form `table.*`.
    // This implies a potential semijoin and additional optimization opportunities.
    let qualified_wildcard = if qualified_wildcards.len() == 1 {
        Some(qualified_wildcards[0])
    } else {
        None
    };
    q = q.with_project(&col_ids, qualified_wildcard);

    Ok(q)
}

/// Builds the schema description [DbTable] from the [TableSchema] and their list of columns
fn compile_columns(table: &TableSchema, columns: Vec<FieldName>) -> DbTable {
    let mut new = Vec::with_capacity(columns.len());

    for col in columns.into_iter() {
        if let Some(x) = table.get_column_by_field(&col) {
            let field = FieldName::named(&table.table_name, &x.col_name);
            new.push(relation::Column::new(field, x.col_type.clone(), x.col_pos));
        }
    }
    DbTable::new(
        Header::new(table.table_name.clone(), new, table.get_constraints()),
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

    Ok(CrudExpr::Update { delete, assignments })
}

/// Compiles a `CREATE TABLE ...` clause
fn compile_create_table(table: TableDef) -> Result<CrudExpr, PlanError> {
    Ok(CrudExpr::CreateTable { table })
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
fn compile_statement(db: &RelationalDB, statement: SqlAst) -> Result<CrudExpr, PlanError> {
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
        SqlAst::CreateTable { table } => compile_create_table(table)?,
        SqlAst::Drop {
            name,
            kind,
            table_access,
        } => compile_drop(name, kind, table_access)?,
    };

    Ok(q.optimize(&|table_id, table_name| db.row_count(table_id, table_name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Bound;

    use crate::db::relational_db::{tests_utils::make_test_db, MutTx};
    use crate::execution_context::ExecutionContext;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::operator::OpQuery;
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::db::auth::StTableType;
    use spacetimedb_sats::db::def::{ColumnDef, IndexDef, TableDef};
    use spacetimedb_sats::relation::Table;
    use spacetimedb_sats::AlgebraicType;
    use spacetimedb_vm::expr::{IndexJoin, IndexScan, JoinExpr, Query};

    fn create_table(
        db: &RelationalDB,
        tx: &mut MutTx,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(ColId, &str)],
    ) -> ResultTest<TableId> {
        let table_name = name.to_string();
        let table_type = StTableType::User;
        let table_access = StAccess::Public;

        let columns: Vec<_> = schema
            .iter()
            .map(|(col_name, col_type)| ColumnDef {
                col_name: col_name.to_string(),
                col_type: col_type.clone(),
            })
            .collect();

        let indexes: Vec<_> = indexes
            .iter()
            .map(|(col_id, index_name)| IndexDef::btree(index_name.to_string(), *col_id, false))
            .collect();

        let schema = TableDef::new(table_name, columns)
            .with_indexes(indexes)
            .with_type(table_type)
            .with_access(table_access);

        Ok(db.create_table(tx, schema)?)
    }

    fn assert_index_scan(
        op: Query,
        col: ColId,
        low_bound: Bound<AlgebraicValue>,
        up_bound: Bound<AlgebraicValue>,
    ) -> TableId {
        if let Query::IndexScan(IndexScan {
            table,
            columns,
            lower_bound,
            upper_bound,
        }) = op
        {
            assert_eq!(columns, col.into(), "Columns don't match");
            assert_eq!(lower_bound, low_bound, "Lower bound don't match");
            assert_eq!(upper_bound, up_bound, "Upper bound don't match");
            table.table_id
        } else {
            panic!("Expected IndexScan, got {op}");
        }
    }

    #[test]
    fn compile_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] without any indexes
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with index on [a]
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        //Compile query
        let sql = "select * from test where a = 1";
        let CrudExpr::Query(QueryExpr {
            source: _,
            query: mut ops,
        }) = compile_sql(&db, &tx, sql)?.remove(0)
        else {
            panic!("Expected QueryExpr");
        };

        assert_eq!(1, ops.len());

        // Assert index scan.
        assert_index_scan(
            ops.swap_remove(0),
            0.into(),
            Bound::Included(1u64.into()),
            Bound::Included(1u64.into()),
        );
        Ok(())
    }

    #[test]
    fn compile_eq_and_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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

        // Assert index scan.
        assert_index_scan(
            ops.swap_remove(0),
            1.into(),
            Bound::Included(2u64.into()),
            Bound::Included(2u64.into()),
        );
        Ok(())
    }

    #[test]
    fn compile_eq_or_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a"), (1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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

        assert_index_scan(
            ops.remove(0),
            1.into(),
            Bound::Excluded(AlgebraicValue::U64(2)),
            Bound::Unbounded,
        );

        Ok(())
    }

    #[test]
    fn compile_index_range_closed() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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

        assert_index_scan(
            ops.remove(0),
            1.into(),
            Bound::Excluded(AlgebraicValue::U64(2)),
            Bound::Excluded(AlgebraicValue::U64(5)),
        );

        Ok(())
    }

    #[test]
    fn compile_index_eq_select_range() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a"), (1.into(), "b")];
        create_table(&db, &mut tx, "test", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
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

        assert_index_scan(
            ops.remove(0),
            0.into(),
            Bound::Included(AlgebraicValue::U64(3)),
            Bound::Included(AlgebraicValue::U64(3)),
        );

        let Query::Select(_) = ops.remove(0) else {
            panic!("Expected Select");
        };
        Ok(())
    }

    #[test]
    fn compile_join_lhs_push_down() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should push sargable equality condition below join
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where lhs.a = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: SourceExpr::DbTable(DbTable { table_id, .. }),
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };

        assert_eq!(table_id, lhs_id);
        assert_eq!(query.len(), 2);

        // First operation in the pipeline should be an index scan
        let table_id = assert_index_scan(
            query[0].clone(),
            0.into(),
            Bound::Included(AlgebraicValue::U64(3)),
            Bound::Included(AlgebraicValue::U64(3)),
        );

        assert_eq!(table_id, lhs_id);

        // Followed by a join with the rhs table
        let Query::JoinInner(JoinExpr {
            rhs:
                QueryExpr {
                    source: SourceExpr::DbTable(DbTable { table_id, .. }),
                    ..
                },
            col_lhs:
                FieldName::Name {
                    table: ref lhs_table,
                    field: ref lhs_field,
                },
            col_rhs:
                FieldName::Name {
                    table: ref rhs_table,
                    field: ref rhs_field,
                },
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(table_id, rhs_id);
        assert_eq!(lhs_field, "b");
        assert_eq!(rhs_field, "b");
        assert_eq!(lhs_table, "lhs");
        assert_eq!(rhs_table, "rhs");
        Ok(())
    }

    #[test]
    fn compile_join_lhs_push_down_no_index() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, &[])?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should push equality condition below join
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where lhs.a = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: SourceExpr::DbTable(DbTable { table_id, .. }),
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };

        assert_eq!(table_id, lhs_id);
        assert_eq!(query.len(), 2);

        // The first operation in the pipeline should be a selection
        let Query::Select(ColumnOp::Cmp {
            op: OpQuery::Cmp(OpCmp::Eq),
            ref lhs,
            ref rhs,
        }) = query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        let ColumnOp::Field(FieldExpr::Name(FieldName::Name { ref table, ref field })) = **lhs else {
            panic!("unexpected left hand side {:#?}", **lhs);
        };

        assert_eq!(table, "lhs");
        assert_eq!(field, "a");

        let ColumnOp::Field(FieldExpr::Value(AlgebraicValue::U64(3))) = **rhs else {
            panic!("unexpected right hand side {:#?}", **rhs);
        };

        // The join should follow the selection
        let Query::JoinInner(JoinExpr {
            rhs:
                QueryExpr {
                    source: SourceExpr::DbTable(DbTable { table_id, .. }),
                    query: ref rhs,
                },
            col_lhs:
                FieldName::Name {
                    table: ref lhs_table,
                    field: ref lhs_field,
                },
            col_rhs:
                FieldName::Name {
                    table: ref rhs_table,
                    field: ref rhs_field,
                },
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(table_id, rhs_id);
        assert_eq!(lhs_field, "b");
        assert_eq!(rhs_field, "b");
        assert_eq!(lhs_table, "lhs");
        assert_eq!(rhs_table, "rhs");
        assert!(rhs.is_empty());
        Ok(())
    }

    #[test]
    fn compile_join_rhs_push_down_no_index() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, &[])?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should push equality condition below join
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where rhs.c = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: SourceExpr::DbTable(DbTable { table_id, .. }),
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };

        assert_eq!(table_id, lhs_id);
        assert_eq!(query.len(), 1);

        // First and only operation in the pipeline should be a join
        let Query::JoinInner(JoinExpr {
            rhs:
                QueryExpr {
                    source: SourceExpr::DbTable(DbTable { table_id, .. }),
                    query: ref rhs,
                },
            col_lhs:
                FieldName::Name {
                    table: ref lhs_table,
                    field: ref lhs_field,
                },
            col_rhs:
                FieldName::Name {
                    table: ref rhs_table,
                    field: ref rhs_field,
                },
        }) = query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        assert_eq!(table_id, rhs_id);
        assert_eq!(lhs_field, "b");
        assert_eq!(rhs_field, "b");
        assert_eq!(lhs_table, "lhs");
        assert_eq!(rhs_table, "rhs");

        // The selection should be pushed onto the rhs of the join
        let Query::Select(ColumnOp::Cmp {
            op: OpQuery::Cmp(OpCmp::Eq),
            ref lhs,
            ref rhs,
        }) = rhs[0]
        else {
            panic!("unexpected operator {:#?}", rhs[0]);
        };

        let ColumnOp::Field(FieldExpr::Name(FieldName::Name { ref table, ref field })) = **lhs else {
            panic!("unexpected left hand side {:#?}", **lhs);
        };

        assert_eq!(table, "rhs");
        assert_eq!(field, "c");

        let ColumnOp::Field(FieldExpr::Value(AlgebraicValue::U64(3))) = **rhs else {
            panic!("unexpected right hand side {:#?}", **rhs);
        };
        Ok(())
    }

    #[test]
    fn compile_join_lhs_and_rhs_push_down() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with index on [c]
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[(1.into(), "c")];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should push the sargable equality condition into the join's left arg.
        // Should push the sargable range condition into the join's right arg.
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where lhs.a = 3 and rhs.c < 4";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: SourceExpr::DbTable(DbTable { table_id, .. }),
            query,
            ..
        }) = exp
        else {
            panic!("unexpected result from compilation: {:?}", exp);
        };

        assert_eq!(table_id, lhs_id);
        assert_eq!(query.len(), 2);

        // First operation in the pipeline should be an index scan
        let table_id = assert_index_scan(
            query[0].clone(),
            0.into(),
            Bound::Included(AlgebraicValue::U64(3)),
            Bound::Included(AlgebraicValue::U64(3)),
        );

        assert_eq!(table_id, lhs_id);

        // Followed by a join
        let Query::JoinInner(JoinExpr {
            rhs:
                QueryExpr {
                    source: SourceExpr::DbTable(DbTable { table_id, .. }),
                    query: ref rhs,
                },
            col_lhs:
                FieldName::Name {
                    table: ref lhs_table,
                    field: ref lhs_field,
                },
            col_rhs:
                FieldName::Name {
                    table: ref rhs_table,
                    field: ref rhs_field,
                },
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(table_id, rhs_id);
        assert_eq!(lhs_field, "b");
        assert_eq!(rhs_field, "b");
        assert_eq!(lhs_table, "lhs");
        assert_eq!(rhs_table, "rhs");

        assert_eq!(1, rhs.len());

        // The right side of the join should be an index scan
        let table_id = assert_index_scan(
            rhs[0].clone(),
            1.into(),
            Bound::Unbounded,
            Bound::Excluded(AlgebraicValue::U64(4)),
        );

        assert_eq!(table_id, rhs_id);
        Ok(())
    }

    #[test]
    fn compile_index_join() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;
        let mut tx = db.begin_mut_tx();

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = create_table(&db, &mut tx, "lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let rhs_id = create_table(&db, &mut tx, "rhs", schema, indexes)?;
        db.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = db.begin_tx();
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c > 2 and rhs.c < 4 and rhs.d = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: SourceExpr::DbTable(DbTable { table_id, .. }),
            query,
            ..
        }) = exp
        else {
            panic!("unexpected result from compilation: {:?}", exp);
        };

        assert_eq!(table_id, lhs_id);
        assert_eq!(query.len(), 1);

        let Query::IndexJoin(IndexJoin {
            probe_side:
                QueryExpr {
                    source: SourceExpr::DbTable(DbTable { table_id, .. }),
                    query: ref rhs,
                },
            probe_field:
                FieldName::Name {
                    table: ref probe_table,
                    field: ref probe_field,
                },
            index_side: Table::DbTable(DbTable {
                table_id: index_table, ..
            }),
            index_col,
            ..
        }) = query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        assert_eq!(table_id, rhs_id);
        assert_eq!(index_table, lhs_id);
        assert_eq!(index_col, 1.into());
        assert_eq!(probe_field, "b");
        assert_eq!(probe_table, "rhs");

        assert_eq!(2, rhs.len());

        // The probe side of the join should be an index scan
        let table_id = assert_index_scan(
            rhs[0].clone(),
            1.into(),
            Bound::Excluded(AlgebraicValue::U64(2)),
            Bound::Excluded(AlgebraicValue::U64(4)),
        );

        assert_eq!(table_id, rhs_id);

        // Followed by a selection
        let Query::Select(ColumnOp::Cmp {
            op: OpQuery::Cmp(OpCmp::Eq),
            lhs: ref field,
            rhs: ref value,
        }) = rhs[1]
        else {
            panic!("unexpected operator {:#?}", rhs[0]);
        };

        let ColumnOp::Field(FieldExpr::Name(FieldName::Name { ref table, ref field })) = **field else {
            panic!("unexpected left hand side {:#?}", field);
        };

        assert_eq!(table, "rhs");
        assert_eq!(field, "d");

        let ColumnOp::Field(FieldExpr::Value(AlgebraicValue::U64(3))) = **value else {
            panic!("unexpected right hand side {:#?}", value);
        };
        Ok(())
    }
}
