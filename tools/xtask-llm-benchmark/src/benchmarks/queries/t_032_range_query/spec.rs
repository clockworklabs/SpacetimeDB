use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "product",
                &["id", "name", "price"],
                json!([[1, "low", 9], [2, "min", 10], [3, "mid", 15], [4, "max", 20], [5, "high", 21]]),
            )?;
            s.call("find_in_price_range", json!([10, 20]))?;
            s.owner_rows(
                "price_range_result",
                &["product_id", "name", "price"],
                json!([[2, "min", 10], [3, "mid", 15], [4, "max", 20]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
