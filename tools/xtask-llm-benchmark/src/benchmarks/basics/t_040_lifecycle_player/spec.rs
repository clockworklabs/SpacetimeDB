use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (user, user_id) = s.new_user()?;
            let live = s.connect(&user)?;
            s.presence_rows("online_player", "identity", &["identity"], json!([[user_id]]))?;
            drop(live);
            s.presence_rows("online_player", "identity", &["identity"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
