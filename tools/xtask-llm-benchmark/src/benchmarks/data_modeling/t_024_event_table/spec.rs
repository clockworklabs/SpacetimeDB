use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let query = format!("SELECT * FROM \"{}\"", s.relation("damage_event"));
            let update = s.watch_update(&query, || s.call("deal_damage", json!([7, 12, "fire"])))?;
            anyhow::ensure!(update.contains("fire") && update.contains("12"), "missing event: {update}");
            s.rows("damage_event", &["entity_id", "damage", "source"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
