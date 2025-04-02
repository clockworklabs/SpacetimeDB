use super::messages::{SubscriptionUpdateMessage, SwitchedServerMessage, ToProtocol, TransactionUpdateMessage};
use super::{ClientConnection, DataMessage, Protocol};
use crate::energy::EnergyQuanta;
use crate::execution_context::WorkloadType;
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::{ReducerArgs, ReducerId};
use crate::identity::Identity;
use crate::messages::websocket::{CallReducer, ClientMessage, OneOffQuery};
use crate::worker_metrics::WORKER_METRICS;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::{bsatn, ConnectionId, Timestamp};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        .with_label_values(&client.replica_id, message_kind)
        .observe(message.len() as f64);

    WORKER_METRICS
        .websocket_requests
        .with_label_values(&client.replica_id, message_kind)
        .inc();

    let message = match message {
        DataMessage::Text(text) => {
            // TODO(breaking): this should ideally be &serde_json::RawValue, not json-nested-in-string
            let DeserializeWrapper(message) =
                serde_json::from_str::<DeserializeWrapper<ClientMessage<Cow<str>>>>(&text)?;
            message.map_args(|s| {
                ReducerArgs::Json(match s {
                    Cow::Borrowed(s) => text.slice_ref(s),
                    Cow::Owned(string) => string.into(),
                })
            })
        }
        DataMessage::Binary(message_buf) => bsatn::from_slice::<ClientMessage<&[u8]>>(&message_buf)?
            .map_args(|b| ReducerArgs::Bsatn(message_buf.slice_ref(b))),
    };

    let database_identity = client.module.info().database_identity;
    let res = match message {
        ClientMessage::CallReducer(CallReducer {
            ref reducer,
            args,
            request_id,
            flags,
        }) => {
            let res = client.call_reducer(reducer, args, request_id, timer, flags).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Reducer, &database_identity, reducer)
                .observe(timer.elapsed().as_secs_f64());
            res.map(drop).map_err(|e| {
                (
                    Some(reducer),
                    client
                        .module
                        .info()
                        .module_def
                        .reducer_full(&**reducer)
                        .map(|(id, _)| id),
                    e.into(),
                )
            })
        }
        ClientMessage::SubscribeMulti(subscription) => {
            let res = client.subscribe_multi(subscription, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Subscribe, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::UnsubscribeMulti(request) => {
            let res = client.unsubscribe_multi(request, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Unsubscribe, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::SubscribeSingle(subscription) => {
            let res = client.subscribe_single(subscription, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Subscribe, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::Unsubscribe(request) => {
            let res = client.unsubscribe(request, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Unsubscribe, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::Subscribe(subscription) => {
            let res = client.subscribe(subscription, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Subscribe, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|e| (None, None, e.into()))
        }
        ClientMessage::OneOffQuery(OneOffQuery {
            query_string: query,
            message_id,
        }) => {
            let res = match client.config.protocol {
                Protocol::Binary => client.one_off_query_bsatn(&query, &message_id, timer),
                Protocol::Text => client.one_off_query_json(&query, &message_id, timer),
            };
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Sql, &database_identity, "")
                .observe(timer.elapsed().as_secs_f64());
            res.map_err(|err| (None, None, err))
        }
    };
    res.map_err(|(reducer, reducer_id, err)| MessageExecutionError {
        reducer: reducer.cloned(),
        reducer_id,
        caller_identity: client.id.identity,
        caller_connection_id: Some(client.id.connection_id),
        err,
    })?;

    Ok(())
}

#[derive(thiserror::Error, Debug)]
#[error("error executing message (reducer: {reducer:?}) (err: {err:?})")]
pub struct MessageExecutionError {
    pub reducer: Option<Box<str>>,
    pub reducer_id: Option<ReducerId>,
    pub caller_identity: Identity,
    pub caller_connection_id: Option<ConnectionId>,
    #[source]
    pub err: anyhow::Error,
}

impl MessageExecutionError {
    fn into_event(self) -> ModuleEvent {
        ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: self.caller_identity,
            caller_connection_id: self.caller_connection_id,
            function_call: ModuleFunctionCall {
                reducer: self.reducer.unwrap_or_else(|| "<none>".into()).into(),
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

impl ToProtocol for MessageExecutionError {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: super::Protocol) -> Self::Encoded {
        TransactionUpdateMessage {
            event: Some(Arc::new(self.into_event())),
            database_update: SubscriptionUpdateMessage::default_for_protocol(protocol, None),
        }
        .to_protocol(protocol)
    }
}
