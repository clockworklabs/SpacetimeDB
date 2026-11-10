use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            // C# uses RegisterUser because Register collides with a generated member.
            let register = if s.lang == crate::eval::Lang::CSharp {
                "register_user"
            } else {
                "register"
            };
            let (user, user_id) = s.new_user()?;
            s.expect_rejected_as(&user, "post_message", json!(["early"]), "not registered")?;
            s.rows("message", &["id"], json!([]))?;
            s.expect_call_as(&user, register, json!(["Ann"]), None)?;
            s.expect_rejected_as(&user, register, json!(["Ann again"]), "already registered")?;
            s.expect_call_as(&user, "post_message", json!(["hello"]), None)?;
            s.rows("message", &["id", "sender", "text"], json!([[1, user_id, "hello"]]))?;
            s.owner_rows("user", &["identity", "name"], json!([[user_id, "Ann"]]))?;
            Ok(())
        }));
        scorers
    })
}
