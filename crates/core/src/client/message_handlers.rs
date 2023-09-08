use std::time::Duration;

use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::{EnergyDiff, ReducerArgs, Timestamp};
use crate::identity::Identity;
use crate::protobuf::client_api::{message, FunctionCall, Message, Subscribe};
use crate::worker_metrics::WORKER_METRICS;
use bytes::Bytes;
use bytestring::ByteString;
use prost::Message as _;

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
        _ => return Err(MessageHandleError::InvalidMessage),
    };

    message.handle(client).await?;

    Ok(())
}

async fn handle_text(client: &ClientConnection, message: String) -> Result<(), MessageHandleError> {
    #[derive(serde::Deserialize)]
    enum Message<'a> {
        #[serde(rename = "call")]
        Call {
            #[serde(borrow, rename = "fn")]
            func: std::borrow::Cow<'a, str>,
            args: &'a serde_json::value::RawValue,
        },
        #[serde(rename = "subscribe")]
        Subscribe { query_strings: Vec<String> },
    }

    let message = ByteString::from(message);
    let msg = serde_json::from_str::<Message>(&message)?;
    let msg = match msg {
        Message::Call { ref func, args } => {
            let args = ReducerArgs::Json(message.slice_ref(args.get()));
            DecodedMessage::Call { reducer: func, args }
        }
        Message::Subscribe { query_strings } => DecodedMessage::Subscribe(Subscribe { query_strings }),
    };

    msg.handle(client).await?;

    Ok(())
}

enum DecodedMessage<'a> {
    Call { reducer: &'a str, args: ReducerArgs },
    Subscribe(Subscribe),
}

impl DecodedMessage<'_> {
    async fn handle(self, client: &ClientConnection) -> Result<(), MessageExecutionError> {
        let res = match self {
            DecodedMessage::Call { reducer, args } => {
                let res = client.call_reducer(reducer, args).await;
                res.map(drop).map_err(|e| (Some(reducer), e.into()))
            }
            DecodedMessage::Subscribe(subscription) => client.subscribe(subscription).map_err(|e| (None, e.into())),
        };
        res.map_err(|(reducer, err)| MessageExecutionError {
            reducer: reducer.map(str::to_owned),
            caller_identity: client.id.identity,
            err,
        })
    }
}

/// An error that arises from
#[derive(thiserror::Error, Debug)]
#[error("error executing message (reducer: {reducer:?}) (err: {err:?})")]
pub struct MessageExecutionError {
    pub reducer: Option<String>,
    pub caller_identity: Identity,
    #[source]
    pub err: anyhow::Error,
}

impl MessageExecutionError {
    fn into_event(self) -> ModuleEvent {
        ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: self.caller_identity,
            function_call: ModuleFunctionCall {
                reducer: self.reducer.unwrap_or_else(|| "<none>".to_owned()),
                args: Default::default(),
            },
            status: EventStatus::Failed(format!("{:#}", self.err)),
            energy_quanta_used: EnergyDiff::ZERO,
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
