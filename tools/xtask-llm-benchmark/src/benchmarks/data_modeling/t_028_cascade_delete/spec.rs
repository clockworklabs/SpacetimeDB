use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.seed("author", &["id", "name"], json!([[1, "gone"], [2, "kept"]]))?;
            s.seed(
                "post",
                &["id", "author_id", "title"],
                json!([[1, 1, "a"], [2, 1, "b"], [3, 2, "c"]]),
            )?;
            s.call("delete_author", json!([1]))?;
            s.owner_rows("author", &["id", "name"], json!([[2, "kept"]]))?;
            s.owner_rows("post", &["id", "author_id", "title"], json!([[3, 2, "c"]]))?;
            Ok(())
        }));
        scorers
    })
}
