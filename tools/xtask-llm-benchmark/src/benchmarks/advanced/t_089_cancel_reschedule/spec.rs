use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.reject("renew", json!([5, 0]), "invalid delay")?;
            s.call("renew", json!([5, 10000]))?;
            s.call("renew", json!([5, 60000]))?;
            s.call("renew", json!([8, 10000]))?;
            s.call("cancel", json!([8]))?;
            s.call("cancel", json!([8]))?;
            let time = spacetimedb_lib::ScheduleAt::Time(spacetimedb_lib::Timestamp::from_micros_since_unix_epoch(0));
            let time = serde_json::to_value(spacetimedb_lib::ser::serde::SerializeWrapper::from_ref(&time))?;
            s.reject_scheduled("expire_reservation", json!([[999, time, 5, 2]]))?;
            s.eventually_rows(
                "expiry_result",
                &["reservation_id", "generation", "applied"],
                json!([[5, 1, false], [8, 1, false]]),
            )?;
            s.rows(
                "timed_reservation",
                &["id", "generation", "status"],
                json!([[5, 2, "active"], [8, 2, "cancelled"]]),
            )?;
            s.call("renew", json!([8, 1]))?;
            s.eventually_rows(
                "timed_reservation",
                &["id", "generation", "status"],
                json!([[5, 2, "active"], [8, 3, "expired"]]),
            )?;
            s.eventually_rows(
                "expiry_result",
                &["reservation_id", "generation", "applied"],
                json!([[5, 1, false], [8, 1, false], [8, 3, true]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
