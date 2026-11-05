use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "order",
                &["id", "category", "amount", "fulfilled"],
                json!([[1, "tools", 10, true], [2, "tools", 25, false], [3, "toys", 99, true]]),
            )?;
            s.call("compute_stats", json!(["tools"]))?;
            s.owner_rows("category_stats", &["category", "total_amount", "order_count"], json!([["tools", 35, 2]]))?;
            s.seed("order", &["id", "category", "amount", "fulfilled"], json!([[4, "tools", 5, true]]))?;
            s.call("compute_stats", json!(["tools"]))?;
            s.owner_rows("category_stats", &["category", "total_amount", "order_count"], json!([["tools", 40, 3]]))?;
            Ok(())
        }));
        scorers
    })
}
