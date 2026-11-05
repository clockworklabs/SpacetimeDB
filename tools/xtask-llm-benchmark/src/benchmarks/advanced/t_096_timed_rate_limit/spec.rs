use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;
use std::time::{Duration, Instant};

fn accepted((status, body): (u16, String)) -> anyhow::Result<bool> {
    if (200..300).contains(&status) {
        return Ok(true);
    }
    anyhow::ensure!(
        status == 530 && (body.contains("rate limited") || body.contains("fatal error")),
        "unexpected failure: {status}: {body}"
    );
    Ok(false)
}

fn check_budget(accepted: u64, elapsed: Duration) -> anyhow::Result<()> {
    // Start before the first request: the bucket cannot be older than this.
    // Delayed requests may legitimately refill tokens; never assume a fixed success count.
    let maximum = 3 + elapsed.as_secs() / 5;
    anyhow::ensure!(accepted <= maximum, "accepted {accepted} requests with at most {maximum} tokens");
    Ok(())
}

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (alice, _) = s.new_user()?;
            let (bob, _) = s.new_user()?;
            s.expect_rejected_as(&alice, "log_search", json!(["bad", ""]), "empty term")?;
            s.rows("search_log", &["request_id"], json!([]))?;
            let started = Instant::now();
            let mut successes = 0;
            let mut rows = Vec::new();
            // The bucket exists once this returns, so time refills from here, never earlier.
            s.expect_call_as(&alice, "log_search", json!(["a", "first"]), None)?;
            let bucket_exists = Instant::now();
            successes += 1;
            rows.push(json!(["a", "first"]));
            for id in ["b", "c"] {
                s.expect_call_as(&alice, "log_search", json!([id, "first"]), None)?;
                successes += 1;
                rows.push(json!([id, "first"]));
            }
            s.expect_call_as(&alice, "log_search", json!(["a", "first"]), None)?;
            s.expect_rejected_as(&alice, "log_search", json!(["a", "changed"]), "request conflict")?;
            s.expect_call_as(&bob, "log_search", json!(["a", "independent"]), None)?;
            rows.push(json!(["a", "independent"]));
            s.rows("search_log", &["request_id", "term"], json!(rows))?;
            if accepted(s.call_as(&alice, "log_search", json!(["d", "next"]))?)? {
                successes += 1;
                rows.push(json!(["d", "next"]));
            }
            check_budget(successes, started.elapsed())?;
            s.rows("search_log", &["request_id", "term"], json!(rows))?;

            // Wait after every preceding call finishes. At least one token must be
            // available, but slow CI can legitimately allow both requests below.
            std::thread::sleep(Duration::from_millis(5200));
            let responses = std::thread::scope(|scope| {
                let a = scope.spawn(|| s.call_as(&alice, "log_search", json!(["e", "next"])));
                let b = scope.spawn(|| s.call_as(&alice, "log_search", json!(["f", "next"])));
                [a.join().unwrap(), b.join().unwrap()]
            });
            let before = successes;
            for (id, response) in ["e", "f"].into_iter().zip(responses) {
                if accepted(response?)? {
                    successes += 1;
                    rows.push(json!([id, "next"]));
                }
            }
            anyhow::ensure!(successes > before, "no request admitted after a full refill interval");
            check_budget(successes, started.elapsed())?;
            s.rows("search_log", &["request_id", "term"], json!(rows))?;

            // Partial intervals carry over: after two full intervals, five tokens have been earned in total.
            std::thread::sleep(Duration::from_millis(10_100).saturating_sub(bucket_exists.elapsed()));
            if successes < 5 {
                anyhow::ensure!(
                    accepted(s.call_as(&alice, "log_search", json!(["g", "next"]))?)?,
                    "partial refill interval was lost"
                );
                rows.push(json!(["g", "next"]));
            }

            // Bob has been idle long enough to refill, but never above three tokens.
            let mut admitted = 0;
            for id in ["b1", "b2", "b3", "b4"] {
                if accepted(s.call_as(&bob, "log_search", json!([id, "burst"]))?)? {
                    admitted += 1;
                    rows.push(json!([id, "burst"]));
                }
            }
            // Bob's next token is not due before 15 seconds, so a slower run cannot assert this.
            if started.elapsed() < Duration::from_secs(14) {
                anyhow::ensure!(admitted == 3, "bucket must hold three tokens, admitted {admitted}");
            }
            s.rows("search_log", &["request_id", "term"], json!(rows))?;
            Ok(())
        }));
        scorers
    })
}

#[test]
fn admission_budget_allows_delayed_refills_but_rejects_overspending() {
    assert!(check_budget(3, Duration::from_secs(4)).is_ok());
    assert!(check_budget(4, Duration::from_secs(4)).is_err());
    assert!(check_budget(4, Duration::from_secs(5)).is_ok());
    assert!(check_budget(5, Duration::from_secs(11)).is_ok());
    assert!(check_budget(6, Duration::from_secs(11)).is_err());
    assert!(accepted((530, "rate limited".into())).is_ok());
    assert!(accepted((530, "fatal error".into())).is_ok());
    assert!(accepted((500, "rate limited".into())).is_err());
}
