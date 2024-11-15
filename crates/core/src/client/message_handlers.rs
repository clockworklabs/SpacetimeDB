use super::messages::{SubscriptionUpdateMessage, SwitchedServerMessage, ToProtocol, TransactionUpdateMessage};
use super::{ClientConnection, DataMessage};
use crate::energy::EnergyQuanta;
use crate::execution_context::WorkloadType;
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::{ReducerArgs, ReducerId, Timestamp};
use crate::identity::Identity;
use crate::messages::websocket::{CallReducer, ClientMessage, OneOffQuery};
use crate::worker_metrics::WORKER_METRICS;
use bytes::Bytes;
use bytestring::ByteString;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::{bsatn, Address};
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
        DataMessage::Text(message) => {
            let message = ByteString::from(message);
            // TODO: update json clients and use the ws version
            serde_json::from_str::<DeserializeWrapper<ClientMessage<ByteString>>>(&message)?
                .0
                .map_args(ReducerArgs::Json)
        }
        DataMessage::Binary(message_buf) => {
            let message_buf = Bytes::from(message_buf);
            bsatn::from_slice::<ClientMessage<Bytes>>(&message_buf)?.map_args(ReducerArgs::Bsatn)
        }
    };

    let address = client.module.info().database_identity;
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
                .with_label_values(&WorkloadType::Reducer, &address, reducer)
                .observe(timer.elapsed().as_secs_f64());
            res.map(drop).map_err(|e| {
                (
                    Some(reducer),
                    client.module.info().reducers_map.lookup_id(reducer),
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
        ClientMessage::Unsubscribe(request) => {
            let res = client.unsubscribe(request, timer).await;
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Unsubscribe, &address, "")
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

#[derive(thiserror::Error, Debug)]
#[error("error executing message (reducer: {reducer:?}) (err: {err:?})")]
pub struct MessageExecutionError {
    pub reducer: Option<Box<str>>,
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
