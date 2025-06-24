use std::time::{Duration, Instant};

use crate::execution_context::WorkloadType;

/// Records the execution time of some `sql`
/// and logs when the duration goes above a specific one.
pub struct SlowQueryLogger<'a> {
    /// The SQL statement of the query.
    sql: &'a str,
    /// The start time of the query execution.
    start: Option<Instant>,
    /// The threshold, if any, over which execution duration would result in logging.
    threshold: Option<Duration>,
    /// The context the query is being run in.
    workload: WorkloadType,
}

impl<'a> SlowQueryLogger<'a> {
    pub fn new(sql: &'a str, threshold: Option<Duration>, workload: WorkloadType) -> Self {
        Self {
            sql,
            start: threshold.map(|_| Instant::now()),
            threshold,
            workload,
        }
    }

    pub fn log_guard(self) -> impl Drop + 'a {
        scopeguard::guard(self, |logger| {
            logger.log();
        })
    }

    /// Log as `tracing::warn!` the query if it exceeds the threshold.
    pub fn log(&self) -> Option<Duration> {
        if let Some((start, threshold)) = self.start.zip(self.threshold) {
            let elapsed = start.elapsed();
            if elapsed > threshold {
                tracing::warn!(workload = %self.workload, ?threshold, ?elapsed, sql = ?self.sql, "SLOW QUERY");
                return Some(elapsed);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::datastore::system_tables::ST_VARNAME_SLOW_QRY;
    use crate::db::datastore::system_tables::{StVarName, ST_VARNAME_SLOW_INC, ST_VARNAME_SLOW_SUB};
    use crate::sql::compiler::compile_sql;
    use crate::sql::execute::tests::execute_for_testing;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::st_var::StVarValue;
    use spacetimedb_lib::ProductValue;

    use crate::db::relational_db::tests_utils::{begin_tx, insert, with_auto_commit, TestDB};
    use crate::db::relational_db::RelationalDB;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::relation::MemTable;

    fn run_query(db: &RelationalDB, sql: String) -> ResultTest<MemTable> {
        let tx = begin_tx(db);
        let q = compile_sql(db, &AuthCtx::for_testing(), &tx, &sql)?;
        Ok(execute_for_testing(db, &sql, q)?.pop().unwrap())
    }

    fn run_query_write(db: &RelationalDB, sql: String) -> ResultTest<()> {
        let tx = begin_tx(db);
        let q = compile_sql(db, &AuthCtx::for_testing(), &tx, &sql)?;
        drop(tx);

        execute_for_testing(db, &sql, q)?;
        Ok(())
    }

    #[test]
    fn test_slow_queries() -> ResultTest<()> {
        let db = TestDB::in_memory()?.db;

        let table_id =
            db.create_table_for_test("test", &[("x", AlgebraicType::I32), ("y", AlgebraicType::I32)], &[])?;

        with_auto_commit(&db, |tx| -> ResultTest<_> {
            for i in 0..100_000 {
                insert(&db, tx, table_id, &product![i, i * 2])?;
            }
            Ok(())
        })?;
        let tx = begin_tx(&db);

        let sql = "select * from test where x > 0";
        let q = compile_sql(&db, &AuthCtx::for_testing(), &tx, sql)?;

        let slow = SlowQueryLogger::new(sql, Some(Duration::from_millis(1)), tx.ctx.workload());

        let result = execute_for_testing(&db, sql, q)?;
        assert_eq!(result[0].data[0], product![1, 2]);
        assert!(slow.log().is_some());

        Ok(())
    }

    // Verify we can change the threshold at runtime
    #[test]
    fn test_runtime_config() -> ResultTest<()> {
        let db = TestDB::in_memory()?.db;

        fn fetch_row(table: MemTable) -> Option<ProductValue> {
            table.data.into_iter().next()
        }

        // Check we can read the default config
        let row1 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_QRY))?);
        let row2 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_SUB))?);
        let row3 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_INC))?);

        assert_eq!(row1, None);
        assert_eq!(row2, None);
        assert_eq!(row3, None);

        // Check we can write a new config
        run_query_write(&db, format!("SET {} TO 1", ST_VARNAME_SLOW_QRY))?;
        run_query_write(&db, format!("SET {} TO 1", ST_VARNAME_SLOW_SUB))?;
        run_query_write(&db, format!("SET {} TO 1", ST_VARNAME_SLOW_INC))?;

        let row1 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_QRY))?);
        let row2 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_SUB))?);
        let row3 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_INC))?);

        assert_eq!(row1, Some(product!(StVarName::SlowQryThreshold, StVarValue::U64(1))));
        assert_eq!(row2, Some(product!(StVarName::SlowSubThreshold, StVarValue::U64(1))));
        assert_eq!(row3, Some(product!(StVarName::SlowIncThreshold, StVarValue::U64(1))));

        // And disable the config
        run_query_write(
            &db,
            format!("DELETE FROM st_var WHERE name = '{}'", ST_VARNAME_SLOW_QRY),
        )?;
        run_query_write(
            &db,
            format!("DELETE FROM st_var WHERE name = '{}'", ST_VARNAME_SLOW_SUB),
        )?;
        run_query_write(
            &db,
            format!("DELETE FROM st_var WHERE name = '{}'", ST_VARNAME_SLOW_INC),
        )?;

        let row1 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_QRY))?);
        let row2 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_SUB))?);
        let row3 = fetch_row(run_query(&db, format!("SHOW {}", ST_VARNAME_SLOW_INC))?);

        assert_eq!(row1, None);
        assert_eq!(row2, None);
        assert_eq!(row3, None);
        Ok(())
    }
}
