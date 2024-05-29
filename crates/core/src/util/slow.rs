use std::time::{Duration, Instant};

use crate::execution_context::{ExecutionContext, WorkloadType};

/// Default threshold for general queries in `ms`.
const THRESHOLD_QUERIES_MILLIS: u64 = 100;

/// Configuration threshold for detecting slow queries.
#[derive(Debug, Clone, Copy, Default)]
pub struct SlowQueryConfig {
    /// The threshold duration for incremental updates.
    pub(crate) incremental_updates: Option<Duration>,
    /// The threshold duration for subscriptions.
    pub(crate) subscriptions: Option<Duration>,
    /// The threshold duration for general queries.
    pub(crate) queries: Option<Duration>,
}

impl SlowQueryConfig {
    /// Creates a new `SlowQueryConfig` with [THRESHOLD_QUERIES_MILLIS] for `queries` and the rest set to [None].
    pub fn with_defaults() -> Self {
        Self {
            incremental_updates: None,
            subscriptions: None,
            queries: Some(Duration::from_millis(THRESHOLD_QUERIES_MILLIS)),
        }
    }

    /// Sets the threshold for incremental updates.
    pub fn with_incremental_updates(mut self, duration: Duration) -> Self {
        self.incremental_updates = Some(duration);
        self
    }
    /// Sets the threshold for subscriptions.
    pub fn with_subscriptions(mut self, duration: Duration) -> Self {
        self.subscriptions = Some(duration);
        self
    }
    /// Sets the threshold for general queries.
    pub fn with_queries(mut self, duration: Duration) -> Self {
        self.queries = Some(duration);
        self
    }
}

/// Records the execution time of some `sql`
/// and logs when the duration goes above a specific one.
pub struct SlowQueryLogger<'a> {
    /// The SQL statement of the query.
    sql: &'a str,
    /// The start time of the query execution.
    start: Option<Instant>,
    /// The threshold, if any, over which execution duration would result in logging.
    threshold: &'a Option<Duration>,
    /// The context the query is being run in.
    workload: WorkloadType,
}

impl<'a> SlowQueryLogger<'a> {
    pub fn new(sql: &'a str, threshold: &'a Option<Duration>, workload: WorkloadType) -> Self {
        Self {
            sql,
            start: threshold.map(|_| Instant::now()),
            threshold,
            workload,
        }
    }

    /// Creates a new [SlowQueryLogger] instance for general queries.
    pub fn query(ctx: &'a ExecutionContext, sql: &'a str) -> Self {
        Self::new(sql, &ctx.slow_query_config.queries, ctx.workload())
    }

    /// Creates a new [SlowQueryLogger] instance for subscriptions.
    pub fn subscription(ctx: &'a ExecutionContext, sql: &'a str) -> Self {
        Self::new(sql, &ctx.slow_query_config.subscriptions, ctx.workload())
    }

    /// Creates a new [SlowQueryLogger] instance for incremental updates.
    pub fn incremental_updates(ctx: &'a ExecutionContext, sql: &'a str) -> Self {
        Self::new(sql, &ctx.slow_query_config.incremental_updates, ctx.workload())
    }

    pub fn log_guard(self) -> impl Drop + 'a {
        scopeguard::guard(self, |logger| {
            logger.log();
        })
    }

    /// Log as `tracing::warn!` the query if it exceeds the threshold.
    pub fn log(&self) -> Option<Duration> {
        if let Some((start, threshold)) = self.start.zip(*self.threshold) {
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

    use crate::execution_context::ExecutionContext;
    use crate::sql::compiler::compile_sql;
    use crate::sql::execute::execute_sql;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::identity::AuthCtx;

    use crate::config::ReadConfigOption;
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::db::relational_db::RelationalDB;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_vm::relation::MemTable;

    fn run_query(db: &RelationalDB, sql: String) -> ResultTest<MemTable> {
        let tx = db.begin_tx();
        let q = compile_sql(db, &tx, &sql)?;
        Ok(execute_sql(db, &sql, q, AuthCtx::for_testing())?.pop().unwrap())
    }

    fn run_query_write(db: &RelationalDB, sql: String) -> ResultTest<()> {
        let tx = db.begin_tx();
        let q = compile_sql(db, &tx, &sql)?;
        drop(tx);

        execute_sql(db, &sql, q, AuthCtx::for_testing())?;

        Ok(())
    }

    #[test]
    fn test_slow_queries() -> ResultTest<()> {
        let db = TestDB::in_memory()?.db;

        let table_id =
            db.create_table_for_test("test", &[("x", AlgebraicType::I32), ("y", AlgebraicType::I32)], &[])?;

        let mut ctx = ExecutionContext::default();
        ctx.slow_query_config = ctx.slow_query_config.with_queries(Duration::from_millis(1));

        db.with_auto_commit(&ctx, |tx| -> ResultTest<_> {
            for i in 0..100_000 {
                db.insert(tx, table_id, product![i, i * 2])?;
            }
            Ok(())
        })?;
        let tx = db.begin_tx();

        let sql = "select * from test where x > 0";
        let q = compile_sql(&db, &tx, sql)?;

        let slow = SlowQueryLogger::query(&ctx, sql);

        let result = execute_sql(&db, sql, q, AuthCtx::for_testing())?;
        assert_eq!(result[0].data[0], product![1, 2]);
        assert!(slow.log().is_some());

        Ok(())
    }

    // Verify we can change the threshold at runtime
    #[test]
    fn test_runtime_config() -> ResultTest<()> {
        let db = TestDB::in_memory()?.db;

        let config = db.read_config();

        let check = |table: MemTable, x: Option<Duration>| {
            assert_eq!(
                table.data[0]
                    .field_as_sum(0, None)
                    .unwrap()
                    .value
                    .as_u128()
                    .map(|x| x.0),
                x.map(|x| x.as_millis())
            );
        };

        // Check we can read the default config
        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowQueryThreshold))?;
        check(result, config.slow_query.queries);
        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowSubscriptionsThreshold))?;
        check(result, config.slow_query.subscriptions);
        let result = run_query(
            &db,
            format!("SHOW {}", ReadConfigOption::SlowIncrementalUpdatesThreshold),
        )?;
        check(result, config.slow_query.incremental_updates);
        // Check we can write a new config
        run_query_write(&db, format!("SET {} TO 1", ReadConfigOption::SlowQueryThreshold))?;
        run_query_write(
            &db,
            format!("SET {} TO 1", ReadConfigOption::SlowSubscriptionsThreshold),
        )?;
        run_query_write(
            &db,
            format!("SET {} TO 1", ReadConfigOption::SlowIncrementalUpdatesThreshold),
        )?;

        let config = db.read_config();

        assert_eq!(config.slow_query.queries, Some(Duration::from_millis(1)));
        assert_eq!(config.slow_query.subscriptions, Some(Duration::from_millis(1)));
        assert_eq!(config.slow_query.incremental_updates, Some(Duration::from_millis(1)));

        // And the new config
        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowQueryThreshold))?;
        check(result, config.slow_query.queries);
        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowSubscriptionsThreshold))?;
        check(result, config.slow_query.subscriptions);
        let result = run_query(
            &db,
            format!("SHOW {}", ReadConfigOption::SlowIncrementalUpdatesThreshold),
        )?;
        check(result, config.slow_query.incremental_updates);

        // And disable the config
        run_query_write(&db, format!("SET {} TO 0", ReadConfigOption::SlowQueryThreshold))?;

        let config = db.read_config();

        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowQueryThreshold))?;
        check(result, config.slow_query.queries);
        Ok(())
    }
}
