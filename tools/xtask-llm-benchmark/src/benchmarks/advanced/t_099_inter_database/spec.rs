use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let peer = s.golden_peer();
            let target = peer.database_identity()?;
            let sender = s.database_identity()?;
            s.reject(
                "send_message",
                json!([target, "rolled-back", false, true]),
                "sender rejected",
            )?;
            s.call("send_message", json!([target, "first", false, false]))?;
            s.call("send_message", json!([target, "rejected", true, false]))?;
            s.call("send_message", json!([target, "barrier", false, false]))?;
            s.eventually_rows(
                "delivery_result",
                &["body", "succeeded"],
                json!([["first", true], ["rejected", false], ["barrier", true]]),
            )?;
            // A later delivery is a barrier: rolled-back sends must not arrive before it.
            let sender_row = json!([sender["__identity__"]]);
            peer.rows(
                "receipt",
                &["body", "sender"],
                json!([["first", sender_row], ["barrier", sender_row]]),
            )?;
            s.eventually_count("outgoing", 0)?;
            // Reverse the direction to grade the candidate receiver, not only its sender.
            peer.call("send_message", json!([sender, "incoming", false, false]))?;
            peer.call("send_message", json!([sender, "bad-incoming", true, false]))?;
            peer.eventually_rows(
                "delivery_result",
                &["body", "succeeded"],
                json!([["incoming", true], ["bad-incoming", false]]),
            )?;
            s.rows(
                "receipt",
                &["body", "sender"],
                json!([["incoming", [target["__identity__"]]]]),
            )?;
            peer.eventually_count("outgoing", 0)?;
            Ok(())
        }));
        scorers
    })
}
