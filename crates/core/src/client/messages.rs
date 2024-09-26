use super::{DataMessage, Protocol};
use crate::execution_context::WorkloadType;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::host::ArgsTuple;
use crate::messages::websocket as ws;
use derive_more::From;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, FormatSwitch, JsonFormat, WebsocketFormat, SERVER_MSG_COMPRESSION_TAG_BROTLI,
    SERVER_MSG_COMPRESSION_TAG_NONE,
};
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::Address;
use spacetimedb_sats::bsatn;
use spacetimedb_vm::relation::MemTable;
use std::sync::Arc;
use std::time::Instant;

/// A server-to-client message which can be encoded according to a [`Protocol`],
/// resulting in a [`ToProtocol::Encoded`] message.
pub trait ToProtocol {
    type Encoded;
    /// Convert `self` into a [`Self::Encoded`] where rows and arguments are encoded with `protocol`.
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded;
}

pub(super) type SwitchedServerMessage = FormatSwitch<ws::ServerMessage<BsatnFormat>, ws::ServerMessage<JsonFormat>>;

/// Serialize `msg` into a [`DataMessage`] containing a [`ws::ServerMessage`].
///
/// If `protocol` is [`Protocol::Binary`], the message will be compressed by this method.
pub fn serialize(msg: impl ToProtocol<Encoded = SwitchedServerMessage>, protocol: Protocol) -> DataMessage {
    // TODO(centril, perf): here we are allocating buffers only to throw them away eventually.
    // Consider pooling these allocations so that we reuse them.
    match msg.to_protocol(protocol) {
        FormatSwitch::Json(msg) => serde_json::to_string(&SerializeWrapper::new(msg)).unwrap().into(),
        FormatSwitch::Bsatn(msg) => {
            // First write the tag so that we avoid shifting the entire message at the end.
            let mut msg_bytes = vec![SERVER_MSG_COMPRESSION_TAG_NONE];
            bsatn::to_writer(&mut msg_bytes, &msg).unwrap();

            // Conditionally compress the message.
            let msg_bytes = if ws::should_compress(msg_bytes[1..].len()) {
                let mut out = vec![SERVER_MSG_COMPRESSION_TAG_BROTLI];
                ws::brotli_compress(&msg_bytes[1..], &mut out);
                out
            } else {
                msg_bytes
            };
            msg_bytes.into()
        }
    }
}

#[derive(Debug, From)]
pub enum SerializableMessage {
    Query(OneOffQueryResponseMessage),
    Identity(IdentityTokenMessage),
    Subscribe(SubscriptionUpdateMessage),
    TxUpdate(TransactionUpdateMessage),
}

impl SerializableMessage {
    pub fn num_rows(&self) -> Option<usize> {
        match self {
            Self::Query(msg) => Some(msg.num_rows()),
            Self::Subscribe(msg) => Some(msg.num_rows()),
            Self::TxUpdate(msg) => Some(msg.num_rows()),
            Self::Identity(_) => None,
        }
    }

    pub fn workload(&self) -> Option<WorkloadType> {
        match self {
            Self::Query(_) => Some(WorkloadType::Sql),
            Self::Subscribe(_) => Some(WorkloadType::Subscribe),
            Self::TxUpdate(_) => Some(WorkloadType::Update),
            Self::Identity(_) => None,
        }
    }
}

impl ToProtocol for SerializableMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        match self {
            SerializableMessage::Query(msg) => msg.to_protocol(protocol),
            SerializableMessage::Identity(msg) => msg.to_protocol(protocol),
            SerializableMessage::Subscribe(msg) => msg.to_protocol(protocol),
            SerializableMessage::TxUpdate(msg) => msg.to_protocol(protocol),
        }
    }
}

pub type IdentityTokenMessage = ws::IdentityToken;

impl ToProtocol for IdentityTokenMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        match protocol {
            Protocol::Text => FormatSwitch::Json(ws::ServerMessage::IdentityToken(self)),
            Protocol::Binary => FormatSwitch::Bsatn(ws::ServerMessage::IdentityToken(self)),
        }
    }
}

#[derive(Debug)]
pub struct TransactionUpdateMessage {
    pub event: Arc<ModuleEvent>,
    pub database_update: SubscriptionUpdateMessage,
}

impl TransactionUpdateMessage {
    fn num_rows(&self) -> usize {
        self.database_update.num_rows()
    }
}

impl ToProtocol for TransactionUpdateMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        fn convert<F: WebsocketFormat>(
            event: Arc<ModuleEvent>,
            request_id: Option<u32>,
            upd: ws::DatabaseUpdate<F>,
            conv_args: impl FnOnce(&ArgsTuple) -> F::Single,
        ) -> ws::ServerMessage<F> {
            let status = match &event.status {
                EventStatus::Committed(_) => ws::UpdateStatus::Committed(upd),
                EventStatus::Failed(errmsg) => ws::UpdateStatus::Failed(errmsg.clone()),
                EventStatus::OutOfEnergy => ws::UpdateStatus::OutOfEnergy,
            };

            let args = conv_args(&event.function_call.args);

            let tx_update = ws::TransactionUpdate {
                timestamp: event.timestamp,
                status,
                caller_identity: event.caller_identity,
                reducer_call: ws::ReducerCallInfo {
                    reducer_name: event.function_call.reducer.to_owned(),
                    reducer_id: event.function_call.reducer_id.into(),
                    args,
                    request_id: request_id.unwrap_or(0),
                },
                energy_quanta_used: event.energy_quanta_used,
                host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
                caller_address: event.caller_address.unwrap_or(Address::zero()),
            };

            ws::ServerMessage::TransactionUpdate(tx_update)
        }

        let TransactionUpdateMessage { event, database_update } = self;
        let update = database_update.database_update;
        protocol.assert_matches_format_switch(&update);
        let request_id = database_update.request_id;
        match update {
            FormatSwitch::Bsatn(update) => FormatSwitch::Bsatn(convert(event, request_id, update, |args| {
                Vec::from(args.get_bsatn().clone()).into()
            })),
            FormatSwitch::Json(update) => {
                FormatSwitch::Json(convert(event, request_id, update, |args| args.get_json().clone()))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdateMessage {
    pub database_update: FormatSwitch<ws::DatabaseUpdate<BsatnFormat>, ws::DatabaseUpdate<JsonFormat>>,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
}

impl SubscriptionUpdateMessage {
    pub(crate) fn default_for_protocol(protocol: Protocol, request_id: Option<RequestId>) -> Self {
        Self {
            database_update: match protocol {
                Protocol::Text => FormatSwitch::Json(<_>::default()),
                Protocol::Binary => FormatSwitch::Bsatn(<_>::default()),
            },
            request_id,
            timer: None,
        }
    }

    fn num_rows(&self) -> usize {
        match &self.database_update {
            FormatSwitch::Bsatn(x) => x.num_rows(),
            FormatSwitch::Json(x) => x.num_rows(),
        }
    }
}

impl ToProtocol for SubscriptionUpdateMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        let request_id = self.request_id.unwrap_or(0);
        let total_host_execution_duration_micros = self.timer.map_or(0, |t| t.elapsed().as_micros() as u64);

        protocol.assert_matches_format_switch(&self.database_update);
        match self.database_update {
            FormatSwitch::Bsatn(database_update) => {
                FormatSwitch::Bsatn(ws::ServerMessage::InitialSubscription(ws::InitialSubscription {
                    database_update,
                    request_id,
                    total_host_execution_duration_micros,
                }))
            }
            FormatSwitch::Json(database_update) => {
                FormatSwitch::Json(ws::ServerMessage::InitialSubscription(ws::InitialSubscription {
                    database_update,
                    request_id,
                    total_host_execution_duration_micros,
                }))
            }
        }
    }
}

#[derive(Debug)]
pub struct OneOffQueryResponseMessage {
    pub message_id: Vec<u8>,
    pub error: Option<String>,
    pub results: Vec<MemTable>,
    pub total_host_execution_duration: u64,
}

impl OneOffQueryResponseMessage {
    fn num_rows(&self) -> usize {
        self.results.iter().map(|t| t.data.len()).sum()
    }
}

impl ToProtocol for OneOffQueryResponseMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        fn convert<F: WebsocketFormat>(msg: OneOffQueryResponseMessage) -> ws::ServerMessage<F> {
            let tables = msg
                .results
                .into_iter()
                .map(|table| ws::OneOffTable {
                    table_name: table.head.table_name.clone().into(),
                    rows: F::encode_list(table.data.into_iter()).0,
                })
                .collect();
            ws::ServerMessage::OneOffQueryResponse(ws::OneOffQueryResponse {
                message_id: msg.message_id,
                error: msg.error,
                tables,
                total_host_execution_duration_micros: msg.total_host_execution_duration,
            })
        }

        match protocol {
            Protocol::Text => FormatSwitch::Json(convert(self)),
            Protocol::Binary => FormatSwitch::Bsatn(convert(self)),
        }
    }
}
