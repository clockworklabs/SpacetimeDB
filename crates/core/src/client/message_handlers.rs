use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::energy::EnergyQuanta;
use crate::execution_context::WorkloadType;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::{ReducerArgs, ReducerId, Timestamp};
use crate::identity::Identity;
use crate::messages::ws::{self, CallReducer, ClientMessage, OneOffQuery, Subscribe};
use crate::worker_metrics::WORKER_METRICS;
use base64::Engine;
use bytes::Bytes;
use bytestring::ByteString;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::{bsatn, Address};

use super::messages::{ServerMessage, TransactionUpdateMessage};
use super::{ClientConnection, DataMessage};

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

    let message = match message {
        DataMessage::Text(message) => {
            let message = ByteString::from(message);
            // TODO: update json clients and use the ws version
            // serde_json::from_str::<ClientMessage<&serde_json::value::RawValue>>(&message)?
            //     .map_args(|args| ReducerArgs::Json(message.slice_ref(args.get())))
            decode_json_message(message)?
        }
        DataMessage::Binary(message_buf) => {
            let message_buf = Bytes::from(message_buf);
            bsatn::from_slice::<ClientMessage<&[u8]>>(&message_buf)?
                .map_args(|args| ReducerArgs::Bsatn(message_buf.slice_ref(args)))
        }
    };

    let address = client.module.info().address;
    let res = match message {
        ClientMessage::CallReducer(CallReducer {
            ref reducer,
            args,
            request_id,
        }) => {
            let ws::ReducerId::Name(reducer) = reducer;
            let res = client.call_reducer(reducer, args, request_id, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Reducer, &address, reducer)
                .observe(timer.elapsed().as_secs_f64());
            res.map(drop).map_err(|e| {
                (
                    Some(reducer),
                    client.module.info().reducers.lookup_id(reducer),
                    e.into(),
                )
            })
        }
        ClientMessage::Subscribe(subscription) => {
            let res = client.subscribe(subscription, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Subscribe, &address, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::OneOffQuery(OneOffQuery {
            query_string: query,
            message_id,
        }) => {
            let res = client.one_off_query(&query, &message_id, timer);
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Sql, &address, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|err| (None, None, err))
        }
    };
    res.map_err(|(reducer, reducer_id, err)| MessageExecutionError {
        reducer: reducer.cloned(),
        reducer_id,
        caller_identity: client.id.identity,
        caller_address: Some(client.id.address),
        err,
    })?;
    Ok(())
}

fn decode_json_message(message: ByteString) -> Result<ClientMessage<ReducerArgs>, MessageHandleError> {
    let msg = match serde_json::from_str::<RawJsonMessage<'_>>(&message)? {
        RawJsonMessage::Call { func, args, request_id } => ClientMessage::CallReducer(CallReducer {
            reducer: ws::ReducerId::Name(func.into_owned()),
            args: ReducerArgs::Json(message.slice_ref(args.get())),
            request_id,
        }),
        RawJsonMessage::Subscribe {
            query_strings,
            request_id,
        } => ClientMessage::Subscribe(Subscribe {
            query_strings,
            request_id,
        }),
        RawJsonMessage::OneOffQuery {
            query_string,
            message_id,
        } => ClientMessage::OneOffQuery(OneOffQuery {
            message_id: base64::engine::general_purpose::STANDARD.decode(&message_id[..])?,
            query_string: query_string.into_owned(),
        }),
    };
    Ok(msg)
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

/// An error that arises from executing a message.
#[derive(thiserror::Error, Debug)]
#[error("error executing message (reducer: {reducer:?}) (err: {err:?})")]
pub struct MessageExecutionError {
    pub reducer: Option<String>,
    pub reducer_id: Option<ReducerId>,
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
                reducer_id: self.reducer_id.unwrap_or(u32::MAX.into()),
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

    fn serialize_binary(self) -> ws::ServerMessage {
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
