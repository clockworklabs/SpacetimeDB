use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::energy::EnergyQuanta;
use crate::execution_context::WorkloadType;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::ReducerArgs;
use crate::identity::Identity;
use crate::protobuf::client_api::{message, FunctionCall, Message, Subscribe};
use crate::worker_metrics::WORKER_METRICS;
use base64::Engine;
use bytes::Bytes;
use bytestring::ByteString;
use prost::Message as _;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::{Address, Timestamp};

use super::messages::{ServerMessage, TransactionUpdateMessage};
use super::{ClientConnection, DataMessage};

#[derive(thiserror::Error, Debug)]
pub enum MessageHandleError {
    #[error(transparent)]
    BinaryDecode(#[from] prost::DecodeError),
    #[error("unexpected protobuf message type")]
    InvalidMessage,
    #[error(transparent)]
    TextDecode(#[from] serde_json::Error),
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    #[error(transparent)]
    Execution(#[from] MessageExecutionError),
}

pub async fn handle(client: &ClientConnection, message: DataMessage, timer: Instant) -> Result<(), MessageHandleError> {
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
        DataMessage::Text(message) => handle_text(client, message, timer).await,
        DataMessage::Binary(message_buf) => handle_binary(client, message_buf, timer).await,
    }
}

async fn handle_binary(
    client: &ClientConnection,
    message_buf: Vec<u8>,
    timer: Instant,
) -> Result<(), MessageHandleError> {
    let message = Message::decode(Bytes::from(message_buf))?;
    let message = match message.r#type {
        Some(message::Type::FunctionCall(FunctionCall {
            ref reducer,
            arg_bytes,
            request_id,
        })) => {
            let args = ReducerArgs::Bsatn(arg_bytes.into());
            DecodedMessage::Call {
                reducer,
                args,
                request_id,
            }
        }
        Some(message::Type::Subscribe(subscription)) => DecodedMessage::Subscribe(subscription),
        Some(message::Type::OneOffQuery(ref oneoff)) => DecodedMessage::OneOffQuery {
            query_string: &oneoff.query_string[..],
            message_id: &oneoff.message_id[..],
        },
        _ => return Err(MessageHandleError::InvalidMessage),
    };

    message.handle(client, timer).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
enum RawJsonMessage<'a> {
    #[serde(rename = "call")]
    Call {
        #[serde(borrow, rename = "fn")]
        func: std::borrow::Cow<'a, str>,
        args: &'a serde_json::value::RawValue,
        #[serde(default)]
        request_id: u32,
    },
    #[serde(rename = "subscribe")]
    Subscribe {
        query_strings: Vec<String>,
        #[serde(default)]
        request_id: u32,
    },
    #[serde(rename = "one_off_query")]
    OneOffQuery {
        #[serde(borrow)]
        query_string: std::borrow::Cow<'a, str>,

        /// A base64-encoded string of bytes.
        #[serde(borrow)]
        message_id: std::borrow::Cow<'a, str>,
    },
}

async fn handle_text(client: &ClientConnection, message: String, timer: Instant) -> Result<(), MessageHandleError> {
    let message = ByteString::from(message);
    let msg = serde_json::from_str::<RawJsonMessage>(&message)?;
    let mut message_id_ = Vec::new();
    let msg = match msg {
        RawJsonMessage::Call {
            ref func,
            args,
            request_id,
        } => {
            let args = ReducerArgs::Json(message.slice_ref(args.get()));
            DecodedMessage::Call {
                reducer: func,
                args,
                request_id,
            }
        }

        RawJsonMessage::Subscribe {
            query_strings,
            request_id,
        } => DecodedMessage::Subscribe(Subscribe {
            query_strings,
            request_id,
        }),
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

    msg.handle(client, timer).await?;

    Ok(())
}

enum DecodedMessage<'a> {
    Call {
        reducer: &'a str,
        args: ReducerArgs,
        request_id: RequestId,
    },
    Subscribe(Subscribe),
    OneOffQuery {
        query_string: &'a str,
        message_id: &'a [u8],
    },
}

impl DecodedMessage<'_> {
    async fn handle(self, client: &ClientConnection, timer: Instant) -> Result<(), MessageExecutionError> {
        let address = client.module.info().address;
        let res = match self {
            DecodedMessage::Call {
                reducer,
                args,
                request_id,
            } => {
                let res = client.call_reducer(reducer, args, request_id, timer).await;
                WORKER_METRICS
                    .request_round_trip
                    .with_label_values(&WorkloadType::Reducer, &address, reducer)
                    .observe(timer.elapsed().as_secs_f64());
                res.map(drop).map_err(|e| (Some(reducer), e.into()))
            }
            DecodedMessage::Subscribe(subscription) => {
                let res = client.subscribe(subscription, timer).await;
                WORKER_METRICS
                    .request_round_trip
                    .with_label_values(&WorkloadType::Subscribe, &address, "")
                    .observe(timer.elapsed().as_secs_f64());
                res.map_err(|e| (None, e.into()))
            }
            DecodedMessage::OneOffQuery {
                query_string: query,
                message_id,
            } => {
                let res = client.one_off_query(query, message_id, timer).await;
                WORKER_METRICS
                    .request_round_trip
                    .with_label_values(&WorkloadType::Sql, &address, "")
                    .observe(timer.elapsed().as_secs_f64());
                res.map_err(|err| (None, err))
            }
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
            request_id: Some(RequestId::default()),
            timer: None,
        }
    }
}

impl ServerMessage for MessageExecutionError {
    fn serialize_text(self) -> crate::json::client_api::MessageJson {
        TransactionUpdateMessage::<DatabaseUpdate> {
            event: Arc::new(self.into_event()),
            database_update: Default::default(),
        }
        .serialize_text()
    }

    fn serialize_binary(self) -> Message {
        TransactionUpdateMessage::<DatabaseUpdate> {
            event: Arc::new(self.into_event()),
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

    #[test]
    fn parse_function_call() {
        let message = r#"{ "call": { "fn": "reducer_name", "request_id": 2, "args": "{}" } }"#;
        let parsed = serde_json::from_str::<RawJsonMessage>(message).unwrap();

        if let RawJsonMessage::Call {
            request_id,
            func,
            args: _,
        } = parsed
        {
            assert_eq!(request_id, 2);
            assert_eq!(func, "reducer_name");
        } else {
            panic!("wrong variant")
        }
    }
}
