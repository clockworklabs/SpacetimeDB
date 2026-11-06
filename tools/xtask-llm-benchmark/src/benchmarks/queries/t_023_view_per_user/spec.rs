use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (first, first_id) = s.new_user()?;
            let (second, second_id) = s.new_user()?;
            let (stranger, _) = s.new_user()?;
            s.seed(
                "profile",
                &["id", "identity", "name", "bio"],
                json!([[1, first_id, "Ann", "first"], [2, second_id, "Bo", "second"]]),
            )?;
            s.rows_as(&first, "my_profile", &["id", "name", "bio"], json!([[1, "Ann", "first"]]))?;
            s.rows_as(&second, "my_profile", &["id", "name", "bio"], json!([[2, "Bo", "second"]]))?;
            s.rows_as(&stranger, "my_profile", &["id", "name", "bio"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
