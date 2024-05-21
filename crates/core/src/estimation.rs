use crate::db::relational_db::Tx;
use spacetimedb_lib::relation::DbTable;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_vm::expr::{Query, QueryExpr, SourceExpr};

/// The estimated number of rows that a query plan will return.
pub fn num_rows(tx: &Tx, expr: &QueryExpr) -> u64 {
    row_est(tx, &expr.source, &expr.query)
}

/// The estimated number of rows that a query sub-plan will return.
fn row_est(tx: &Tx, src: &SourceExpr, ops: &[Query]) -> u64 {
    ops.last().map_or_else(
        || match src {
            SourceExpr::DbTable(DbTable { table_id, .. }) => tx.get_row_count(*table_id).unwrap_or(0),
            _ => 0,
        },
        |op| {
            let input = &ops[0..ops.len() - 1];
            match op {
                Query::Project(_, _) => {
                    // We assume projections select 100% of their input rows.
                    row_est(tx, src, input)
                }
                Query::Select(_) => {
                    // How selective is an arbitrary predicate?
                    // If it is not sargable,
                    // meaning it cannot be satisfied using an index,
                    // we assume the worst-case scenario,
                    // that it will select all of its input rows.
                    // That is we set the selectivity = 1.
                    row_est(tx, src, input)
                }
                Query::IndexScan(scan) if scan.is_range() => {
                    // We do the same for sargable range conditions.
                    row_est(tx, src, input)
                }
                Query::IndexScan(scan) => {
                    // How selective is an index lookup?
                    // We assume a uniform distribution of keys,
                    // which implies a selectivity = 1 / NDV,
                    // where NDV stands for Number of Distinct Values.
                    index_row_est(tx, scan.table.table_id, &scan.columns)
                }
                Query::IndexJoin(join) => {
                    // How selective is an index join?
                    // We have an estimate for the number of probe side rows,
                    // We have an estimate for the number of rows each index probe will return.
                    // Multiplying both estimates together will give us our expectation.
                    let table_id = if join.return_index_rows {
                        src.table_id().unwrap()
                    } else {
                        join.probe_side.source.table_id().unwrap()
                    };
                    row_est(tx, &join.probe_side.source, &join.probe_side.query)
                        * index_row_est(tx, table_id, &join.index_col.into())
                }
                Query::JoinInner(join) => {
                    // Since inner join is our most expensive operation,
                    // we maximally overestimate its output cardinality,
                    // as though each row from the left joins with each row from the right.
                    row_est(tx, src, input) * row_est(tx, &join.rhs.source, &join.rhs.query)
                }
            }
        },
    )
}

/// The estimated number of rows that an index probe will return.
/// Note this method is not applicable to range scans.
fn index_row_est(tx: &Tx, table_id: TableId, cols: &ColList) -> u64 {
    tx.ndv(table_id, cols)
        .map_or(0, |ndv| tx.get_row_count(table_id).unwrap_or(0) / ndv)
}

#[cfg(test)]
mod tests {
    use crate::{
        db::relational_db::tests_utils::TestDB, error::DBError, estimation::num_rows,
        execution_context::ExecutionContext, sql::compiler::compile_sql,
    };
    use spacetimedb_lib::AlgebraicType;
    use spacetimedb_sats::product;
    use spacetimedb_vm::expr::CrudExpr;

    #[test]
    /// Cardinality estimation for an index lookup depends only on
    /// (1) the total number of rows,
    /// (2) the number of distinct values.
    fn cardinality_estimation_index_lookup() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let table_id = db
            .create_table_for_test(
                "T",
                &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)],
                &[(0.into(), "a")],
            )
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, table_id, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select * from T where a = 0";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(2, num_rows(&tx, &expr));
    }

    #[test]
    /// We estimate an index range to return all input rows.
    fn cardinality_estimation_index_range() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let table_id = db
            .create_table_for_test(
                "T",
                &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)],
                &[(0.into(), "a")],
            )
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, table_id, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select * from T where a > 0 and a < 2";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(10, num_rows(&tx, &expr));
    }

    #[test]
    /// We estimate a selection on a non-indexed column to return all input rows.
    fn select_cardinality_estimation() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let table_id = db
            .create_table_for_test(
                "T",
                &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)],
                &[(0.into(), "a")],
            )
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, table_id, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select * from T where b = 0";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(10, num_rows(&tx, &expr));
    }

    #[test]
    /// We estimate a projection to return all input rows.
    fn project_cardinality_estimation() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let table_id = db
            .create_table_for_test(
                "T",
                &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)],
                &[(0.into(), "a")],
            )
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, table_id, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select a from T";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(10, num_rows(&tx, &expr));
    }

    #[test]
    /// We estimate an inner join to return the product of its input sizes.
    fn cardinality_estimation_inner_join() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let lhs = db
            .create_table_for_test("T", &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)], &[])
            .expect("Failed to create table");

        let rhs = db
            .create_table_for_test("S", &[("a", AlgebraicType::U8), ("c", AlgebraicType::U8)], &[])
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, lhs, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..2 {
                db.insert(tx, rhs, product!(i, i)).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select T.* from T join S on T.a = S.a where S.c = 0";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(20, num_rows(&tx, &expr));
    }

    #[test]
    /// An index join estimates its output cardinality in the same way.
    /// As the product of its estimated input cardinalities.
    fn cardinality_estimation_index_join() {
        let db = TestDB::in_memory().expect("failed to make test db");

        let lhs = db
            .create_table_for_test(
                "T",
                &[("a", AlgebraicType::U8), ("b", AlgebraicType::U8)],
                &[(0.into(), "a")],
            )
            .expect("Failed to create table");

        let rhs = db
            .create_table_for_test(
                "S",
                &[("a", AlgebraicType::U8), ("c", AlgebraicType::U8)],
                &[(0.into(), "a"), (1.into(), "c")],
            )
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..10 {
                db.insert(tx, lhs, product!(i % 5, i))
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0u8..2 {
                db.insert(tx, rhs, product!(i, i)).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");

        let tx = db.begin_tx();
        let sql = "select T.* from T join S on T.a = S.a where S.c = 0";
        let exp = compile_sql(&db, &tx, sql).expect("Failed to compile sql").remove(0);

        let CrudExpr::Query(expr) = exp else {
            panic!("unexpected result from compilation: {:#?}", exp);
        };

        assert_eq!(2, num_rows(&tx, &expr));
    }
}
