use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (token, _) = s.new_user()?;
            s.expect_call_as(&token, "add_item", json!([1, ""]), Some("name required"))?;
            s.rows("item", &["id", "name"], json!([]))?;
            s.expect_call_as(&token, "add_item", json!([1, "book"]), None)?;
            s.rows("item", &["id", "name"], json!([[1, "book"]]))?;
            Ok(())
        }));
        scorers
    })
}
