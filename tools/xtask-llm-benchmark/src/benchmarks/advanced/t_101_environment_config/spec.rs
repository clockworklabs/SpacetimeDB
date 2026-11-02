use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            // The fixture's spacetime.json supplies these values, and the prompt never states them.
            s.reject("require_development", json!([]), "production locked")?;
            s.call("record_deployment", json!([7]))?;
            s.rows(
                "deployment_note",
                &["id", "region", "mode", "banner"],
                json!([[7, "eu-west-3", "production", "(none)"]]),
            )?;
            s.reject("record_deployment", json!([7]), "note exists")?;
            s.rows("current_deployment", &["region", "mode"], json!([["eu-west-3", "production"]]))?;
            Ok(())
        }));
        scorers
    })
}
