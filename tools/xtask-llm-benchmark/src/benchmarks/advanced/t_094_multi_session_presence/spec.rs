use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (alice, _) = s.new_user()?;
            let (bob, _) = s.new_user()?;
            let first = s.connect(&alice)?;
            let second = s.connect(&alice)?;
            let other = s.connect(&bob)?;
            s.presence_rows("online_user", "owner", &["connections"], json!([[2], [1]]))?;
            drop(first);
            s.presence_rows("online_user", "owner", &["connections"], json!([[1], [1]]))?;
            drop(other);
            s.presence_rows("online_user", "owner", &["connections"], json!([[1]]))?;
            let reconnected = s.connect(&alice)?;
            s.presence_rows("online_user", "owner", &["connections"], json!([[2]]))?;
            drop(second);
            s.presence_rows("online_user", "owner", &["connections"], json!([[1]]))?;
            drop(reconnected);
            s.presence_rows("online_user", "owner", &["connections"], json!([]))?;
            s.presence_rows("live_session", "owner", &["connection_id"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
