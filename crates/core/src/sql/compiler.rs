use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, PlanError};
use crate::sql::ast::{compile_to_ast, Column, From, Join, Selection, SqlAst};
use spacetimedb_sats::db::auth::StAccess;
use spacetimedb_sats::db::def::{TableDef, TableSchema};
use spacetimedb_sats::relation::{self, DbTable, FieldExpr, FieldName, Header};
use spacetimedb_vm::dsl::{db_table, db_table_raw, query};
use spacetimedb_vm::expr::{ColumnOp, CrudExpr, DbType, Expr, QueryExpr, SourceExpr};
use spacetimedb_vm::operator::OpCmp;
use std::collections::HashMap;
use std::sync::Arc;

use super::ast::TableSchemaView;

/// Compile the `SQL` expression into an `ast`
pub fn compile_sql<T: TableSchemaView>(db: &RelationalDB, tx: &T, sql_text: &str) -> Result<Vec<CrudExpr>, DBError> {
    tracing::trace!(sql = sql_text);
    let ast = compile_to_ast(db, tx, sql_text)?;

    // TODO(perf, bikeshedding): SmallVec?
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
            let f = table.find_field(&x)?;

            Ok(FieldExpr::Name(f.into()))
        }
        Expr::Value(x) => Ok(FieldExpr::Value(x)),
        x => unreachable!("Wrong expression in SQL query {:?}", x),
    }
}

fn check_field(table: &From, field: &FieldExpr) -> Result<(), PlanError> {
    if let FieldExpr::Name(field) = field {
        table.find_field(&field.to_string())?;
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
    for op in filter.clause.flatten_ands() {
        q = q.with_select(op);
    }
    Ok(q)
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

    let source_expr = SourceExpr::DbTable(db_table_raw(
        &table.root,
        table.root.table_id,
        table.root.table_type,
        table.root.table_access,
    ));

    let mut q = query(source_expr);

    if let Some(ref joins) = table.join {
        for join in joins {
            match join {
                Join::Inner { rhs, on } => {
                    let rhs_source_expr = SourceExpr::DbTable(db_table(rhs, rhs.table_id));
                    match on.op {
                        OpCmp::Eq => {}
                        x => unreachable!("Unsupported operator `{x}` for joins"),
                    }
                    // Always construct inner joins, never semijoins.
                    // The query optimizer can rewrite certain inner joins into semijoins later in the pipeline.
                    q = q.with_join_inner(rhs_source_expr, on.lhs.clone(), on.rhs.clone(), false);
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
        Arc::new(Header::new(table.table_name.clone(), new, table.get_constraints())),
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
    let source_expr = SourceExpr::DbTable(compile_columns(&table, columns));

    let mut rows = Vec::with_capacity(values.len());
    for x in values {
        let mut row = Vec::with_capacity(x.len());
        for v in x {
            match v {
                FieldExpr::Name(x) => {
                    todo!("Deal with idents in insert?: {}", x)
                }
                FieldExpr::Value(x) => {
                    row.push(x);
                }
            }
        }
        rows.push(row.into())
    }

    Ok(CrudExpr::Insert {
        source: source_expr,
        rows,
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
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::make_test_db;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::operator::OpQuery;
    use spacetimedb_primitives::{col_list, ColList, TableId};
    use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue};
    use spacetimedb_vm::expr::{IndexJoin, IndexScan, JoinExpr, Query};
    use std::ops::Bound;

    fn assert_index_scan(
        op: &Query,
        cols: impl Into<ColList>,
        low_bound: Bound<AlgebraicValue>,
        up_bound: Bound<AlgebraicValue>,
    ) -> TableId {
        if let Query::IndexScan(IndexScan { table, columns, bounds }) = op {
            assert_eq!(columns, &cols.into(), "Columns don't match");
            assert_eq!(bounds.0, low_bound, "Lower bound don't match");
            assert_eq!(bounds.1, up_bound, "Upper bound don't match");
            table.table_id
        } else {
            panic!("Expected IndexScan, got {op}");
        }
    }

    fn assert_one_eq_index_scan(op: &Query, cols: impl Into<ColList>, val: AlgebraicValue) -> TableId {
        let val = Bound::Included(val);
        assert_index_scan(op, cols, val.clone(), val)
    }

    fn assert_select(op: &Query) {
        assert!(matches!(op, Query::Select(_)));
    }

    #[test]
    fn compile_eq() -> ResultTest<()> {
        let (db, _) = make_test_db()?;

        // Create table [test] without any indexes
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Compile query
        let sql = "select * from test where a = 1";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        assert_select(&query[0]);
        Ok(())
    }

    #[test]
    fn compile_not_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with cols [a, b] and index on [b].
        db.create_table_for_test(
            "test",
            &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)],
            &[(1.into(), "b"), (0.into(), "a")],
        )?;

        let tx = db.begin_tx();
        // Should work with any qualified field.
        let sql = "select * from test where a = 1 and b <> 3";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(2, query.len());
        assert_one_eq_index_scan(&query[0], 0, 1u64.into());
        assert_select(&query[1]);
        Ok(())
    }

    #[test]
    fn compile_index_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with index on [a]
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        //Compile query
        let sql = "select * from test where a = 1";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        assert_one_eq_index_scan(&query[0], 0, 1u64.into());
        Ok(())
    }

    #[test]
    fn compile_eq_and_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Note, order does not matter matter.
        // The sargable predicate occurs last but we can still generate an index scan.
        let sql = "select * from test where a = 1 and b = 2";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(2, query.len());
        assert_one_eq_index_scan(&query[0], 1, 2u64.into());
        assert_select(&query[1]);
        Ok(())
    }

    #[test]
    fn compile_index_eq_and_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Note, order does not matters.
        // The sargable predicate occurs first adn we can generate an index scan.
        let sql = "select * from test where b = 2 and a = 1";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(2, query.len());
        assert_one_eq_index_scan(&query[0], 1, 2u64.into());
        assert_select(&query[1]);
        Ok(())
    }

    #[test]
    fn compile_index_multi_eq_and_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with index on [b]
        let schema = &[
            ("a", AlgebraicType::U64),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        db.create_table_for_test_multi_column("test", schema, col_list![0, 1])?;

        let tx = db.begin_mut_tx(IsolationLevel::Serializable);
        let sql = "select * from test where b = 2 and a = 1";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        assert_one_eq_index_scan(&query[0], col_list![0, 1], product![1u64, 2u64].into());
        Ok(())
    }

    #[test]
    fn compile_eq_or_eq() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a"), (1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Compile query
        let sql = "select * from test where a = 1 or b = 2";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        // Assert no index scan because OR is not sargable.
        assert_select(&query[0]);
        Ok(())
    }

    #[test]
    fn compile_index_range_open() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Compile query
        let sql = "select * from test where b > 2";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        assert_index_scan(&query[0], 1, Bound::Excluded(AlgebraicValue::U64(2)), Bound::Unbounded);

        Ok(())
    }

    #[test]
    fn compile_index_range_closed() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Compile query
        let sql = "select * from test where b > 2 and b < 5";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(1, query.len());
        assert_index_scan(
            &query[0],
            1,
            Bound::Excluded(AlgebraicValue::U64(2)),
            Bound::Excluded(AlgebraicValue::U64(5)),
        );

        Ok(())
    }

    #[test]
    fn compile_index_eq_select_range() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a"), (1.into(), "b")];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = db.begin_tx();
        // Note, order matters - the equality condition occurs first which
        // means an index scan will be generated rather than the range condition.
        let sql = "select * from test where a = 3 and b > 2 and b < 5";
        let CrudExpr::Query(QueryExpr { source: _, query }) = compile_sql(&db, &tx, sql)?.remove(0) else {
            panic!("Expected QueryExpr");
        };
        assert_eq!(2, query.len());
        assert_one_eq_index_scan(&query[0], 0, 3u64.into());
        assert_select(&query[1]);
        Ok(())
    }

    #[test]
    fn compile_join_lhs_push_down() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

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
        let table_id = assert_one_eq_index_scan(&query[0], 0, 3u64.into());

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
            semi: false,
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

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = db.create_table_for_test("lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = db.create_table_for_test("rhs", schema, &[])?;

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
            semi: false,
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

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = db.create_table_for_test("lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = db.create_table_for_test("rhs", schema, &[])?;

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
            semi: false,
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

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [c]
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[(1.into(), "c")];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

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
        let table_id = assert_one_eq_index_scan(&query[0], 0, 3u64.into());

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
            semi: false,
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
        let table_id = assert_index_scan(&rhs[0], 1, Bound::Unbounded, Bound::Excluded(AlgebraicValue::U64(4)));

        assert_eq!(table_id, rhs_id);
        Ok(())
    }

    #[test]
    fn compile_index_join() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[(0.into(), "b"), (1.into(), "c")];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

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
            index_side: SourceExpr::DbTable(DbTable {
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
            &rhs[0],
            1,
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

    #[test]
    fn compile_index_multi_join() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(1.into(), "b")];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = col_list![0, 1];
        let rhs_id = db.create_table_for_test_multi_column("rhs", schema, indexes)?;

        let tx = db.begin_tx();
        // Should generate an index join since there is an index on `lhs.b`.
        // Should push the sargable range condition into the index join's probe side.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c = 2 and rhs.b = 4 and rhs.d = 3";
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
            index_side: SourceExpr::DbTable(DbTable {
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
        let table_id = assert_one_eq_index_scan(&rhs[0], col_list![0, 1], product![4u64, 2u64].into());

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

    #[test]
    fn compile_check_ambiguous_field() -> ResultTest<()> {
        let (db, _tmp) = make_test_db()?;

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[(0.into(), "a")];
        db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[];
        db.create_table_for_test("rhs", schema, indexes)?;

        let tx = db.begin_tx();
        // Should work with any qualified field
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where lhs.b = 3";
        assert!(compile_sql(&db, &tx, sql).is_ok());
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where lhs.a = 3";
        assert!(compile_sql(&db, &tx, sql).is_ok());
        // Should work with any unqualified but unique field
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where a = 3";
        assert!(compile_sql(&db, &tx, sql).is_ok());
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where c = 3";
        assert!(compile_sql(&db, &tx, sql).is_ok());
        // ... and fail on ambiguous
        let sql = "select * from lhs join rhs on lhs.b = rhs.b where b = 3";
        match compile_sql(&db, &tx, sql) {
            Err(DBError::Plan {
                error: PlanError::AmbiguousField { field, found },
                ..
            }) => {
                assert_eq!(field, "b");
                assert_eq!(found, vec![FieldName::named("lhs", "b"), FieldName::named("rhs", "b")]);
            }
            _ => {
                panic!("Unexpected")
            }
        }
        Ok(())
    }
}
