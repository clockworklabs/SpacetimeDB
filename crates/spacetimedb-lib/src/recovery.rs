use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct RecoveryCode {
    pub code: String,
    #[serde(with = "ts_seconds")]
    pub generation_time: DateTime<Utc>,
    pub identity: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryCodeResponse {
    pub identity: String,
    pub token: String,
}
