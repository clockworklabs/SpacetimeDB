use crate::{
    db::relational_db::{RelationalDB, Tx},
    error::DBError,
};
use spacetimedb_datastore::locking_tx_datastore::{state_view::StateView as _, NumDistinctValues};
use spacetimedb_lib::{identity::AuthCtx, query::Delta};
use spacetimedb_physical_plan::plan::{HashJoin, IxJoin, IxScan, PhysicalPlan, Sarg, TableScan};
use spacetimedb_primitives::{ColList, TableId};

/// If the caller is not allowed to exceed the row limit,
/// reject the request if the estimated cardinality exceeds the limit.
pub fn check_row_limit<Query>(
    queries: &[Query],
    db: &RelationalDB,
    tx: &Tx,
    row_est: impl Fn(&Query, &Tx) -> u64,
    auth: &AuthCtx,
) -> Result<(), DBError> {
    if !auth.exceed_row_limit()
        && let Some(limit) = db.row_limit(tx)?
    {
        let mut estimate: u64 = 0;
        for query in queries {
            estimate = estimate.saturating_add(row_est(query, tx));
        }
        if estimate > limit {
            return Err(DBError::Other(anyhow::anyhow!(
                "Estimated cardinality ({estimate} rows) exceeds limit ({limit} rows)"
            )));
        }
    }
    Ok(())
}

/// Use cardinality estimates to predict the total number of rows scanned by a query.
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

/// Estimate the cardinality of a physical plan.
pub fn row_estimate(tx: &Tx, plan: &PhysicalPlan) -> u64 {
    match plan {
        PhysicalPlan::TableScan(TableScan { limit: Some(n), .. }, _)
        | PhysicalPlan::IxScan(IxScan { limit: Some(n), .. }, _) => *n,
        PhysicalPlan::TableScan(
            TableScan {
                schema,
                limit: None,
                delta: None,
            },
            _,
        ) => tx.table_row_count(schema.table_id).unwrap_or_default(),
        PhysicalPlan::TableScan(
            TableScan {
                limit: None,
                delta: Some(Delta::Inserts | Delta::Deletes),
                ..
            },
            _,
        ) => 0,
        PhysicalPlan::IxScan(
            ix @ IxScan {
                arg: Sarg::Eq(last_col, _),
                ..
            },
            _,
        ) => {
            let mut cols: ColList = ix.prefix.iter().map(|(c, _)| *c).collect();
            cols.push(*last_col);
            index_row_est(tx, ix.schema.table_id, &cols)
        }
        PhysicalPlan::IxScan(IxScan { schema, .. }, _) => tx.table_row_count(schema.table_id).unwrap_or_default(),
        PhysicalPlan::Filter(input, _) => row_estimate(tx, input),
        PhysicalPlan::NLJoin(lhs, rhs) => row_estimate(tx, lhs).saturating_mul(row_estimate(tx, rhs)),
        PhysicalPlan::IxJoin(IxJoin { lhs, unique: true, .. }, _)
        | PhysicalPlan::HashJoin(HashJoin { lhs, unique: true, .. }, _) => row_estimate(tx, lhs),
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

/// The estimated number of rows that an index probe will return.
fn index_row_est(tx: &Tx, table_id: TableId, cols: &ColList) -> u64 {
    let table_rc = || tx.table_row_count(table_id).unwrap_or_default();
    match tx.num_distinct_values(table_id, cols) {
        NumDistinctValues::NonZero(ndv) => table_rc() / ndv,
        NumDistinctValues::Zero => 0,
        NumDistinctValues::Error => table_rc(),
    }
}

#[cfg(test)]
mod tests {
    use super::{estimate_rows_scanned, row_estimate};
    use crate::db::relational_db::tests_utils::{begin_tx, insert, with_auto_commit};
    use crate::db::relational_db::{tests_utils::TestDB, RelationalDB};
    use crate::error::DBError;
    use crate::sql::ast::SchemaViewer;
    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType};
    use spacetimedb_query::compile_subscription;
    use spacetimedb_sats::product;

    fn in_mem_db() -> TestDB {
        TestDB::in_memory().expect("failed to make test db")
    }

    fn estimate_for(db: &RelationalDB, sql: &str) -> u64 {
        let auth = AuthCtx::for_testing();
        let tx = begin_tx(db);
        let tx = SchemaViewer::new(&tx, &auth);

        compile_subscription(sql, &tx, &auth)
            .map(|(plans, ..)| plans)
            .expect("failed to compile sql query")
            .into_iter()
            .map(|plan| plan.optimize(&auth).expect("failed to optimize sql query"))
            .map(|plan| row_estimate(&tx, &plan))
            .sum()
    }

    fn scanned_for(db: &RelationalDB, sql: &str) -> u64 {
        let auth = AuthCtx::for_testing();
        let tx = begin_tx(db);
        let tx = SchemaViewer::new(&tx, &auth);

        compile_subscription(sql, &tx, &auth)
            .map(|(plans, ..)| plans)
            .expect("failed to compile sql query")
            .into_iter()
            .map(|plan| plan.optimize(&auth).expect("failed to optimize sql query"))
            .map(|plan| estimate_rows_scanned(&tx, plan.physical_plan()))
            .sum()
    }

    fn create_table_t(db: &RelationalDB, indexed: bool) {
        let indexes = &[0.into()];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        let table_id = db
            .create_table_for_test("T", &["a", "b"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");

        with_auto_commit(db, |tx| -> Result<(), DBError> {
            for i in 0u64..10u64 {
                insert(db, tx, table_id, &product![i % 5, i]).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");
    }

    #[test]
    fn cardinality_estimation_index_lookup() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert_eq!(2, estimate_for(&db, "select * from T where a = 0"));
    }

    #[test]
    fn scanned_rows_respect_filters() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert!(scanned_for(&db, "select * from T where a = 0") <= scanned_for(&db, "select * from T"));
    }
}
