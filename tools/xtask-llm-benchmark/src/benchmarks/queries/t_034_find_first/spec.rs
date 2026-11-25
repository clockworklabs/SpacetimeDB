use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            // Only one incomplete task, because the prompt does not define an iteration order.
            s.seed(
                "task",
                &["id", "title", "completed"],
                json!([[1, "done", true], [2, "open", false], [3, "also done", true]]),
            )?;
            s.call("find_first_incomplete", json!([]))?;
            s.owner_rows("first_incomplete", &["task_id", "title"], json!([[2, "open"]]))?;
            Ok(())
        }));
        scorers
    })
}
