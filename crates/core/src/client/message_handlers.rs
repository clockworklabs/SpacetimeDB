use std::time::Duration;

use crate::energy::EnergyQuanta;
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::{ReducerArgs, Timestamp};
use crate::identity::Identity;
use crate::protobuf::client_api::{message, FunctionCall, Message, Subscribe};
use crate::worker_metrics::WORKER_METRICS;
use base64::Engine;
use bytes::Bytes;
use bytestring::ByteString;
use prost::Message as _;
use spacetimedb_lib::Address;

use super::messages::{ServerMessage, TransactionUpdateMessage};
use super::{ClientConnection, DataMessage};

#[derive(thiserror::Error, Debug)]
pub enum MessageHandleError {
    #[error(transparent)]
    BinaryDecode(#[from] prost::DecodeError),
    #[error("unexepected protobuf message type")]
    InvalidMessage,
    #[error(transparent)]
    TextDecode(#[from] serde_json::Error),
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    #[error(transparent)]
    Execution(#[from] MessageExecutionError),
}

pub async fn handle(client: &ClientConnection, message: DataMessage) -> Result<(), MessageHandleError> {
    let message_kind = match message {
        DataMessage::Text(_) => "text",
        DataMessage::Binary(_) => "binary",
    };

    WORKER_METRICS
        .websocket_request_msg_size
        .with_label_values(&client.database_instance_id, message_kind)
        .observe(message.len() as f64);

    WORKER_METRICS
        .websocket_requests
        .with_label_values(&client.database_instance_id, message_kind)
        .inc();

    match message {
        DataMessage::Text(message) => handle_text(client, message).await,
        DataMessage::Binary(message_buf) => handle_binary(client, message_buf).await,
    }
}

async fn handle_binary(client: &ClientConnection, message_buf: Vec<u8>) -> Result<(), MessageHandleError> {
    let message = Message::decode(Bytes::from(message_buf))?;
    let message = match message.r#type {
        Some(message::Type::FunctionCall(FunctionCall { ref reducer, arg_bytes })) => {
            let args = ReducerArgs::Bsatn(arg_bytes.into());
            DecodedMessage::Call { reducer, args }
        }
        Some(message::Type::Subscribe(subscription)) => DecodedMessage::Subscribe(subscription),
        Some(message::Type::OneOffQuery(ref oneoff)) => DecodedMessage::OneOffQuery {
            query_string: &oneoff.query_string[..],
            message_id: &oneoff.message_id[..],
        },
        _ => return Err(MessageHandleError::InvalidMessage),
    };

    message.handle(client).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
enum RawJsonMessage<'a> {
    #[serde(rename = "call")]
    Call {
        #[serde(borrow, rename = "fn")]
        func: std::borrow::Cow<'a, str>,
        args: &'a serde_json::value::RawValue,
    },
    #[serde(rename = "subscribe")]
    Subscribe { query_strings: Vec<String> },
    #[serde(rename = "one_off_query")]
    OneOffQuery {
        #[serde(borrow)]
        query_string: std::borrow::Cow<'a, str>,

        /// A base64-encoded string of bytes.
        #[serde(borrow)]
        message_id: std::borrow::Cow<'a, str>,
    },
}

async fn handle_text(client: &ClientConnection, message: String) -> Result<(), MessageHandleError> {
    let message = ByteString::from(message);
    let msg = serde_json::from_str::<RawJsonMessage>(&message)?;
    let mut message_id_ = Vec::new();
    let msg = match msg {
        RawJsonMessage::Call { ref func, args } => {
            let args = ReducerArgs::Json(message.slice_ref(args.get()));
            DecodedMessage::Call { reducer: func, args }
        }
        RawJsonMessage::Subscribe { query_strings } => DecodedMessage::Subscribe(Subscribe { query_strings }),
        RawJsonMessage::OneOffQuery {
            query_string: ref query,
            message_id,
        } => {
            let _ = std::mem::replace(
                &mut message_id_,
                base64::engine::general_purpose::STANDARD.decode(&message_id[..])?,
            );
            DecodedMessage::OneOffQuery {
                query_string: &query[..],
                message_id: &message_id_[..],
            }
        }
    };

    msg.handle(client).await?;

    Ok(())
}

enum DecodedMessage<'a> {
    Call {
        reducer: &'a str,
        args: ReducerArgs,
    },
    Subscribe(Subscribe),
    OneOffQuery {
        query_string: &'a str,
        message_id: &'a [u8],
    },
}

impl DecodedMessage<'_> {
    async fn handle(self, client: &ClientConnection) -> Result<(), MessageExecutionError> {
        let res = match self {
            DecodedMessage::Call { reducer, args } => {
                let res = client.call_reducer(reducer, args).await;
                res.map(drop).map_err(|e| (Some(reducer), e.into()))
            }
            DecodedMessage::Subscribe(subscription) => client.subscribe(subscription).map_err(|e| (None, e.into())),
            DecodedMessage::OneOffQuery {
                query_string: query,
                message_id,
            } => client.one_off_query(query, message_id).await.map_err(|err| (None, err)),
        };
        res.map_err(|(reducer, err)| MessageExecutionError {
            reducer: reducer.map(str::to_owned),
            caller_identity: client.id.identity,
            caller_address: Some(client.id.address),
            err,
        })
    }
}

/// An error that arises from executing a message.  
#[derive(thiserror::Error, Debug)]
#[error("error executing message (reducer: {reducer:?}) (err: {err:?})")]
pub struct MessageExecutionError {
    pub reducer: Option<String>,
    pub caller_identity: Identity,
    pub caller_address: Option<Address>,
    #[source]
    pub err: anyhow::Error,
}

impl MessageExecutionError {
    fn into_event(self) -> ModuleEvent {
        ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: self.caller_identity,
            caller_address: self.caller_address,
            function_call: ModuleFunctionCall {
                reducer: self.reducer.unwrap_or_else(|| "<none>".to_owned()),
                args: Default::default(),
            },
            status: EventStatus::Failed(format!("{:#}", self.err)),
            energy_quanta_used: EnergyQuanta::ZERO,
            host_execution_duration: Duration::ZERO,
        }
    }
}

impl ServerMessage for MessageExecutionError {
    fn serialize_text(self) -> crate::json::client_api::MessageJson {
        TransactionUpdateMessage {
            event: &mut self.into_event(),
            database_update: Default::default(),
        }
        .serialize_text()
    }

    fn serialize_binary(self) -> Message {
        TransactionUpdateMessage {
            event: &mut self.into_event(),
            database_update: Default::default(),
        }
        .serialize_binary()
    }
}

#[cfg(test)]
mod tests {
    use super::RawJsonMessage;

    #[test]
    fn parse_one_off_query() {
        let message = r#"{ "one_off_query": { "message_id": "ywS3WFquDECZQ0UdLZN1IA==", "query_string": "SELECT * FROM User WHERE name != 'bananas'" } }"#;
        let parsed = serde_json::from_str::<RawJsonMessage>(message).unwrap();

        if let RawJsonMessage::OneOffQuery {
            query_string: query,
            message_id,
        } = parsed
        {
            assert_eq!(query, "SELECT * FROM User WHERE name != 'bananas'");
            assert_eq!(message_id, "ywS3WFquDECZQ0UdLZN1IA==");
        } else {
            panic!("wrong variant")
        }
    }
}
