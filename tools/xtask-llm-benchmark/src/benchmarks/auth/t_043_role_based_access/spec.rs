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
            let (admin, admin_id) = s.new_user()?;
            let (member, member_id) = s.new_user()?;
            let (outsider, outsider_id) = s.new_user()?;
            s.seed("user", &["identity", "role"], json!([[admin_id, "admin"]]))?;
            s.expect_rejected_as(&outsider, "member_action", json!([]), "not registered")?;
            s.expect_rejected_as(&outsider, "admin_action", json!([]), "not registered")?;
            s.expect_rejected_as(&outsider, "promote", json!([member_id]), "not registered")?;
            s.expect_call_as(&member, register, json!([]), None)?;
            s.expect_rejected_as(&member, register, json!([]), "already registered")?;
            s.expect_call_as(&member, "member_action", json!([]), None)?;
            s.expect_rejected_as(&member, "admin_action", json!([]), "not admin")?;
            s.expect_rejected_as(&member, "promote", json!([member_id]), "not admin")?;
            s.expect_rejected_as(&admin, "promote", json!([outsider_id]), "target not registered")?;
            s.expect_call_as(&admin, "promote", json!([member_id]), None)?;
            s.expect_call_as(&member, "admin_action", json!([]), None)?;
            s.owner_rows(
                "user",
                &["identity", "role"],
                json!([[admin_id, "admin"], [member_id, "admin"]]),
            )?;
            Ok(())
        }));
        scorers
    })
}
