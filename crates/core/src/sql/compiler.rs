use super::ast::TableSchemaView;
use super::ast::{compile_to_ast, Column, From, Join, Selection, SqlAst};
use super::type_check::TypeCheck;
use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, PlanError};
use core::ops::Deref;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{self, ColExpr, DbTable, FieldName, Header};
use spacetimedb_primitives::ColId;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_vm::expr::{CrudExpr, Expr, FieldExpr, QueryExpr, SourceExpr};
use spacetimedb_vm::operator::OpCmp;
use std::sync::Arc;

/// DIRTY HACK ALERT: Maximum allowed length, in UTF-8 bytes, of SQL queries.
/// Any query longer than this will be rejected.
/// This prevents a stack overflow when compiling queries with deeply-nested `AND` and `OR` conditions.
const MAX_SQL_LENGTH: usize = 50_000;

/// Compile the `SQL` expression into an `ast`
pub fn compile_sql<T: TableSchemaView + StateView>(
    db: &RelationalDB,
    auth: &AuthCtx,
    tx: &T,
    sql_text: &str,
) -> Result<Vec<CrudExpr>, DBError> {
    if sql_text.len() > MAX_SQL_LENGTH {
        return Err(anyhow::anyhow!("SQL query exceeds maximum allowed length: \"{sql_text:.120}...\"").into());
    }
    tracing::trace!(sql = sql_text);
    let ast = compile_to_ast(db, auth, tx, sql_text)?;

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
        Expr::Ident(x) => table.find_field(&x).map(|(f, _)| FieldExpr::Name(f)),
        Expr::Value(x) => Ok(FieldExpr::Value(x)),
        x => unreachable!("Wrong expression in SQL query {:?}", x),
    }
}

/// Compiles a `WHERE ...` clause
fn compile_where(mut q: QueryExpr, filter: Selection) -> Result<QueryExpr, PlanError> {
    for op in filter.clause.flatten_ands() {
        q = q.with_select(op)?;
    }
    Ok(q)
}

/// Compiles a `SELECT ...` clause
fn compile_select(table: From, project: Box<[Column]>, selection: Option<Selection>) -> Result<QueryExpr, PlanError> {
    let mut not_found = Vec::with_capacity(project.len());
    let mut col_ids = Vec::new();
    let mut qualified_wildcards = Vec::new();
    //Match columns to their tables...
    for select_item in Vec::from(project) {
        match select_item {
            Column::UnnamedExpr(x) => match expr_for_projection(&table, x) {
                Ok(field) => col_ids.push(field),
                Err(PlanError::UnknownField { field, tables: _ }) => not_found.push(field),
                Err(err) => return Err(err),
            },
            Column::QualifiedWildcard { table: name } => {
                if let Some(t) = table.iter_tables().find(|x| *x.table_name == name) {
                    for c in t.columns().iter() {
                        col_ids.push(FieldName::new(t.table_id, c.col_pos).into());
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

    let source_expr: SourceExpr = table.root.deref().into();
    let mut q = QueryExpr::new(source_expr);

    for join in table.joins {
        match join {
            Join::Inner { rhs, on } => {
                let col_lhs = q.head().column_pos_or_err(on.lhs)?;
                let rhs_source_expr: SourceExpr = rhs.deref().into();
                let col_rhs = rhs_source_expr.head().column_pos_or_err(on.rhs)?;

                match on.op {
                    OpCmp::Eq => {}
                    x => unreachable!("Unsupported operator `{x}` for joins"),
                }
                // Always construct inner joins, never semijoins.
                // The query optimizer can rewrite certain inner joins into semijoins later in the pipeline.
                // The full pipeline for a query like `SELECT lhs.* FROM lhs JOIN rhs ON lhs.a = rhs.a` is:
                // - We produce `[JoinInner(semi: false), Project]`.
                // - Optimizer rewrites to `[JoinInner(semi: true)]`.
                // - Optimizer rewrites to `[IndexJoin]`.
                // For incremental queries, this all happens on the original query with `DbTable` sources.
                // Then, the query is "incrementalized" by replacing the sources with `MemTable`s,
                // and the `IndexJoin` is rewritten back into a `JoinInner(semi: true)`.
                q = q.with_join_inner(rhs_source_expr, col_lhs, col_rhs, false);
            }
        }
    }

    if let Some(filter) = selection {
        q = compile_where(q, filter)?;
    }
    // It is important to project at the end.
    // This is so joins and filters see fields that are not projected.
    // It is also important to identify a wildcard project of the form `table.*`.
    // This implies a potential semijoin and additional optimization opportunities.
    let qualified_wildcard = (qualified_wildcards.len() == 1).then(|| qualified_wildcards[0]);
    q = q.with_project(col_ids, qualified_wildcard)?;

    Ok(q)
}

/// Builds the schema description [DbTable] from the [TableSchema] and their list of columns
fn compile_columns(table: &TableSchema, cols: &[ColId]) -> DbTable {
    let mut columns = Vec::with_capacity(cols.len());
    let cols = cols
        .iter()
        // TODO: should we error here instead?
        // When would the user be passing in columns that aren't present?
        .filter_map(|col| table.get_column(col.idx()))
        .map(|col| relation::Column::new(FieldName::new(table.table_id, col.col_pos), col.col_type.clone()));
    columns.extend(cols);

    let header = Header::from(table).project_col_list(&columns.iter().map(|x| x.field.col).collect());

    DbTable::new(Arc::new(header), table.table_id, table.table_type, table.table_access)
}

/// Compiles a `INSERT ...` clause
fn compile_insert(table: &TableSchema, cols: &[ColId], values: Box<[Box<[ColExpr]>]>) -> CrudExpr {
    let table = compile_columns(table, cols);

    let mut rows = Vec::with_capacity(values.len());
    for x in Vec::from(values) {
        let mut row = Vec::with_capacity(x.len());
        for v in Vec::from(x) {
            match v {
                ColExpr::Col(x) => {
                    todo!("Deal with idents in insert?: {}", x)
                }
                ColExpr::Value(x) => {
                    row.push(x);
                }
            }
        }
        rows.push(row.into())
    }

    CrudExpr::Insert { table, rows }
}

/// Compiles a `DELETE ...` clause
fn compile_delete(table: Arc<TableSchema>, selection: Option<Selection>) -> Result<CrudExpr, PlanError> {
    let query = QueryExpr::new(&*table);
    let query = if let Some(filter) = selection {
        compile_where(query, filter)?
    } else {
        query
    };
    Ok(CrudExpr::Delete { query })
}

/// Compiles a `UPDATE ...` clause
fn compile_update(
    table: Arc<TableSchema>,
    assignments: IntMap<ColId, ColExpr>,
    selection: Option<Selection>,
) -> Result<CrudExpr, PlanError> {
    let query = QueryExpr::new(&*table);
    let delete = if let Some(filter) = selection {
        compile_where(query, filter)?
    } else {
        query
    };

    Ok(CrudExpr::Update { delete, assignments })
}

/// Compiles a `SQL` clause
fn compile_statement(db: &RelationalDB, statement: SqlAst) -> Result<CrudExpr, PlanError> {
    statement.type_check()?;

    let q = match statement {
        SqlAst::Select {
            from,
            project,
            selection,
        } => CrudExpr::Query(compile_select(from, project, selection)?),
        SqlAst::Insert { table, columns, values } => compile_insert(&table, &columns, values),
        SqlAst::Update {
            table,
            assignments,
            selection,
        } => compile_update(table, assignments, selection)?,
        SqlAst::Delete { table, selection } => compile_delete(table, selection)?,
        SqlAst::SetVar { name, literal } => CrudExpr::SetVar { name, literal },
        SqlAst::ReadVar { name } => CrudExpr::ReadVar { name },
    };

    Ok(q.optimize(&|table_id, table_name| db.row_count(table_id, table_name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::{begin_mut_tx, begin_tx, insert, with_auto_commit, TestDB};
    use crate::sql::execute::tests::run_for_testing;
    use spacetimedb_lib::error::{ResultTest, TestError};
    use spacetimedb_lib::{ConnectionId, Identity};
    use spacetimedb_primitives::{col_list, ColList, TableId};
    use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, GroundSpacetimeType as _};
    use spacetimedb_vm::expr::{ColumnOp, IndexJoin, IndexScan, JoinExpr, Query};
    use std::convert::From;
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

    fn compile_sql<T: TableSchemaView + StateView>(
        db: &RelationalDB,
        tx: &T,
        sql: &str,
    ) -> Result<Vec<CrudExpr>, DBError> {
        super::compile_sql(db, &AuthCtx::for_testing(), tx, sql)
    }

    #[test]
    fn compile_eq() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] without any indexes
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [test] with cols [a, b] and index on [b].
        db.create_table_for_test(
            "test",
            &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)],
            &[1.into(), 0.into()],
        )?;

        let tx = begin_tx(&db);
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
    fn compile_index_eq_basic() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [a]
        let schema = &[("a", AlgebraicType::U64)];
        let indexes = &[0.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
    fn compile_eq_identity_connection_id() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] without any indexes
        let schema = &[
            ("identity", Identity::get_type()),
            ("identity_mix", Identity::get_type()),
            ("connection_id", ConnectionId::get_type()),
        ];
        let indexes = &[];
        let table_id = db.create_table_for_test("test", schema, indexes)?;

        let row = product![
            Identity::__dummy(),
            Identity::from_hex("93dda09db9a56d8fa6c024d843e805d8262191db3b4ba84c5efcd1ad451fed4e").unwrap(),
            ConnectionId::ZERO,
        ];

        with_auto_commit(&db, |tx| {
            insert(&db, tx, table_id, &row.clone())?;
            Ok::<(), TestError>(())
        })?;

        // Check can be used by CRUD ops:
        let sql = &format!(
            "INSERT INTO test (identity, identity_mix, connection_id) VALUES ({}, x'91DDA09DB9A56D8FA6C024D843E805D8262191DB3B4BA84C5EFCD1AD451FED4E', {})",
            Identity::__dummy(),
            ConnectionId::ZERO,
        );
        run_for_testing(&db, sql)?;

        // Compile query, check for both hex formats and it to be case-insensitive...
        let sql = &format!(
            "select * from test where identity = {} AND identity_mix = x'93dda09db9a56d8fa6c024d843e805D8262191db3b4bA84c5efcd1ad451fed4e' AND connection_id = x'{}' AND connection_id = {}",
            Identity::__dummy(),
            ConnectionId::ZERO,
            ConnectionId::ZERO,
        );

        let rows = run_for_testing(&db, sql)?;

        let tx = begin_tx(&db);
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

        assert_eq!(rows, vec![row]);

        Ok(())
    }

    #[test]
    fn compile_eq_and_eq() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
        // Note, order does not matter.
        // The sargable predicate occurs last, but we can still generate an index scan.
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
        let db = TestDB::durable()?;

        // Create table [test] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
        // Note, order does not matter.
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
        let db = TestDB::durable()?;

        // Create table [test] with index on [b]
        let schema = &[
            ("a", AlgebraicType::U64),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        db.create_table_for_test_multi_column("test", schema, col_list![0, 1])?;

        let tx = begin_mut_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[0.into(), 1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [test] with indexes on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [test] with indexes on [a] and [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[0.into(), 1.into()];
        db.create_table_for_test("test", schema, indexes)?;

        let tx = begin_tx(&db);
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
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[0.into()];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
        // Should push sargable equality condition below join
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where lhs.a = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: source_lhs,
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };

        assert_eq!(source_lhs.table_id().unwrap(), lhs_id);
        assert_eq!(query.len(), 3);

        // First operation in the pipeline should be an index scan
        let table_id = assert_one_eq_index_scan(&query[0], 0, 3u64.into());

        assert_eq!(table_id, lhs_id);

        // Followed by a join with the rhs table
        let Query::JoinInner(JoinExpr {
            ref rhs,
            col_lhs,
            col_rhs,
            inner: Some(ref inner_header),
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(rhs.source.table_id().unwrap(), rhs_id);
        assert_eq!(col_lhs, 1.into());
        assert_eq!(col_rhs, 0.into());
        assert_eq!(&**inner_header, &source_lhs.head().extend(rhs.source.head()));
        Ok(())
    }

    #[test]
    fn compile_join_lhs_push_down_no_index() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = db.create_table_for_test("lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = db.create_table_for_test("rhs", schema, &[])?;

        let tx = begin_tx(&db);
        // Should push equality condition below join
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where lhs.a = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: source_lhs,
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };
        assert_eq!(source_lhs.table_id().unwrap(), lhs_id);
        assert_eq!(query.len(), 3);

        // The first operation in the pipeline should be a selection with `col#0 = 3`
        let Query::Select(ColumnOp::ColCmpVal {
            cmp: OpCmp::Eq,
            lhs: ColId(0),
            rhs: AlgebraicValue::U64(3),
        }) = query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        // The join should follow the selection
        let Query::JoinInner(JoinExpr {
            ref rhs,
            col_lhs,
            col_rhs,
            inner: Some(ref inner_header),
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(rhs.source.table_id().unwrap(), rhs_id);
        assert_eq!(col_lhs, 1.into());
        assert_eq!(col_rhs, 0.into());
        assert_eq!(&**inner_header, &source_lhs.head().extend(rhs.source.head()));
        assert!(rhs.query.is_empty());
        Ok(())
    }

    #[test]
    fn compile_join_rhs_push_down_no_index() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with no indexes
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let lhs_id = db.create_table_for_test("lhs", schema, &[])?;

        // Create table [rhs] with no indexes
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let rhs_id = db.create_table_for_test("rhs", schema, &[])?;

        let tx = begin_tx(&db);
        // Should push equality condition below join
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where rhs.c = 3";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: source_lhs,
            query,
            ..
        }) = exp
        else {
            panic!("unexpected expression: {:#?}", exp);
        };

        assert_eq!(source_lhs.table_id().unwrap(), lhs_id);
        assert_eq!(query.len(), 1);

        // First and only operation in the pipeline should be a join
        let Query::JoinInner(JoinExpr {
            ref rhs,
            col_lhs,
            col_rhs,
            inner: None,
        }) = query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        assert_eq!(rhs.source.table_id().unwrap(), rhs_id);
        assert_eq!(col_lhs, 1.into());
        assert_eq!(col_rhs, 0.into());

        // The selection should be pushed onto the rhs of the join
        let Query::Select(ColumnOp::ColCmpVal {
            cmp: OpCmp::Eq,
            lhs: ColId(1),
            rhs: AlgebraicValue::U64(3),
        }) = rhs.query[0]
        else {
            panic!("unexpected operator {:#?}", rhs.query[0]);
        };
        Ok(())
    }

    #[test]
    fn compile_join_lhs_and_rhs_push_down() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [a]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[0.into()];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [c]
        let schema = &[("b", AlgebraicType::U64), ("c", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
        // Should push the sargable equality condition into the join's left arg.
        // Should push the sargable range condition into the join's right arg.
        let sql = "select lhs.* from lhs join rhs on lhs.b = rhs.b where lhs.a = 3 and rhs.c < 4";
        let exp = compile_sql(&db, &tx, sql)?.remove(0);

        let CrudExpr::Query(QueryExpr {
            source: source_lhs,
            query,
            ..
        }) = exp
        else {
            panic!("unexpected result from compilation: {:?}", exp);
        };

        assert_eq!(source_lhs.table_id().unwrap(), lhs_id);
        assert_eq!(query.len(), 3);

        // First operation in the pipeline should be an index scan
        let table_id = assert_one_eq_index_scan(&query[0], 0, 3u64.into());

        assert_eq!(table_id, lhs_id);

        // Followed by a join
        let Query::JoinInner(JoinExpr {
            ref rhs,
            col_lhs,
            col_rhs,
            inner: Some(ref inner_header),
        }) = query[1]
        else {
            panic!("unexpected operator {:#?}", query[1]);
        };

        assert_eq!(rhs.source.table_id().unwrap(), rhs_id);
        assert_eq!(col_lhs, 1.into());
        assert_eq!(col_rhs, 0.into());
        assert_eq!(&**inner_header, &source_lhs.head().extend(rhs.source.head()));

        assert_eq!(1, rhs.query.len());

        // The right side of the join should be an index scan
        let table_id = assert_index_scan(
            &rhs.query[0],
            1,
            Bound::Unbounded,
            Bound::Excluded(AlgebraicValue::U64(4)),
        );

        assert_eq!(table_id, rhs_id);
        Ok(())
    }

    #[test]
    fn compile_index_join() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = &[0.into(), 1.into()];
        let rhs_id = db.create_table_for_test("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
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
                    query: rhs,
                },
            probe_col,
            index_side: SourceExpr::DbTable(DbTable {
                table_id: index_table, ..
            }),
            index_col,
            ..
        }) = &query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        assert_eq!(*table_id, rhs_id);
        assert_eq!(*index_table, lhs_id);
        assert_eq!(index_col, &1.into());
        assert_eq!(*probe_col, 0.into());

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
        let Query::Select(ColumnOp::ColCmpVal {
            cmp: OpCmp::Eq,
            lhs: ColId(2),
            rhs: AlgebraicValue::U64(3),
        }) = rhs[1]
        else {
            panic!("unexpected operator {:#?}", rhs[0]);
        };
        Ok(())
    }

    #[test]
    fn compile_index_multi_join() -> ResultTest<()> {
        let db = TestDB::durable()?;

        // Create table [lhs] with index on [b]
        let schema = &[("a", AlgebraicType::U64), ("b", AlgebraicType::U64)];
        let indexes = &[1.into()];
        let lhs_id = db.create_table_for_test("lhs", schema, indexes)?;

        // Create table [rhs] with index on [b, c]
        let schema = &[
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
            ("d", AlgebraicType::U64),
        ];
        let indexes = col_list![0, 1];
        let rhs_id = db.create_table_for_test_multi_column("rhs", schema, indexes)?;

        let tx = begin_tx(&db);
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
                    query: rhs,
                },
            probe_col,
            index_side: SourceExpr::DbTable(DbTable {
                table_id: index_table, ..
            }),
            index_col,
            ..
        }) = &query[0]
        else {
            panic!("unexpected operator {:#?}", query[0]);
        };

        assert_eq!(*table_id, rhs_id);
        assert_eq!(*index_table, lhs_id);
        assert_eq!(index_col, &1.into());
        assert_eq!(*probe_col, 0.into());

        assert_eq!(2, rhs.len());

        // The probe side of the join should be an index scan
        let table_id = assert_one_eq_index_scan(&rhs[0], col_list![0, 1], product![4u64, 2u64].into());

        assert_eq!(table_id, rhs_id);

        // Followed by a selection
        let Query::Select(ColumnOp::ColCmpVal {
            cmp: OpCmp::Eq,
            lhs: ColId(2),
            rhs: AlgebraicValue::U64(3),
        }) = rhs[1]
        else {
            panic!("unexpected operator {:#?}", rhs[0]);
        };
        Ok(())
    }

    #[test]
    fn compile_join_with_diff_col_names() -> ResultTest<()> {
        let db = TestDB::durable()?;
        db.create_table_for_test("A", &[("x", AlgebraicType::U64)], &[])?;
        db.create_table_for_test("B", &[("y", AlgebraicType::U64)], &[])?;
        assert!(compile_sql(&db, &begin_tx(&db), "select B.* from B join A on B.y = A.x").is_ok());
        Ok(())
    }

    #[test]
    fn compile_type_check() -> ResultTest<()> {
        let db = TestDB::durable()?;
        db.create_table_for_test("PlayerState", &[("entity_id", AlgebraicType::U64)], &[0.into()])?;
        db.create_table_for_test("EnemyState", &[("entity_id", AlgebraicType::I8)], &[0.into()])?;
        db.create_table_for_test("FriendState", &[("entity_id", AlgebraicType::U64)], &[0.into()])?;
        let sql = "SELECT * FROM PlayerState WHERE entity_id = '161853'";

        // Should fail with type mismatch for selections and joins.
        //
        // TODO: Type check other operations deferred for the new query engine.

        assert!(
            compile_sql(&db, &begin_tx(&db), sql).is_err(),
            // Err("SqlError: Type Mismatch: `PlayerState.entity_id: U64` != `String(\"161853\"): String`, executing: `SELECT * FROM PlayerState WHERE entity_id = '161853'`".into())
        );

        // Check we can still compile the query if we remove the type mismatch and have multiple logical operations.
        let sql = "SELECT * FROM PlayerState WHERE entity_id = 1 AND entity_id = 2 AND entity_id = 3 OR entity_id = 4 OR entity_id = 5";

        assert!(compile_sql(&db, &begin_tx(&db), sql).is_ok());

        // Now verify when we have a type mismatch in the middle of the logical operations.
        let sql = "SELECT * FROM PlayerState WHERE entity_id = 1 AND entity_id";

        assert!(
            compile_sql(&db, &begin_tx(&db), sql).is_err(),
            // Err("SqlError: Type Mismatch: `PlayerState.entity_id: U64 == U64(1): U64` and `PlayerState.entity_id: U64`, both sides must be an `Bool` expression, executing: `SELECT * FROM PlayerState WHERE entity_id = 1 AND entity_id`".into())
        );
        // Verify that all operands of `AND` must be `Bool`.
        let sql = "SELECT * FROM PlayerState WHERE entity_id AND entity_id";

        assert!(
            compile_sql(&db, &begin_tx(&db), sql).is_err(),
            // Err("SqlError: Type Mismatch: `PlayerState.entity_id: U64` and `PlayerState.entity_id: U64`, both sides must be an `Bool` expression, executing: `SELECT * FROM PlayerState WHERE entity_id AND entity_id`".into())
        );
        Ok(())
    }
}
