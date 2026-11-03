use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let upstream = crate::eval::scenario::HttpFixture::start()?;
            let result = (|| -> anyhow::Result<()> {
                s.output(
                    "fetch_cached",
                    json!(["", "en", 1000, upstream.url]),
                    json!("invalid input"),
                )?;
                anyhow::ensure!(upstream.requests() == 0, "invalid input contacted upstream");
                s.output(
                    "fetch_cached",
                    json!(["book", "en", 60000, upstream.url]),
                    json!("value-1"),
                )?;
                s.output(
                    "fetch_cached",
                    json!(["book", "en", 1000, upstream.url]),
                    json!("value-1"),
                )?;
                anyhow::ensure!(upstream.requests() == 1, "TTL must not be part of cache key");
                s.output(
                    "fetch_cached",
                    json!(["book", "fr", 60000, upstream.url]),
                    json!("value-2"),
                )?;
                s.output(
                    "fetch_cached",
                    json!(["a:b", "c", 60000, upstream.url]),
                    json!("value-3"),
                )?;
                s.output(
                    "fetch_cached",
                    json!(["a", "b:c", 60000, upstream.url]),
                    json!("value-4"),
                )?;
                s.output("fetch_cached", json!(["book", "en", 0, upstream.url]), json!("value-5"))?;
                s.output(
                    "fetch_cached",
                    json!(["book", "en", 60000, upstream.url]),
                    json!("value-1"),
                )?;
                s.output(
                    "fetch_cached",
                    json!(["short", "en", 1, upstream.url]),
                    json!("value-6"),
                )?;
                std::thread::sleep(std::time::Duration::from_millis(30));
                upstream.fail(true);
                s.output(
                    "fetch_cached",
                    json!(["short", "en", 60000, upstream.url]),
                    json!("upstream error"),
                )?;
                upstream.fail(false);
                s.output(
                    "fetch_cached",
                    json!(["short", "en", 60000, upstream.url]),
                    json!("value-8"),
                )?;
                anyhow::ensure!(upstream.requests() == 8, "unexpected upstream call count");
                s.rows(
                    "cache_entry",
                    &["product", "language", "value"],
                    json!([
                        ["book", "en", "value-1"],
                        ["book", "fr", "value-2"],
                        ["a:b", "c", "value-3"],
                        ["a", "b:c", "value-4"],
                        ["short", "en", "value-8"]
                    ]),
                )?;
                Ok(())
            })();
            upstream.finish()?;
            result
        }));
        scorers
    })
}
