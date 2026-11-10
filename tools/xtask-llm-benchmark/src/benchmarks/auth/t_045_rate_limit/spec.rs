use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (first, first_id) = s.new_user()?;
            let (second, second_id) = s.new_user()?;
            s.expect_call_as(&first, "limited_action", json!(["one"]), None)?;
            s.expect_rejected_as(&first, "limited_action", json!(["blocked"]), "rate limited")?;
            s.expect_call_as(&second, "limited_action", json!(["other caller"]), None)?;
            s.rows(
                "action_log",
                &["identity", "payload"],
                json!([[first_id, "one"], [second_id, "other caller"]]),
            )?;
            std::thread::sleep(std::time::Duration::from_millis(1100));
            s.expect_call_as(&first, "limited_action", json!(["two"]), None)?;
            s.rows(
                "action_log",
                &["identity", "payload"],
                json!([[first_id, "one"], [second_id, "other caller"], [first_id, "two"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
