use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let table = crate::eval::table_name("cleanup_job", s.lang);
            let query = format!("SELECT * FROM \"{table}\"");
            let rows = s.owner_sql(&query)?;
            anyhow::ensure!(rows.as_array().map(Vec::len) == Some(1), "expected one job, got {rows}");
            let id = rows[0][0].clone();
            s.call("cancel_cleanup", json!([id]))?;
            let rows = s.owner_sql(&query)?;
            anyhow::ensure!(
                rows.as_array().map(Vec::is_empty) == Some(true),
                "job not cancelled: {rows}"
            );
            Ok(())
        }));
        scorers
    })
}
