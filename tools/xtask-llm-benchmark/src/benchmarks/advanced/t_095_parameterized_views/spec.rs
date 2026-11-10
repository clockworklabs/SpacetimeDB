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
            s.expect_call_as(&alice, "put_note", json!([1, "red", "first"]), None)?;
            s.expect_call_as(&alice, "put_note", json!([2, "blue", "second"]), None)?;
            s.expect_call_as(&bob, "put_note", json!([3, "red", "private"]), None)?;
            s.expect_rejected_as(&bob, "put_note", json!([1, "red", "stolen"]), "owner only")?;
            let view = s.relation("notes_for_tag");
            let query = |tag: &str| format!("SELECT * FROM {view}('{tag}')");
            let mut red = s.connect(&alice)?;
            let mut blue = s.connect(&alice)?;
            red.subscribe(1, &query("red"))?;
            blue.subscribe(1, &query("blue"))?;
            red.expect_update("SubscribeApplied", json!([[1, "first"]]), json!([]))?;
            blue.expect_update("SubscribeApplied", json!([[2, "second"]]), json!([]))?;
            s.expect_call_as(&alice, "put_note", json!([1, "blue", "moved"]), None)?;
            red.expect_update("TransactionUpdate", json!([]), json!([[1, "first"]]))?;
            blue.expect_update("TransactionUpdate", json!([[1, "moved"]]), json!([]))?;
            s.expect_rejected_as(&bob, "remove_note", json!([2]), "owner only")?;
            s.expect_call_as(&alice, "remove_note", json!([2]), None)?;
            blue.expect_update("TransactionUpdate", json!([]), json!([[2, "second"]]))?;
            s.expect_call_as(&alice, "remove_note", json!([404]), None)?;
            let mut other = s.connect(&bob)?;
            other.subscribe(1, &query("blue"))?;
            other.expect_update("SubscribeApplied", json!([]), json!([]))?;
            let (status, _) = s.sql_as(&bob, &format!("SELECT * FROM {}", s.relation("private_note")))?;
            anyhow::ensure!((400..500).contains(&status), "private source exposed: {status}");
            Ok(())
        }));
        scorers
    })
}
