use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("create_job", json!([11]))?;
            s.reject("complete", json!([11, "", 0, "bad"]), "stale lease")?;
            s.reject("claim", json!([11, "worker", 0]), "invalid lease")?;
            s.call("claim", json!([11, "old", 1]))?;
            // Short lease, then a generous new lease. Only the expired boundary uses a delay.
            std::thread::sleep(std::time::Duration::from_millis(30));
            s.reject("complete", json!([11, "old", 1, "expired"]), "stale lease")?;
            s.call("claim", json!([11, "new", 60000]))?;
            s.reject("claim", json!([11, "third", 60000]), "lease busy")?;
            s.reject("complete", json!([11, "old", 1, "stale"]), "stale lease")?;
            s.reject("complete", json!([11, "new", 1, "wrong-token"]), "stale lease")?;
            s.rows(
                "leased_job",
                &["id", "worker", "generation", "done", "result"],
                json!([[11, "new", 2, false, ""]]),
            )?;
            s.call("complete", json!([11, "new", 2, "accepted"]))?;
            s.call("complete", json!([11, "new", 2, "accepted"]))?;
            s.reject("complete", json!([11, "new", 2, "changed"]), "already done")?;
            s.reject("claim", json!([11, "third", 60000]), "already done")?;
            s.rows(
                "leased_job",
                &["id", "worker", "generation", "done", "result"],
                json!([[11, "new", 2, true, "accepted"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
