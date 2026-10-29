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
            s.call("transfer", json!(["east", "west", 1, 4]))?;
            s.rows("east_stock", &["id", "quantity"], json!([[1, 6]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, 24]]))?;
            s.reject("transfer", json!(["east", "reserve", 1, 7]), "insufficient stock")?;
            s.reject("transfer", json!(["east", "missing", 1, 1]), "invalid location")?;
            s.reject("transfer", json!(["east", "east", 1, 1]), "same location")?;
            s.reject("transfer", json!(["east", "west", 1, 0]), "invalid quantity")?;
            s.call("transfer", json!(["reserve", "east", 1, 3]))?;
            s.rows("east_stock", &["id", "quantity"], json!([[1, 9]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, 24]]))?;
            // Overflow happens after Take. Both submodule writes must roll back together.
            s.call("set_stock", json!(["west", 1, u32::MAX]))?;
            let (token, _) = s.new_user()?;
            let (status, _) = s.call_as(&token, "transfer", json!(["east", "west", 1, 1]))?;
            anyhow::ensure!(status == 530, "expected reducer failure, got {status}");
            s.rows("east_stock", &["id", "quantity"], json!([[1, 9]]))?;
            s.rows("west_stock", &["id", "quantity"], json!([[1, u32::MAX]]))?;
            Ok(())
        }));
        scorers
    })
}
