use base64::Engine;
use brotli::CompressorReader;
use derive_more::From;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_vm::relation::MemTable;
use std::io::Read;
use std::sync::Arc;
use std::time::Instant;

use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ProtocolDatabaseUpdate};
use crate::json::client_api::{
    EventJson, FunctionCallJson, IdentityTokenJson, MessageJson, OneOffQueryResponseJson, OneOffTableJson,
    SubscriptionUpdateJson, TableUpdateJson, TransactionUpdateJson,
};
use crate::messages::ws;
use spacetimedb_lib::{bsatn, Address};

use super::message_handlers::MessageExecutionError;
use super::{DataMessage, Protocol};

/// A message sent from the server to the client. Because clients can request either text or binary messages,
/// a server message needs to be encodable as either.
pub trait ServerMessage: Sized {
    fn serialize(self, protocol: Protocol) -> DataMessage {
        match protocol {
            Protocol::Text => self.serialize_text().to_json().into(),
            Protocol::Binary => {
                let msg_bytes = bsatn::to_vec(&self.serialize_binary()).unwrap();
                let reader = &mut &msg_bytes[..];

                // TODO(perf): Compression should depend on message size and type.
                //
                // SubscriptionUpdate messages will typically be quite large,
                // while TransactionUpdate messages will typically be quite small.
                //
                // If we are optimizing for SubscriptionUpdates,
                // we want a large buffer.
                // But if we are optimizing for TransactionUpdates,
                // we probably want to skip compression altogether.
                //
                // For now we choose a reasonable middle ground,
                // which is to compress everything using a 32KB buffer.
                const BUFFER_SIZE: usize = 32 * 1024;
                // Again we are optimizing for compression speed,
                // so we choose the lowest (fastest) level of compression.
                // Experiments on internal workloads have shown compression ratios between 7:1 and 10:1
                // for large `SubscriptionUpdate` messages at this level.
                const COMPRESSION_LEVEL: u32 = 1;
                // The default value for an internal compression parameter.
                // See `BrotliEncoderParams` for more details.
                const LG_WIN: u32 = 22;

                let mut encoder = CompressorReader::new(reader, BUFFER_SIZE, COMPRESSION_LEVEL, LG_WIN);

                let mut out = Vec::new();
                encoder
                    .read_to_end(&mut out)
                    .expect("Failed to Brotli compress `SubscriptionUpdateMessage`");
                out.into()
            }
        }
    }
    fn serialize_text(self) -> MessageJson;
    fn serialize_binary(self) -> ws::ServerMessage;
}

#[derive(Debug, From)]
pub enum SerializableMessage {
    Query(OneOffQueryResponseMessage),
    Error(MessageExecutionError),
    Identity(IdentityTokenMessage),
    Subscribe(SubscriptionUpdateMessage),
    DatabaseUpdate(TransactionUpdateMessage<DatabaseUpdate>),
    ProtocolUpdate(TransactionUpdateMessage<ProtocolDatabaseUpdate>),
}

impl ServerMessage for SerializableMessage {
    fn serialize_text(self) -> MessageJson {
        match self {
            SerializableMessage::Query(msg) => msg.serialize_text(),
            SerializableMessage::Error(msg) => msg.serialize_text(),
            SerializableMessage::Identity(msg) => msg.serialize_text(),
            SerializableMessage::Subscribe(msg) => msg.serialize_text(),
            SerializableMessage::DatabaseUpdate(msg) => msg.serialize_text(),
            SerializableMessage::ProtocolUpdate(msg) => msg.serialize_text(),
        }
    }

    fn serialize_binary(self) -> ws::ServerMessage {
        match self {
            SerializableMessage::Query(msg) => msg.serialize_binary(),
            SerializableMessage::Error(msg) => msg.serialize_binary(),
            SerializableMessage::Identity(msg) => msg.serialize_binary(),
            SerializableMessage::Subscribe(msg) => msg.serialize_binary(),
            SerializableMessage::DatabaseUpdate(msg) => msg.serialize_binary(),
            SerializableMessage::ProtocolUpdate(msg) => msg.serialize_binary(),
        }
    }
}

pub type IdentityTokenMessage = ws::IdentityToken;

impl ServerMessage for IdentityTokenMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::IdentityToken(IdentityTokenJson {
            identity: self.identity,
            token: self.token,
            address: self.address,
        })
    }
    fn serialize_binary(self) -> ws::ServerMessage {
        ws::ServerMessage::IdentityToken(self)
    }
}

#[derive(Debug)]
pub struct TransactionUpdateMessage<U> {
    pub event: Arc<ModuleEvent>,
    pub database_update: SubscriptionUpdate<U>,
}

impl<U: Into<ws::DatabaseUpdate> + Into<Vec<TableUpdateJson>>> ServerMessage for TransactionUpdateMessage<U> {
    fn serialize_text(self) -> MessageJson {
        let Self { event, database_update } = self;
        let (status_str, errmsg) = match &event.status {
            EventStatus::Committed(_) => ("committed", String::new()),
            EventStatus::Failed(errmsg) => ("failed", errmsg.clone()),
            EventStatus::OutOfEnergy => ("out_of_energy", String::new()),
        };

        let event = EventJson {
            timestamp: event.timestamp.0,
            status: status_str.to_string(),
            caller_identity: event.caller_identity,
            function_call: FunctionCallJson {
                reducer: event.function_call.reducer.to_owned(),
                args: event.function_call.args.get_json().clone(),
                request_id: database_update.request_id.unwrap_or(0),
            },
            energy_quanta_used: event.energy_quanta_used.get(),
            message: errmsg,
            caller_address: event.caller_address.unwrap_or(Address::__DUMMY),
        };

        let subscription_update = database_update.into_json();
        MessageJson::TransactionUpdate(TransactionUpdateJson {
            event,
            subscription_update,
        })
    }

    fn serialize_binary(self) -> ws::ServerMessage {
        let Self { event, database_update } = self;
        let status = match &event.status {
            EventStatus::Committed(_) => ws::UpdateStatus::Committed(database_update.database_update.into()),
            EventStatus::Failed(errmsg) => ws::UpdateStatus::Failed(errmsg.clone()),
            EventStatus::OutOfEnergy => ws::UpdateStatus::OutOfEnergy,
        };

        let tx_update = ws::TransactionUpdate {
            timestamp: event.timestamp,
            status,
            caller_identity: event.caller_identity,
            reducer_call: ws::ReducerCallInfo {
                reducer_name: event.function_call.reducer.to_owned(),
                reducer_id: event.function_call.reducer_id.into(),
                args: event.function_call.args.get_bsatn().clone().into(),
                request_id: database_update.request_id.unwrap_or(0),
            },
            energy_quanta_used: event.energy_quanta_used,
            host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
            caller_address: event.caller_address.unwrap_or(Address::zero()),
        };

        ws::ServerMessage::TransactionUpdate(tx_update)
    }
}

#[derive(Debug)]
pub struct SubscriptionUpdateMessage {
    pub subscription_update: SubscriptionUpdate<ProtocolDatabaseUpdate>,
}

impl ServerMessage for SubscriptionUpdateMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::SubscriptionUpdate(self.subscription_update.into_json())
    }

    fn serialize_binary(self) -> ws::ServerMessage {
        let upd = self.subscription_update;
        ws::ServerMessage::InitialSubscription(ws::InitialSubscription {
            database_update: upd.database_update.into(),
            request_id: upd.request_id.unwrap_or(0),
            total_host_execution_duration_micros: upd.timer.map_or(0, |t| t.elapsed().as_micros() as u64),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct SubscriptionUpdate<U> {
    pub database_update: U,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
}

impl<T: Into<Vec<TableUpdateJson>>> SubscriptionUpdate<T> {
    fn into_json(self) -> SubscriptionUpdateJson {
        SubscriptionUpdateJson {
            table_updates: self.database_update.into(),
            request_id: self.request_id.unwrap_or(0),
            total_host_execution_duration_micros: self.timer.map_or(0, |t| t.elapsed().as_micros() as u64),
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

impl ServerMessage for OneOffQueryResponseMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::OneOffQueryResponse(OneOffQueryResponseJson {
            message_id_base64: base64::engine::general_purpose::STANDARD.encode(self.message_id),
            error: self.error,
            result: self
                .results
                .into_iter()
                .map(|table| OneOffTableJson {
                    table_name: table.head.table_name.clone().into(),
                    rows: table.data,
                })
                .collect(),
        })
    }

    fn serialize_binary(self) -> ws::ServerMessage {
        ws::ServerMessage::OneOffQueryResponse(ws::OneOffQueryResponse {
            message_id: self.message_id,
            error: self.error,
            tables: self
                .results
                .into_iter()
                .map(|table| ws::OneOffTable {
                    table_name: table.head.table_name.clone().into(),
                    rows: table
                        .data
                        .into_iter()
                        .map(|row| bsatn::to_vec(&row).unwrap().into())
                        .collect(),
                })
                .collect(),
            total_host_execution_duration_micros: self.total_host_execution_duration,
        })
    }
}
