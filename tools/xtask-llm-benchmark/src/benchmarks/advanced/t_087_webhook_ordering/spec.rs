use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            for body in [
                "bad",
                "a|e|x|v",
                "a|e|-1|v",
                "a|e|0|v",
                "a|e|18446744073709551616|v",
                "a||1|v",
                "a|e|1|v|extra",
                "a|e| 1|v",
            ] {
                s.expect_http("/webhook", body, 400, "invalid")?;
            }
            s.rows("webhook_receipt", &["event_id"], json!([]))?;
            s.expect_http("/webhook", "a|e1|5|new", 200, "applied")?;
            s.expect_http("/webhook", "a|e1|5|new", 200, "duplicate")?;
            s.expect_http("/webhook", "a|e1|99|changed", 200, "conflict")?;
            s.expect_http("/webhook", "b|e1|5|new", 200, "conflict")?;
            s.expect_http("/webhook", "a|e2|3|old", 200, "stale")?;
            s.expect_http("/webhook", "a|e2|3|old", 200, "duplicate")?;
            s.expect_http("/webhook", "b|e3|1|other", 200, "applied")?;
            s.rows(
                "webhook_account",
                &["account", "sequence", "value"],
                json!([["a", 5, "new"], ["b", 1, "other"]]),
            )?;
            let replies = std::thread::scope(|scope| {
                let a = scope.spawn(|| s.http("/webhook", "a|race|6|next"));
                let b = scope.spawn(|| s.http("/webhook", "a|race|6|next"));
                [a.join().unwrap(), b.join().unwrap()]
            });
            let mut bodies = Vec::new();
            for reply in replies {
                let (status, body) = reply?;
                anyhow::ensure!(status == 200, "race status {status}");
                bodies.push(body);
            }
            bodies.sort();
            anyhow::ensure!(bodies == ["applied", "duplicate"], "duplicate deliveries: {bodies:?}");
            s.rows(
                "webhook_receipt",
                &["event_id"],
                json!([["e1"], ["e2"], ["e3"], ["race"]]),
            )?;
            s.rows(
                "webhook_account",
                &["account", "sequence", "value"],
                json!([["a", 6, "next"], ["b", 1, "other"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
