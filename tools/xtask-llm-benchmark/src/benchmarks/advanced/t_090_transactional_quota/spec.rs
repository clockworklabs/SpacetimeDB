use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (alice, _) = s.new_user()?;
            let (bob, _) = s.new_user()?;
            s.expect_call_as(&alice, "submit", json!(["retry", -1]), Some("invalid request"))?;
            s.expect_call_as(&alice, "submit", json!(["retry", 4]), Some("quota exceeded"))?;
            s.rows("quota", &["used"], json!([]))?;
            s.rows("accepted_request", &["request_id"], json!([]))?;
            s.expect_call_as(&alice, "submit", json!(["retry", 2]), None)?;
            s.expect_call_as(&alice, "submit", json!(["retry", 2]), None)?;
            s.expect_call_as(&bob, "submit", json!(["retry", 2]), Some("request conflict"))?;
            s.expect_call_as(&alice, "submit", json!(["retry", 1]), Some("request conflict"))?;
            let results = std::thread::scope(|scope| {
                let a = scope.spawn(|| s.call_as(&alice, "submit", json!(["a", 1])));
                let b = scope.spawn(|| s.call_as(&alice, "submit", json!(["b", 1])));
                [a.join().unwrap(), b.join().unwrap()]
            });
            let responses = results.into_iter().collect::<anyhow::Result<Vec<_>>>()?;
            anyhow::ensure!(
                responses.iter().filter(|(code, _)| (200..300).contains(code)).count() == 1,
                "one quota request must win: {responses:?}"
            );
            anyhow::ensure!(
                responses
                    .iter()
                    .filter(|(code, _)| !(200..300).contains(code))
                    .all(|(code, body)| *code == 530 && body.contains("quota exceeded")),
                "unexpected error: {responses:?}"
            );
            s.expect_call_as(&bob, "submit", json!(["bob", 3]), None)?;
            s.rows("quota", &["used"], json!([[3], [3]]))?;
            s.expect_call_as(&alice, "submit", json!(["overflow", 1]), Some("quota exceeded"))?;
            s.rows("quota", &["used"], json!([[3], [3]]))?;
            Ok(())
        }));
        scorers
    })
}
