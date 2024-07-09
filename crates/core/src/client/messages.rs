use base64::Engine;
use brotli::CompressorReader;
use derive_more::From;
use prost::Message as _;
use spacetimedb_lib::identity::RequestId;
use std::io::Read;
use std::sync::Arc;
use std::time::Instant;

use crate::execution_context::WorkloadType;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ProtocolDatabaseUpdate};
use crate::identity::Identity;
use crate::json::client_api::{
    EventJson, FunctionCallJson, IdentityTokenJson, MessageJson, OneOffQueryResponseJson, OneOffTableJson,
    SubscriptionUpdateJson, TableUpdateJson, TransactionUpdateJson,
};
use crate::protobuf::client_api::{event, message, Event, FunctionCall, IdentityToken, Message, TransactionUpdate};
use spacetimedb_client_api_messages::client_api::{OneOffQueryResponse, OneOffTable, TableUpdate};
use spacetimedb_lib::Address;
use spacetimedb_vm::relation::MemTable;

use super::message_handlers::MessageExecutionError;
use super::{DataMessage, Protocol};

/// A message sent from the server to the client. Because clients can request either text or binary messages,
/// a server message needs to be encodable as either.
pub trait ServerMessage: Sized {
    fn serialize(self, protocol: Protocol) -> DataMessage {
        match protocol {
            Protocol::Text => self.serialize_text().to_json().into(),
            Protocol::Binary => {
                let msg_bytes = self.serialize_binary().encode_to_vec();
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
    fn serialize_binary(self) -> Message;
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

impl SerializableMessage {
    /// The number of rows in the payload
    pub fn num_rows(&self) -> Option<usize> {
        match self {
            Self::Query(msg) => Some(msg.num_rows()),
            Self::Subscribe(msg) => Some(msg.num_rows()),
            Self::DatabaseUpdate(msg) => Some(msg.num_rows()),
            Self::ProtocolUpdate(msg) => Some(msg.num_rows()),
            _ => None,
        }
    }

    /// The type of workload from which this message originates
    pub fn workload(&self) -> Option<WorkloadType> {
        match self {
            Self::Query(_) => Some(WorkloadType::Sql),
            Self::Subscribe(_) => Some(WorkloadType::Subscribe),
            Self::DatabaseUpdate(_) | Self::ProtocolUpdate(_) => Some(WorkloadType::Update),
            _ => None,
        }
    }
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

    fn serialize_binary(self) -> Message {
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

#[derive(Debug)]
pub struct IdentityTokenMessage {
    pub identity: Identity,
    pub identity_token: String,
    pub address: Address,
}

impl ServerMessage for IdentityTokenMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::IdentityToken(IdentityTokenJson {
            identity: self.identity,
            token: self.identity_token,
            address: self.address,
        })
    }
    fn serialize_binary(self) -> Message {
        Message {
            r#type: Some(message::Type::IdentityToken(IdentityToken {
                identity: self.identity.as_bytes().to_vec(),
                token: self.identity_token,
                address: self.address.as_slice().to_vec(),
            })),
        }
    }
}

#[derive(Debug)]
pub struct TransactionUpdateMessage<U> {
    pub event: Arc<ModuleEvent>,
    pub database_update: SubscriptionUpdate<U>,
}

impl TransactionUpdateMessage<DatabaseUpdate> {
    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.database_update.database_update.num_rows()
    }
}

impl TransactionUpdateMessage<ProtocolDatabaseUpdate> {
    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.database_update.database_update.num_rows()
    }
}

impl<U: Into<Vec<TableUpdate>> + Into<Vec<TableUpdateJson>>> ServerMessage for TransactionUpdateMessage<U> {
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

    fn serialize_binary(self) -> Message {
        let Self { event, database_update } = self;
        let (status, errmsg) = match &event.status {
            EventStatus::Committed(_) => (event::Status::Committed, String::new()),
            EventStatus::Failed(errmsg) => (event::Status::Failed, errmsg.clone()),
            EventStatus::OutOfEnergy => (event::Status::OutOfEnergy, String::new()),
        };

        let event = Event {
            timestamp: event.timestamp.0,
            status: status.into(),
            caller_identity: event.caller_identity.to_vec(),
            function_call: Some(FunctionCall {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.args.get_bsatn().clone().into(),
                request_id: database_update.request_id.unwrap_or(0),
            }),
            message: errmsg,
            energy_quanta_used: event.energy_quanta_used.get() as i64,
            host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
            caller_address: event.caller_address.unwrap_or(Address::zero()).as_slice().to_vec(),
        };

        let tx_update = TransactionUpdate {
            event: Some(event),
            subscription_update: Some(database_update.into_protobuf()),
        };

        Message {
            r#type: Some(message::Type::TransactionUpdate(tx_update)),
        }
    }
}

#[derive(Debug)]
pub struct SubscriptionUpdateMessage {
    pub subscription_update: SubscriptionUpdate<ProtocolDatabaseUpdate>,
}

impl SubscriptionUpdateMessage {
    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.subscription_update.database_update.num_rows()
    }
}

impl ServerMessage for SubscriptionUpdateMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::SubscriptionUpdate(self.subscription_update.into_json())
    }

    fn serialize_binary(self) -> Message {
        let msg = self.subscription_update.into_protobuf();
        let r#type = Some(message::Type::SubscriptionUpdate(msg));
        Message { r#type }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SubscriptionUpdate<U> {
    pub database_update: U,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
}

impl<T: Into<Vec<TableUpdate>>> SubscriptionUpdate<T> {
    fn into_protobuf(self) -> spacetimedb_client_api_messages::client_api::SubscriptionUpdate {
        spacetimedb_client_api_messages::client_api::SubscriptionUpdate {
            table_updates: self.database_update.into(),
            request_id: self.request_id.unwrap_or(0),
            total_host_execution_duration_micros: self.timer.map_or(0, |t| t.elapsed().as_micros() as u64),
        }
    }
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

impl OneOffQueryResponseMessage {
    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.results.iter().map(|t| t.data.len()).sum()
    }
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

    fn serialize_binary(self) -> Message {
        Message {
            r#type: Some(message::Type::OneOffQueryResponse(OneOffQueryResponse {
                message_id: self.message_id,
                error: self.error.unwrap_or_default(),
                tables: self
                    .results
                    .into_iter()
                    .map(|table| OneOffTable {
                        table_name: table.head.table_name.clone().into(),
                        row: table
                            .data
                            .into_iter()
                            .map(|row| {
                                let mut row_bytes = Vec::new();
                                row.encode(&mut row_bytes);
                                row_bytes
                            })
                            .collect(),
                    })
                    .collect(),
                total_host_execution_duration_micros: self.total_host_execution_duration,
            })),
        }
    }
}
