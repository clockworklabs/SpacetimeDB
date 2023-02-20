use chrono::Utc;
use rand::Rng;
use spacetimedb_lib::recovery::RecoveryCode;

pub fn gen_new_recovery_code(identity: String) -> Result<RecoveryCode, anyhow::Error> {
    let code = rand::thread_rng().gen_range(0..=999999);
    Ok(RecoveryCode {
        code: format!("{code:06}"),
        generation_time: Utc::now(),
        identity,
    })
}
