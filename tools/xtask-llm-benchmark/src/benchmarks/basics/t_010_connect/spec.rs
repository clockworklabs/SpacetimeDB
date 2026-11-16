use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (user, _) = s.new_user()?;
            drop(s.connect(&user)?);
            let count = |kind: &str| -> anyhow::Result<u64> {
                let table = crate::eval::table_name("event", s.lang);
                let column = crate::eval::ident("kind", crate::eval::casing_for_lang(s.lang));
                let query = format!("SELECT COUNT(*) AS n FROM \"{table}\" WHERE \"{column}\" = '{kind}'");
                Ok(s.owner_sql(&query)?[0][0].as_u64().unwrap_or_default())
            };
            s.eventually(|| {
                let (connected, disconnected) = (count("connected")?, count("disconnected")?);
                // HTTP SQL runs the same hooks, so the reading itself can be one event ahead.
                anyhow::ensure!(
                    connected >= 1 && disconnected >= 1 && connected.abs_diff(disconnected) <= 1,
                    "connected {connected}, disconnected {disconnected}"
                );
                Ok(())
            })?;
            Ok(())
        }));
        scorers
    })
}
