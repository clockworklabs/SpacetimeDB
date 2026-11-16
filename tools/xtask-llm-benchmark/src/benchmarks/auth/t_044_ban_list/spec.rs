use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            let (admin, admin_id) = s.new_user()?;
            let (player, player_id) = s.new_user()?;
            let (other, other_id) = s.new_user()?;
            s.seed("admin", &["identity"], json!([[admin_id]]))?;
            s.expect_rejected_as(&player, "add_admin", json!([player_id]), "not admin")?;
            s.expect_rejected_as(&player, "ban_player", json!([other_id]), "not admin")?;
            s.expect_call_as(&admin, "add_admin", json!([other_id]), None)?;
            s.expect_call_as(&admin, "add_admin", json!([other_id]), None)?;
            s.expect_call_as(&player, "join_game", json!(["Ann"]), None)?;
            s.expect_rejected_as(&player, "join_game", json!(["Ann again"]), "already in game")?;
            s.rows("player", &["identity", "name"], json!([[player_id, "Ann"]]))?;
            s.expect_call_as(&admin, "ban_player", json!([player_id]), None)?;
            s.rows("player", &["identity"], json!([]))?;
            s.expect_rejected_as(&player, "join_game", json!(["Ann"]), "banned")?;
            s.expect_call_as(&other, "ban_player", json!([admin_id]), None)?;
            Ok(())
        }));
        scorers
    })
}
