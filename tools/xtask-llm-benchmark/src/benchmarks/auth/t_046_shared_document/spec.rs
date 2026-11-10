use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (owner, owner_id) = s.new_user()?;
            let (friend, friend_id) = s.new_user()?;
            let (stranger, _) = s.new_user()?;
            s.expect_call_as(&owner, "create_document", json!(["draft"]), None)?;
            s.rows("document", &["id", "owner", "title"], json!([[1, owner_id, "draft"]]))?;
            s.expect_rejected_as(&owner, "share_document", json!([9, friend_id]), "not found")?;
            s.expect_rejected_as(&stranger, "share_document", json!([1, friend_id]), "not owner")?;
            s.expect_rejected_as(&friend, "edit_document", json!([9, "x"]), "not found")?;
            s.expect_rejected_as(&friend, "edit_document", json!([1, "early"]), "unauthorized")?;
            s.expect_call_as(&owner, "share_document", json!([1, friend_id]), None)?;
            s.expect_call_as(&friend, "edit_document", json!([1, "shared edit"]), None)?;
            s.expect_rejected_as(&stranger, "edit_document", json!([1, "stolen"]), "unauthorized")?;
            s.rows(
                "document",
                &["id", "owner", "title"],
                json!([[1, owner_id, "shared edit"]]),
            )?;
            s.expect_call_as(&owner, "edit_document", json!([1, "owner edit"]), None)?;
            s.rows(
                "document",
                &["id", "owner", "title"],
                json!([[1, owner_id, "owner edit"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
