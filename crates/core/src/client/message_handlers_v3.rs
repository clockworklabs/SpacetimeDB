use super::{ClientConnection, DataMessage, MessageHandleError};
use serde::de::Error as _;
use spacetimedb_client_api_messages::websocket::{v2 as ws_v2, v3 as ws_v3};
use spacetimedb_lib::bsatn;
use std::time::Instant;

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
    client.observe_websocket_request_message(&message);
    let frame = match message {
        DataMessage::Binary(message_buf) => bsatn::from_slice::<ws_v3::ClientFrame>(&message_buf)?,
        DataMessage::Text(_) => {
            return Err(MessageHandleError::TextDecode(serde_json::Error::custom(
                "v3 websocket does not support text messages",
            )))
        }
    };

    match frame {
        ws_v3::ClientFrame::Single(message) => {
            let message = bsatn::from_slice::<ws_v2::ClientMessage>(&message)?;
            super::message_handlers_v2::handle_decoded_message(client, message, timer).await?;
        }
        ws_v3::ClientFrame::Batch(messages) => {
            for message in messages {
                let message = bsatn::from_slice::<ws_v2::ClientMessage>(&message)?;
                super::message_handlers_v2::handle_decoded_message(client, message, timer).await?;
            }
        }
    }

    Ok(())
}
