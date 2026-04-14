use super::{ClientConnection, DataMessage, WsVersion};
use crate::client::message_handlers_v1::MessageExecutionError;
use spacetimedb_lib::bsatn;
use std::time::Instant;

#[derive(thiserror::Error, Debug)]
pub enum MessageHandleError {
    #[error(transparent)]
    BinaryDecode(#[from] bsatn::DecodeError),
    #[error(transparent)]
    TextDecode(#[from] serde_json::Error),
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    #[error(transparent)]
    Execution(#[from] MessageExecutionError),

    #[error("unsupported websocket version: {0}")]
    UnsupportedVersion(&'static str),
}

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
    match client.config.version {
        WsVersion::V1 => super::message_handlers_v1::handle(client, message, timer).await,
        WsVersion::V2 => super::message_handlers_v2::handle(client, message, timer).await,
    }
}
