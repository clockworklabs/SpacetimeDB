use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (owner, owner_id) = s.new_user()?;
            let (other, _) = s.new_user()?;
            s.expect_call_as(&owner, "create_message", json!(["hello"]), None)?;
            s.rows("message", &["id", "owner", "text"], json!([[1, owner_id, "hello"]]))?;
            s.expect_rejected_as(&other, "delete_message", json!([99]), "not found")?;
            s.expect_rejected_as(&other, "delete_message", json!([1]), "unauthorized")?;
            s.rows("message", &["id", "owner", "text"], json!([[1, owner_id, "hello"]]))?;
            s.expect_call_as(&owner, "delete_message", json!([1]), None)?;
            s.rows("message", &["id"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
