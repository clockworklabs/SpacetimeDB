use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (owner, _) = s.new_user()?;
            let (reader, reader_id) = s.new_user()?;
            let (outsider, outsider_id) = s.new_user()?;
            s.expect_call_as(&owner, "set_document", json!(["first", "secret-one"]), None)?;
            s.rows_as(&reader, "visible_document", &["id", "title"], json!([]))?;
            s.expect_call_as(&reader, "set_document", json!(["stolen", "leak"]), Some("owner only"))?;
            s.expect_call_as(&reader, "set_access", json!([outsider_id, true]), Some("owner only"))?;
            let (code, _) = s.sql_as(
                &reader,
                &format!(
                    "SELECT * FROM \"{}\"",
                    crate::eval::table_name("private_document", s.lang)
                ),
            )?;
            anyhow::ensure!((400..500).contains(&code), "private source exposed: HTTP {code}");
            let query = format!(
                "SELECT * FROM \"{}\"",
                crate::eval::table_name("visible_document", s.lang)
            );
            let mut live = s.connect(&reader)?;
            live.subscribe(1, &query)?;
            live.expect_update("SubscribeApplied", json!([]), json!([]))?;
            s.expect_call_as(&owner, "set_access", json!([reader_id, true]), None)?;
            live.expect_update("TransactionUpdate", json!([[1, "first"]]), json!([]))?;
            s.expect_call_as(&owner, "set_document", json!(["second", "secret-two"]), None)?;
            live.expect_update("TransactionUpdate", json!([[1, "second"]]), json!([[1, "first"]]))?;
            s.expect_call_as(&owner, "set_access", json!([reader_id, false]), None)?;
            live.expect_update("TransactionUpdate", json!([]), json!([[1, "second"]]))?;
            s.expect_call_as(&owner, "set_document", json!(["after-revoke", "secret-three"]), None)?;
            s.rows_as(&reader, "visible_document", &["id", "title"], json!([]))?;
            s.rows_as(&outsider, "visible_document", &["id", "title"], json!([]))?;
            // An acknowledged fresh subscription is also a barrier: an intervening leaked update fails.
            live.subscribe(2, &query)?;
            live.expect_update("SubscribeApplied", json!([]), json!([]))?;
            s.rows_as(
                &owner,
                "visible_document",
                &["id", "title"],
                json!([[1, "after-revoke"]]),
            )?;
            s.expect_call_as(&owner, "set_access", json!([reader_id, true]), None)?;
            // Two overlapping subscriptions can produce two row copies on the wire.
            let mut reconnected = s.connect(&reader)?;
            reconnected.subscribe(3, &query)?;
            reconnected.expect_update("SubscribeApplied", json!([[1, "after-revoke"]]), json!([]))?;
            Ok(())
        }));
        scorers
    })
}
