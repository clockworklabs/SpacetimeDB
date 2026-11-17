use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("create_player", json!(["Ann", {"some": "Ace"}, {"some": 10}]))?;
            s.call("create_player", json!(["Bo", {"none": []}, {"none": []}]))?;
            let table = crate::eval::table_name("player", s.lang);
            let rows = s.owner_sql(&format!("SELECT * FROM \"{table}\""))?;
            anyhow::ensure!(
                rows == json!([[1, "Ann", [0, "Ace"], [0, 10]], [2, "Bo", [1, []], [1, []]]]),
                "unexpected players: {rows}"
            );
            Ok(())
        }));
        scorers
    })
}
