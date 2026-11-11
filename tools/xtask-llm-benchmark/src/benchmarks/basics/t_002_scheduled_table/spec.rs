use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let query = format!("SELECT * FROM \"{}\"", crate::eval::table_name("tick_timer", s.lang));
            let rows = s.owner_sql(&query)?;
            anyhow::ensure!(
                rows.as_array().map(Vec::len) == Some(1),
                "expected one timer, got {rows}"
            );
            anyhow::ensure!(
                rows.to_string().contains("50000"),
                "expected a 50ms interval, got {rows}"
            );
            std::thread::sleep(std::time::Duration::from_millis(300));
            // An interval row must survive its ticks.
            anyhow::ensure!(s.owner_sql(&query)? == rows, "interval row changed");
            Ok(())
        }));
        scorers
    })
}
