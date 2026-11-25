use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (user, _) = s.new_user()?;
            s.expect_call_as(&user, "create_account", json!(["a@example.com", "first"]), None)?;
            let (status, body) = s.call_as(&user, "create_account", json!(["a@example.com", "second"]))?;
            anyhow::ensure!(status == 530, "duplicate email accepted: HTTP {status}: {body}");
            s.expect_call_as(&user, "create_account", json!(["b@example.com", "third"]), None)?;
            s.owner_rows(
                "account",
                &["email", "display_name"],
                json!([["a@example.com", "first"], ["b@example.com", "third"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
