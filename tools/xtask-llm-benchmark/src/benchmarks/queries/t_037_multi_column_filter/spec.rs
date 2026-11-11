use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "event_log",
                &["id", "category", "severity", "message"],
                json!([
                    [1, "db", 2, "match"],
                    [2, "db", 3, "wrong severity"],
                    [3, "net", 2, "wrong category"],
                    [4, "db", 2, "also match"]
                ]),
            )?;
            s.call("filter_events", json!(["db", 2]))?;
            s.owner_rows(
                "filtered_event",
                &["event_id", "message"],
                json!([[1, "match"], [4, "also match"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
