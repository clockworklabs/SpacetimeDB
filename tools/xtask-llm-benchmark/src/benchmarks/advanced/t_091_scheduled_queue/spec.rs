use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.reject("enqueue", json!([90, 1000001]), "invalid input")?;
            for row in [json!([4, -7]), json!([9, 13]), json!([20, 0])] {
                s.call("enqueue", row)?;
            }
            s.eventually_rows(
                "queued_work",
                &["id", "status", "attempts"],
                json!([[4, "failed", 1], [9, "complete", 1], [20, "complete", 1]]),
            )?;
            s.rows("work_effect", &["id", "value"], json!([[9, 26], [20, 0]]))?;
            let time = spacetimedb_lib::ScheduleAt::Time(spacetimedb_lib::Timestamp::from_micros_since_unix_epoch(0));
            let time = serde_json::to_value(spacetimedb_lib::ser::serde::SerializeWrapper::from_ref(&time))?;
            s.reject_scheduled("execute_work", json!([[999, time, 9]]))?;
            s.call("enqueue", json!([4, -7]))?;
            s.call("enqueue", json!([9, 13]))?;
            s.reject("enqueue", json!([9, 99]), "request conflict")?;
            s.rows(
                "queued_work",
                &["id", "status", "attempts"],
                json!([[4, "failed", 1], [9, "complete", 1], [20, "complete", 1]]),
            )?;
            s.rows("work_effect", &["id", "value"], json!([[9, 26], [20, 0]]))?;
            Ok(())
        }));
        scorers
    })
}
