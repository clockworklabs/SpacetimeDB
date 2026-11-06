use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed(
                "player",
                &["id", "name", "score"],
                json!([[1, "low", 5], [2, "top", 50], [3, "mid", 20], [4, "second", 30]]),
            )?;
            s.call("build_leaderboard", json!([2]))?;
            s.owner_rows(
                "leaderboard",
                &["rank", "player_name", "score"],
                json!([[1, "top", 50], [2, "second", 30]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
