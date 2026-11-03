use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "announcement",
                &["id", "message", "active"],
                json!([[1, "shown", true], [2, "hidden", false], [3, "also shown", true]]),
            )?;
            s.rows(
                "active_announcements",
                &["id", "message", "active"],
                json!([[1, "shown", true], [3, "also shown", true]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
