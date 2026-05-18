use super::{ClientConnection, DataMessage, MessageHandleError};
use serde::de::Error as _;
use spacetimedb_lib::bsatn;
use std::time::Instant;

const EMPTY_V3_PAYLOAD_ERR: &str = "v3 websocket binary payload must contain at least one v2 client message";

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
    client.observe_websocket_request_message(&message);
    match message {
        DataMessage::Binary(message_buf) => {
            let mut remaining = &message_buf[..];

            if remaining.is_empty() {
                return Err(bsatn::DecodeError::Other(EMPTY_V3_PAYLOAD_ERR.into()).into());
            }

            loop {
                let message = bsatn::from_reader(&mut remaining)?;
                super::message_handlers_v2::handle_decoded_message(client, message, timer).await?;
                if remaining.is_empty() {
                    break;
                }
            }
        }
        DataMessage::Text(_) => {
            return Err(MessageHandleError::TextDecode(serde_json::Error::custom(
                "v3 websocket does not support text messages",
            )))
        }
    }

    Ok(())
}
