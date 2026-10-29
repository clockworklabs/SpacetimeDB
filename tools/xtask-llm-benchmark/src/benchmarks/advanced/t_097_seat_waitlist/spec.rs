use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("create_event", json!([7, 2]))?;
            s.call("create_event", json!([8, 1]))?;
            s.reject("create_event", json!([7, 99]), "event exists")?;
            let users = (0..5)
                .map(|_| s.new_user().map(|u| u.0))
                .collect::<anyhow::Result<Vec<_>>>()?;
            for user in &users {
                s.expect_call_as(user, "join_event", json!([7]), None)?;
            }
            s.expect_call_as(&users[0], "join_event", json!([8]), None)?;
            s.expect_call_as(&users[0], "join_event", json!([7]), None)?;
            s.expect_call_as(&users[2], "join_event", json!([7]), None)?;
            s.rows(
                "booking",
                &["id", "event_id", "status"],
                json!([
                    [1, 7, "seated"],
                    [2, 7, "seated"],
                    [3, 7, "waiting"],
                    [4, 7, "waiting"],
                    [5, 7, "waiting"],
                    [6, 8, "seated"]
                ]),
            )?;
            s.expect_call_as(&users[4], "cancel_booking", json!([1]), Some("owner only"))?;
            std::thread::scope(|scope| {
                let a = scope.spawn(|| s.expect_call_as(&users[0], "cancel_booking", json!([1]), None));
                let b = scope.spawn(|| s.expect_call_as(&users[1], "cancel_booking", json!([2]), None));
                a.join().unwrap()?;
                b.join().unwrap()
            })?;
            s.expect_call_as(&users[0], "cancel_booking", json!([1]), None)?;
            s.rows(
                "booking",
                &["id", "status"],
                json!([
                    [1, "cancelled"],
                    [2, "cancelled"],
                    [3, "seated"],
                    [4, "seated"],
                    [5, "waiting"],
                    [6, "seated"]
                ]),
            )?;
            s.expect_call_as(&users[4], "cancel_booking", json!([5]), None)?;
            s.expect_call_as(&users[0], "join_event", json!([7]), None)?;
            s.expect_call_as(&users[2], "cancel_booking", json!([3]), None)?;
            s.rows(
                "booking",
                &["id", "status"],
                json!([
                    [1, "cancelled"],
                    [2, "cancelled"],
                    [3, "cancelled"],
                    [4, "seated"],
                    [5, "cancelled"],
                    [6, "seated"],
                    [7, "seated"]
                ]),
            )?;
            s.expect_call_as(&users[0], "join_event", json!([404]), Some("missing event"))?;
            s.eventually_count("booking", 7)?;
            Ok(())
        }));
        scorers
    })
}
