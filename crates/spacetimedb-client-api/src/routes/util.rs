use chrono::Utc;
use ring::rand::SecureRandom;
use spacetimedb_lib::recovery::RecoveryCode;

pub fn gen_new_recovery_code(identity: String) -> Result<RecoveryCode, anyhow::Error> {
    let mut randoms: [u8; 8] = [0; 8];
    let sr = ring::rand::SystemRandom::new();
    match sr.fill(&mut randoms) {
        Ok(..) => {}
        Err(err) => {
            return Err(anyhow::anyhow!("SecureRandom error: {}", err.to_string()));
        }
    }
    let code = u64::from_be_bytes(randoms) % 1000000;
    let mut code = code.to_string();
    while code.len() < 6 {
        code.insert(0, '0');
    }
    return Ok(RecoveryCode {
        code: code.to_string(),
        generation_time: Utc::now(),
        identity,
    });
}
