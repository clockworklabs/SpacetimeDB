use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            for (id, value) in [(2, "old"), (9, "delete"), (27, "tail")] {
                s.call("seed_legacy", json!([id, value]))?;
            }
            s.call("capture_batch", json!([0, 2]))?;
            s.rows("backfill_snapshot", &["id", "revision"], json!([[2, 1], [9, 1]]))?;
            s.call("write_item", json!([2, "new"]))?;
            s.call("delete_item", json!([9]))?;
            s.call("delete_item", json!([9]))?;
            s.call("apply_batch", json!([]))?;
            s.call("apply_batch", json!([]))?;
            s.rows("item_v2", &["id", "value", "revision"], json!([[2, "new", 2]]))?;
            s.call("write_item", json!([9, "reborn"]))?;
            s.call("apply_batch", json!([]))?;
            s.rows(
                "item_v2",
                &["id", "value", "revision"],
                json!([[2, "new", 2], [9, "reborn", 3]]),
            )?;
            s.call("capture_batch", json!([9, 1]))?;
            s.reject("capture_batch", json!([0, 0]), "invalid limit")?;
            s.rows("backfill_snapshot", &["id"], json!([[27]]))?;
            s.call("apply_batch", json!([]))?;
            s.rows(
                "item_v2",
                &["id", "value", "revision"],
                json!([[2, "new", 2], [9, "reborn", 3], [27, "tail", 1]]),
            )?;
            s.call("capture_batch", json!([27, 2]))?;
            s.call("apply_batch", json!([]))?;
            s.rows("backfill_snapshot", &["id"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
