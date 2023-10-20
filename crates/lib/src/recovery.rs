use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Identity;

#[derive(Deserialize, Serialize, Clone)]
pub struct RecoveryCode {
    pub code: String,
    #[serde(with = "ts_seconds")]
    pub generation_time: DateTime<Utc>,
    pub identity: Identity,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryCodeResponse {
    pub identity: Identity,
    pub token: String,
}
