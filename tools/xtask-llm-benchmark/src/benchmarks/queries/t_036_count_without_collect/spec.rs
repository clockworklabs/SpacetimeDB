use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "user",
                &["id", "name", "active"],
                json!([[1, "a", true], [2, "b", false], [3, "c", true]]),
            )?;
            s.call("compute_user_counts", json!([]))?;
            s.owner_rows("user_stats", &["key", "count"], json!([["active", 2], ["total", 3]]))?;
            Ok(())
        }));
        scorers
    })
}
