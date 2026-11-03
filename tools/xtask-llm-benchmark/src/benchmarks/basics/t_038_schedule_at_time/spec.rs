use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let table = crate::eval::table_name("reminder", s.lang);
            let column = crate::eval::ident("scheduled_at", crate::eval::casing_for_lang(s.lang));
            let rows = s.owner_sql(&format!("SELECT \"{column}\" FROM \"{table}\""))?;
            anyhow::ensure!(rows.as_array().map(Vec::len) == Some(1), "expected one reminder, got {rows}");
            // Tag 1 is ScheduleAt::Time, about 60 seconds after publishing, not an interval.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_micros() as i64;
            let due = rows[0][0][1][0].as_i64().unwrap_or_default();
            anyhow::ensure!(
                rows[0][0][0] == 1 && due > now && due <= now + 61_000_000,
                "expected a time about 60 seconds ahead, got {rows}"
            );
            Ok(())
        }));
        scorers
    })
}
