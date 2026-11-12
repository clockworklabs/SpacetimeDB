use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;
pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            for (place, quantity) in [("east", 10), ("west", 20), ("reserve", 30)] {
                s.call("set_stock", json!([place, 1, quantity]))?;
            }
            s.call("set_stock", json!(["east", 2, 5]))?;
            s.call("transfer", json!(["east", "west", 1, 4]))?;
            s.call("transfer", json!(["east", "west", 2, 1]))?;
            s.rows("east_stock", &["id", "quantity"], json!([[1, 6], [2, 4]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, 24], [2, 1]]))?;
            s.reject("transfer", json!(["east", "reserve", 1, 7]), "insufficient stock")?;
            s.reject("transfer", json!(["east", "missing", 1, 1]), "invalid location")?;
            s.reject("transfer", json!(["east", "east", 1, 1]), "same location")?;
            s.reject("transfer", json!(["east", "west", 1, 0]), "invalid quantity")?;
            s.call("transfer", json!(["reserve", "east", 1, 3]))?;
            s.rows("east_stock", &["id", "quantity"], json!([[1, 9], [2, 4]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, 24], [2, 1]]))?;
            // No view shows the reserve, so prove its balance by emptying it: 30 - 3 + 2 = 29.
            s.call("transfer", json!(["east", "reserve", 1, 2]))?;
            s.reject("transfer", json!(["reserve", "west", 1, 30]), "insufficient stock")?;
            s.call("transfer", json!(["reserve", "west", 1, 29]))?;
            s.reject("transfer", json!(["reserve", "west", 1, 1]), "insufficient stock")?;
            s.rows("east_stock", &["id", "quantity"], json!([[1, 7], [2, 4]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, 53], [2, 1]]))?;
            // Overflow happens after Take. Both submodule writes must roll back together.
            s.call("set_stock", json!(["west", 1, u32::MAX]))?;
            let (token, _) = s.new_user()?;
            let (status, _) = s.call_as(&token, "transfer", json!(["east", "west", 1, 1]))?;
            anyhow::ensure!(status == 530, "expected reducer failure, got {status}");
            s.rows("east_stock", &["id", "quantity"], json!([[1, 7], [2, 4]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, u32::MAX], [2, 1]]))?;
            Ok(())
        }));
        scorers
    })
}
