use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            for row in [
                json!([50, "a", 20, true]),
                json!([8, "a", 10, true]),
                json!([3, "b", 10, true]),
                json!([1, "a", 5, false]),
                json!([9, "a", 10, true]),
                json!([4, "a", 6, false]),
                json!([2, "a", 10, true]),
            ] {
                s.call("add_entry", row)?;
            }
            s.call("read_page", json!(["a", 0, 0, 2]))?;
            s.rows(
                "page_entry",
                &["position", "entry_id", "occurred_at"],
                json!([[0, 2, 10], [1, 8, 10]]),
            )?;
            s.reject("read_page", json!(["a", 0, 0, 0]), "invalid limit")?;
            s.rows(
                "page_entry",
                &["position", "entry_id", "occurred_at"],
                json!([[0, 2, 10], [1, 8, 10]]),
            )?;
            // Insert behind and ahead of the cursor between pages.
            s.call("add_entry", json!([6, "a", 10, true]))?;
            s.call("add_entry", json!([10, "a", 10, true]))?;
            s.call("read_page", json!(["a", 10, 8, 2]))?;
            s.rows(
                "page_entry",
                &["position", "entry_id", "occurred_at"],
                json!([[0, 9, 10], [1, 10, 10]]),
            )?;
            s.call("read_page", json!(["a", 10, 10, 2]))?;
            s.rows(
                "page_entry",
                &["position", "entry_id", "occurred_at"],
                json!([[0, 50, 20]]),
            )?;
            s.call("read_page", json!(["a", 20, 50, 2]))?;
            s.rows("page_entry", &["position", "entry_id", "occurred_at"], json!([]))?;
            s.call("read_page", json!(["b", 0, 0, 2]))?;
            s.rows(
                "page_entry",
                &["position", "entry_id", "occurred_at"],
                json!([[0, 3, 10]]),
            )?;
            s.reject("read_page", json!(["b", 0, 0, 11]), "invalid limit")?;
            Ok(())
        }));
        scorers
    })
}
