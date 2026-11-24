use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("register_user", json!(["Ann", "ann@example.com", "secret-hash"]))?;
            s.rows("user_public", &["id", "name"], json!([[1, "Ann"]]))?;
            s.owner_rows(
                "user_internal",
                &["id", "name", "email", "password_hash"],
                json!([[1, "Ann", "ann@example.com", "secret-hash"]]),
            )?;
            let (stranger, _) = s.new_user()?;
            let query = format!("SELECT * FROM \"{}\"", crate::eval::table_name("user_internal", s.lang));
            let (status, _) = s.sql_as(&stranger, &query)?;
            anyhow::ensure!((400..500).contains(&status), "private table exposed: HTTP {status}");
            Ok(())
        }));
        scorers
    })
}
