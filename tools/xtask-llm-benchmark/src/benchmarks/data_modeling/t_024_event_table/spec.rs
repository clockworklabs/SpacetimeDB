use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(scenario(file!(), route_tag, host_url, lang, |s| {
            let (token, _) = s.new_user()?;
            let query = format!("SELECT * FROM {}", crate::eval::table_name("damage_event", s.lang));
            let mut first = s.connect_v2(&token)?;
            first.subscribe_empty(&query)?;
            let expected = vec![spacetimedb_lib::bsatn::to_vec(&(7u64, 11u32, "fire"))?];
            s.call("deal_damage", json!([7, 11, "fire"]))?;
            anyhow::ensure!(first.events()? == expected, "wrong first event");
            // A late subscriber must not receive old events.
            let mut late = s.connect_v2(&token)?;
            late.subscribe_empty(&query)?;
            s.call("deal_damage", json!([7, 11, "fire"]))?;
            anyhow::ensure!(first.events()? == expected, "repeated event was lost or changed");
            anyhow::ensure!(late.events()? == expected, "late subscriber missed new event");
            Ok(())
        }));
        scorers
    })
}
