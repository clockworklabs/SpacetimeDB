use crate::db::{datastore::locking_tx_datastore::state_view::StateView as _, relational_db::Tx};
use spacetimedb_lib::query::Delta;
use spacetimedb_physical_plan::plan::{HashJoin, IxJoin, IxScan, PhysicalPlan, Sarg, TableScan};
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_vm::expr::{Query, QueryExpr, SourceExpr};

/// The estimated number of rows that a query plan will return.
pub fn num_rows(tx: &Tx, expr: &QueryExpr) -> u64 {
    row_est(tx, &expr.source, &expr.query)
}

/// Use cardinality estimates to predict the total number of rows scanned by a query
pub fn estimate_rows_scanned(tx: &Tx, plan: &PhysicalPlan) -> u64 {
    match plan {
        PhysicalPlan::TableScan(..) | PhysicalPlan::IxScan(..) => row_estimate(tx, plan),
        PhysicalPlan::Filter(input, _) => estimate_rows_scanned(tx, input).saturating_add(row_estimate(tx, input)),
        PhysicalPlan::NLJoin(lhs, rhs) => estimate_rows_scanned(tx, lhs)
            .saturating_add(estimate_rows_scanned(tx, rhs))
            .saturating_add(row_estimate(tx, lhs).saturating_mul(row_estimate(tx, rhs))),
        PhysicalPlan::IxJoin(IxJoin { lhs, unique: true, .. }, _) => {
            estimate_rows_scanned(tx, lhs).saturating_add(row_estimate(tx, lhs))
        }
        PhysicalPlan::IxJoin(
            IxJoin {
                lhs, rhs, rhs_field, ..
            },
            _,
        ) => estimate_rows_scanned(tx, lhs).saturating_add(row_estimate(tx, lhs).saturating_mul(index_row_est(
            tx,
            rhs.table_id,
            &ColList::from(*rhs_field),
        ))),
        PhysicalPlan::HashJoin(
            HashJoin {
                lhs, rhs, unique: true, ..
            },
            _,
        ) => estimate_rows_scanned(tx, lhs)
            .saturating_add(estimate_rows_scanned(tx, rhs))
            .saturating_add(row_estimate(tx, lhs)),
        PhysicalPlan::HashJoin(HashJoin { lhs, rhs, .. }, _) => estimate_rows_scanned(tx, lhs)
            .saturating_add(estimate_rows_scanned(tx, rhs))
            .saturating_add(row_estimate(tx, lhs).saturating_mul(row_estimate(tx, rhs))),
    }
}

/// Estimate the cardinality of a physical plan
pub fn row_estimate(tx: &Tx, plan: &PhysicalPlan) -> u64 {
    match plan {
        // Use a row limit as the estimate if present
        PhysicalPlan::TableScan(TableScan { limit: Some(n), .. }, _)
        | PhysicalPlan::IxScan(IxScan { limit: Some(n), .. }, _) => *n,
        // Table scans return the number of rows in the table
        PhysicalPlan::TableScan(
            TableScan {
                schema,
                limit: None,
                delta: None,
            },
            _,
        ) => tx.table_row_count(schema.table_id).unwrap_or_default(),
        // We don't estimate the cardinality of delta scans currently
        PhysicalPlan::TableScan(
            TableScan {
                limit: None,
                delta: Some(Delta::Inserts | Delta::Deletes),
                ..
            },
            _,
        ) => 0,
        // The selectivity of a single column index scan is 1 / NDV,
        // where NDV is the Number of Distinct Values of a column.
        // Note, this assumes a uniform distribution of column values.
        PhysicalPlan::IxScan(
            ix @ IxScan {
                arg: Sarg::Eq(col_id, _),
                ..
            },
            _,
        ) if ix.prefix.is_empty() => index_row_est(tx, ix.schema.table_id, &ColList::from(*col_id)),
        // For all other index scans we assume a worst-case scenario.
        PhysicalPlan::IxScan(IxScan { schema, .. }, _) => tx.table_row_count(schema.table_id).unwrap_or_default(),
        // Same for filters
        PhysicalPlan::Filter(input, _) => row_estimate(tx, input),
        // Nested loop joins are cross joins
        PhysicalPlan::NLJoin(lhs, rhs) => row_estimate(tx, lhs).saturating_mul(row_estimate(tx, rhs)),
        // Unique joins return a maximal estimation.
        // We assume every lhs row has a matching rhs row.
        PhysicalPlan::IxJoin(IxJoin { lhs, unique: true, .. }, _)
        | PhysicalPlan::HashJoin(HashJoin { lhs, unique: true, .. }, _) => row_estimate(tx, lhs),
        // Otherwise we estimate the rows returned from the rhs
        PhysicalPlan::IxJoin(
            IxJoin {
                lhs, rhs, rhs_field, ..
            },
            _,
        ) => row_estimate(tx, lhs).saturating_mul(index_row_est(tx, rhs.table_id, &ColList::from(*rhs_field))),
        PhysicalPlan::HashJoin(HashJoin { lhs, rhs, .. }, _) => {
            row_estimate(tx, lhs).saturating_mul(row_estimate(tx, rhs))
        }
    }
}

/// The estimated number of rows that a query sub-plan will return.
fn row_est(tx: &Tx, src: &SourceExpr, ops: &[Query]) -> u64 {
    match ops {
        // The base case is the table row count.
        [] => src.table_id().and_then(|id| tx.table_row_count(id)).unwrap_or(0),
        // Walk in reverse from the end (`op`) to the beginning.
        [input @ .., op] => match op {
            // How selective is an index lookup?
            // We assume a uniform distribution of keys,
            // which implies a selectivity = 1 / NDV,
            // where NDV stands for Number of Distinct Values.
            Query::IndexScan(scan) if scan.is_point() => {
                index_row_est(tx, scan.table.table_id, &scan.columns)
            }
            // We assume projections select 100% of their input rows.
            Query::Project(..)
            // How selective is an arbitrary predicate?
            // If it is not sargable,
            // meaning it cannot be satisfied using an index,
            // we assume the worst-case scenario,
            // that it will select all of its input rows.
            // That is we set the selectivity = 1.
            | Query::Select(_)
            // We do the same for sargable range conditions.
            | Query::IndexScan(_) => {
                row_est(tx, src, input)
            }
            // How selective is an index join?
            // We have an estimate for the number of probe side rows,
            // We have an estimate for the number of rows each index probe will return.
            // Multiplying both estimates together will give us our expectation.
            Query::IndexJoin(join) => {
                row_est(tx, &join.probe_side.source, &join.probe_side.query)
                    .saturating_mul(
                        index_row_est(tx, src.table_id().unwrap(), &join.index_col.into())
                    )
            }
            // Since inner join is our most expensive operation,
            // we maximally overestimate its output cardinality,
            // as though each row from the left joins with each row from the right.
            Query::JoinInner(join) => {
                row_est(tx, src, input)
                    .saturating_mul(
                        row_est(tx, &join.rhs.source, &join.rhs.query)
                    )
            }
        },
    }
}

/// The estimated number of rows that an index probe will return.
/// Note this method is not applicable to range scans.
fn index_row_est(tx: &Tx, table_id: TableId, cols: &ColList) -> u64 {
    tx.num_distinct_values(table_id, cols)
        .map_or(0, |ndv| tx.table_row_count(table_id).unwrap_or(0) / ndv)
}

#[cfg(test)]
mod tests {
    use crate::db::relational_db::tests_utils::{begin_tx, insert, with_auto_commit};
    use crate::sql::ast::SchemaViewer;
    use crate::{
        db::relational_db::{tests_utils::TestDB, RelationalDB},
        error::DBError,
        estimation::num_rows,
        sql::compiler::compile_sql,
    };
    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType};
    use spacetimedb_query::compile_subscription;
    use spacetimedb_sats::product;
    use spacetimedb_vm::expr::CrudExpr;

    use super::row_estimate;

    fn in_mem_db() -> TestDB {
        TestDB::in_memory().expect("failed to make test db")
    }

    fn num_rows_for(db: &RelationalDB, sql: &str) -> u64 {
        let tx = begin_tx(db);
        match &*compile_sql(db, &AuthCtx::for_testing(), &tx, sql).expect("Failed to compile sql") {
            [CrudExpr::Query(expr)] => num_rows(&tx, expr),
            exprs => panic!("unexpected result from compilation: {:#?}", exprs),
        }
    }

    /// Using the new query plan
    fn new_row_estimate(db: &RelationalDB, sql: &str) -> u64 {
        let auth = AuthCtx::for_testing();
        let tx = begin_tx(db);
        let tx = SchemaViewer::new(&tx, &auth);

        compile_subscription(sql, &tx, &auth)
            .map(|(plans, ..)| plans)
            .expect("failed to compile sql query")
            .into_iter()
            .map(|plan| plan.optimize().expect("failed to optimize sql query"))
            .map(|plan| row_estimate(&tx, &plan))
            .sum()
    }

    const NUM_T_ROWS: u64 = 10;
    const NDV_T: u64 = 5;
    const NUM_S_ROWS: u64 = 2;
    const NDV_S: u64 = 2;

    fn create_table_t(db: &RelationalDB, indexed: bool) {
        let indexes = &[0.into()];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        let table_id = db
            .create_table_for_test("T", &["a", "b"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");

        with_auto_commit(db, |tx| -> Result<(), DBError> {
            for i in 0..NUM_T_ROWS {
                insert(db, tx, table_id, &product![i % NDV_T, i]).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");
    }

    fn create_table_s(db: &RelationalDB, indexed: bool) {
        let indexes = &[0.into(), 1.into()];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        let rhs = db
            .create_table_for_test("S", &["a", "c"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");

        with_auto_commit(db, |tx| -> Result<(), DBError> {
            for i in 0..NUM_S_ROWS {
                insert(db, tx, rhs, &product![i, i]).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");
    }

    fn create_empty_table_r(db: &RelationalDB, indexed: bool) {
        let indexes = &[0.into()];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        db.create_table_for_test("R", &["a", "b"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");
    }

    /// Cardinality estimation for an index lookup depends only on
    /// (1) the total number of rows,
    /// (2) the number of distinct values.
    #[test]
    fn cardinality_estimation_index_lookup() {
        let db = in_mem_db();
        create_table_t(&db, true);
        let sql = "select * from T where a = 0";
        let est = NUM_T_ROWS / NDV_T;
        assert_eq!(est, num_rows_for(&db, sql));
        assert_eq!(est, new_row_estimate(&db, sql));
    }

    #[test]
    fn cardinality_estimation_0_ndv() {
        let db = in_mem_db();
        create_empty_table_r(&db, true);
        let sql = "select * from R where a = 0";
        assert_eq!(0, num_rows_for(&db, sql));
        assert_eq!(0, new_row_estimate(&db, sql));
    }

    /// We estimate an index range to return all input rows.
    #[test]
    fn cardinality_estimation_index_range() {
        let db = in_mem_db();
        create_table_t(&db, true);
        let sql = "select * from T where a > 0 and a < 2";
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, sql));
        assert_eq!(NUM_T_ROWS, new_row_estimate(&db, sql));
    }

    /// We estimate a selection on a non-indexed column to return all input rows.
    #[test]
    fn select_cardinality_estimation() {
        let db = in_mem_db();
        create_table_t(&db, true);
        let sql = "select * from T where b = 0";
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, sql));
        assert_eq!(NUM_T_ROWS, new_row_estimate(&db, sql));
    }

    /// We estimate a projection to return all input rows.
    #[test]
    fn project_cardinality_estimation() {
        let db = in_mem_db();
        create_table_t(&db, true);
        let sql = "select a from T";
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, sql));
    }

    /// We estimate an inner join to return the product of its input sizes.
    #[test]
    fn cardinality_estimation_inner_join() {
        let db = in_mem_db();
        create_table_t(&db, false);
        create_table_s(&db, false);
        let sql = "select T.* from T join S on T.a = S.a where S.c = 0";
        let est = NUM_T_ROWS * NUM_S_ROWS;
        assert_eq!(est, num_rows_for(&db, sql));
        assert_eq!(est, new_row_estimate(&db, sql));
    }

    /// An index join estimates its output cardinality in the same way.
    /// As the product of its estimated input cardinalities.
    #[test]
    fn cardinality_estimation_index_join() {
        let db = in_mem_db();
        create_table_t(&db, true);
        create_table_s(&db, true);
        let sql = "select T.* from T join S on T.a = S.a where S.c = 0";
        let est = NUM_T_ROWS / NDV_T * NUM_S_ROWS / NDV_S;
        assert_eq!(est, num_rows_for(&db, sql));
        assert_eq!(est, new_row_estimate(&db, sql));
    }
}
