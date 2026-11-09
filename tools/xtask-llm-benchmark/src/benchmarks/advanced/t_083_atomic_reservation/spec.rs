use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("add_stock", json!([7, 20]))?;
            s.call("add_stock", json!([19, 3]))?;
            for (args, error) in [
                (json!(["retry", 7, 4, 19, 4]), "insufficient stock"),
                (json!(["missing", 7, 2, 99, 1]), "missing product"),
                (json!(["self", 7, 1, 7, 1]), "invalid reservation"),
                (json!(["negative", 7, -1, 19, 1]), "invalid reservation"),
                (json!(["zero", 7, 0, 19, 1]), "invalid reservation"),
            ] {
                s.reject("reserve", args, error)?;
                s.rows("stock", &["id", "available"], json!([[7, 20], [19, 3]]))?;
                s.rows("reservation", &["request_id"], json!([]))?;
            }
            s.call("reserve", json!(["retry", 7, 4, 19, 2]))?;
            s.call("reserve", json!(["retry", 7, 4, 19, 2]))?;
            s.reject("reserve", json!(["retry", 7, 5, 19, 2]), "request conflict")?;
            s.rows("stock", &["id", "available"], json!([[7, 16], [19, 1]]))?;
            s.rows("reservation", &["request_id"], json!([["retry"]]))?;
            // Two callers contend for the last unit. Exactly one must succeed.
            let results = std::thread::scope(|scope| {
                let a = scope.spawn(|| s.call("reserve", json!(["a", 7, 1, 19, 1])));
                let b = scope.spawn(|| s.call("reserve", json!(["b", 7, 1, 19, 1])));
                [a.join().unwrap(), b.join().unwrap()]
            });
            anyhow::ensure!(
                results.iter().filter(|r| r.is_ok()).count() == 1,
                "one reservation must win: {results:?}"
            );
            anyhow::ensure!(
                results
                    .iter()
                    .filter_map(|r| r.as_ref().err())
                    .all(|e| e.to_string().contains("insufficient stock")),
                "unexpected contention error: {results:?}"
            );
            s.rows("stock", &["id", "available"], json!([[7, 15], [19, 0]]))?;
            let winner = if results[0].is_ok() { "a" } else { "b" };
            s.rows("reservation", &["request_id"], json!([["retry"], [winner]]))?;
            Ok(())
        }));
        scorers
    })
}
