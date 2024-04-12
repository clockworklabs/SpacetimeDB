use std::time::{Duration, Instant};

/// Default threshold for general queries in `ms`.
const THRESHOLD_QUERIES_MILLIS: u64 = 100;

/// Configuration threshold for detecting slow queries.
#[derive(Debug, Clone, Copy)]
pub struct SlowQueryConfig {
    /// The threshold duration for incremental updates.
    pub(crate) incremental_updates: Option<Duration>,
    /// The threshold duration for subscriptions.
    pub(crate) subscriptions: Option<Duration>,
    /// The threshold duration for general queries.
    pub(crate) queries: Option<Duration>,
}

impl SlowQueryConfig {
    /// Creates a new `SlowQueryConfig` with all the threshold set to [None].
    pub fn new() -> Self {
        Self {
            incremental_updates: None,
            subscriptions: None,
            queries: None,
        }
    }

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

    pub fn for_queries(self, sql: &str) -> SlowQuery {
        SlowQuery::query(self, sql)
    }
    pub fn for_subscriptions(self, sql: &str) -> SlowQuery {
        SlowQuery::query(self, sql)
    }
    pub fn for_incremental_updates(self, sql: &str) -> SlowQuery {
        SlowQuery::incremental_updates(self, sql)
    }
}

impl Default for SlowQueryConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents `threshold` for [SlowQuery].
pub enum Threshold {
    IncrementalUpdates(Option<Duration>),
    Subscriptions(Option<Duration>),
    Queries(Option<Duration>),
}

/// Start the recording of a `sql` with a specific [Threshold].
pub struct SlowQuery<'a> {
    /// The SQL statement of the query.
    sql: &'a str,
    /// The start time of the query execution.
    start: Instant,
    /// Which [Threshold] to use.
    threshold: Threshold,
}

impl<'a> SlowQuery<'a> {
    pub fn new(sql: &'a str, threshold: Threshold) -> Self {
        Self {
            sql,
            start: Instant::now(),
            threshold,
        }
    }

    /// Creates a new [SlowQuery] instance for general queries.
    pub fn query(config: SlowQueryConfig, sql: &'a str) -> Self {
        Self::new(sql, Threshold::Queries(config.queries))
    }

    /// Creates a new [SlowQuery] instance for subscriptions.
    pub fn subscription(config: SlowQueryConfig, sql: &'a str) -> Self {
        Self::new(sql, Threshold::Subscriptions(config.subscriptions))
    }

    /// Creates a new [SlowQuery] instance for incremental updates.
    pub fn incremental_updates(config: SlowQueryConfig, sql: &'a str) -> Self {
        Self::new(sql, Threshold::IncrementalUpdates(config.queries))
    }

    /// Log as `tracing::warn!` the query if it exceeds the threshold.
    pub fn log(&self) -> Option<Duration> {
        let (kind, dur) = match self.threshold {
            Threshold::IncrementalUpdates(dur) => ("IncrementalUpdates", dur),
            Threshold::Subscriptions(dur) => ("Subscriptions", dur),
            Threshold::Queries(dur) => ("Queries", dur),
        };
        if let Some(dur) = dur {
            let elapsed = self.start.elapsed();
            if elapsed > dur {
                tracing::warn!(kind = kind, threshold = ?dur, elapsed = ?elapsed, sql = self.sql, "SLOW QUERY");
                return Some(elapsed);
            }
        };
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

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> ResultTest<_> {
            for i in 0..100_000 {
                db.insert(tx, table_id, product![i, i * 2])?;
            }
            Ok(())
        })?;
        let tx = db.begin_tx();

        let sql = "select * from test where x > 0";
        let q = compile_sql(&db, &tx, sql)?;

        let slow = SlowQueryConfig::default()
            .with_queries(Duration::from_millis(1))
            .for_queries(sql);

        let result = execute_sql(&db, sql, q, AuthCtx::for_testing())?;
        assert_eq!(result[0].data[0], product![1, 2]);
        assert!(slow.log().is_some());

        Ok(())
    }

    // Verify we can change the threshold at runtime
    #[test]
    fn test_runtime_config() -> ResultTest<()> {
        let db = TestDB::in_memory()?.db;

        let config = *db.config.read();

        let check = |table: MemTable, x: Option<Duration>| {
            assert_eq!(
                table.data[0].field_as_sum(0, None).unwrap().value.as_u128().copied(),
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

        let config = *db.config.read();

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

        let config = *db.config.read();

        let result = run_query(&db, format!("SHOW {}", ReadConfigOption::SlowQueryThreshold))?;
        check(result, config.slow_query.queries);
        Ok(())
    }
}
