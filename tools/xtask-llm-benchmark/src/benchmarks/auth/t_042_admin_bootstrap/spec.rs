use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (admin, _) = s.new_user()?;
            let (other, _) = s.new_user()?;
            s.expect_rejected_as(&admin, "admin_action", json!(["early"]), "not bootstrapped")?;
            s.expect_call_as(&admin, "bootstrap_admin", json!([]), None)?;
            s.expect_rejected_as(&other, "bootstrap_admin", json!([]), "already bootstrapped")?;
            s.expect_rejected_as(&other, "admin_action", json!(["stolen"]), "not admin")?;
            s.expect_call_as(&admin, "admin_action", json!(["allowed"]), None)?;
            s.rows("admin_log", &["id", "action"], json!([[1, "allowed"]]))?;
            Ok(())
        }));
        scorers
    })
}
