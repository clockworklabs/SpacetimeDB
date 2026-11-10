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
                &["id", "category", "amount"],
                json!([[1, "tools", 1], [2, "toys", 2], [3, "tools", 3], [4, "books", 4]]),
            )?;
            s.call("collect_distinct_categories", json!([]))?;
            s.owner_rows(
                "distinct_category",
                &["category"],
                json!([["books"], ["tools"], ["toys"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
