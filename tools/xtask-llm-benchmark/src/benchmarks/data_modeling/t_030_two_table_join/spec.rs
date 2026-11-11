use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed("customer", &["id", "name"], json!([[1, "Ann"], [2, "Bo"]]))?;
            s.seed(
                "order",
                &["id", "customer_id", "product", "amount"],
                json!([[10, 1, "lamp", 3], [11, 2, "desk", 1], [12, 99, "orphan", 7]]),
            )?;
            s.call("build_order_details", json!([]))?;
            s.owner_rows(
                "order_detail",
                &["order_id", "customer_name", "product", "amount"],
                json!([[10, "Ann", "lamp", 3], [11, "Bo", "desk", 1]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
