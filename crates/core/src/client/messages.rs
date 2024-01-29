use base64::Engine;
use prost::Message as _;

use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::identity::Identity;
use crate::json::client_api::{
    EventJson, FunctionCallJson, IdentityTokenJson, MessageJson, OneOffQueryResponseJson, OneOffTableJson,
    TransactionUpdateJson,
};
use crate::protobuf::client_api::{event, message, Event, FunctionCall, IdentityToken, Message, TransactionUpdate};
use spacetimedb_client_api_messages::client_api::{OneOffQueryResponse, OneOffTable};
use spacetimedb_lib::Address;
use spacetimedb_sats::relation::MemTable;

use super::{DataMessage, Protocol};

/// A message sent from the server to the client. Because clients can request either text or binary messages,
/// a server message needs to be encodable as either.
pub trait ServerMessage: Sized {
    fn serialize(self, protocol: Protocol) -> DataMessage {
        match protocol {
            Protocol::Text => self.serialize_text().to_json().into(),
            Protocol::Binary => self.serialize_binary().encode_to_vec().into(),
        }
    }
    fn serialize_text(self) -> MessageJson;
    fn serialize_binary(self) -> Message;
}

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

pub struct TransactionUpdateMessage<'a> {
    pub event: &'a ModuleEvent,
    pub database_update: DatabaseUpdate,
}

impl ServerMessage for TransactionUpdateMessage<'_> {
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
            }),
            message: errmsg,
            energy_quanta_used: event.energy_quanta_used.get() as i64,
            host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
            caller_address: event.caller_address.unwrap_or(Address::zero()).as_slice().to_vec(),
        };

        let subscription_update = database_update.into_protobuf();

        let tx_update = TransactionUpdate {
            event: Some(event),
            subscription_update: Some(subscription_update),
        };

        Message {
            r#type: Some(message::Type::TransactionUpdate(tx_update)),
        }
    }
}

impl ServerMessage for &mut TransactionUpdateMessage<'_> {
    fn serialize_text(self) -> MessageJson {
        TransactionUpdateMessage {
            event: self.event,
            database_update: self.database_update.clone(),
        }
        .serialize_text()
    }
    fn serialize_binary(self) -> Message {
        TransactionUpdateMessage {
            event: self.event,
            database_update: self.database_update.clone(),
        }
        .serialize_binary()
    }
}

pub struct SubscriptionUpdateMessage {
    pub database_update: DatabaseUpdate,
}

impl ServerMessage for SubscriptionUpdateMessage {
    fn serialize_text(self) -> MessageJson {
        MessageJson::SubscriptionUpdate(self.database_update.into_json())
    }

    fn serialize_binary(self) -> Message {
        Message {
            r#type: Some(message::Type::SubscriptionUpdate(self.database_update.into_protobuf())),
        }
    }
}

pub struct CachedMessage<M> {
    msg: M,
    text: Option<String>,
    binary: Option<Vec<u8>>,
}

impl<M> CachedMessage<M> {
    pub fn new(msg: M) -> Self {
        Self {
            msg,
            text: None,
            binary: None,
        }
    }
}

impl<M> CachedMessage<M>
where
    for<'b> &'b mut M: ServerMessage,
{
    pub fn serialize(&mut self, protocol: Protocol) -> DataMessage {
        match protocol {
            Protocol::Text => self
                .text
                .get_or_insert_with(|| self.msg.serialize_text().to_json())
                .clone()
                .into(),
            Protocol::Binary => self
                .binary
                .get_or_insert_with(|| self.msg.serialize_binary().encode_to_vec())
                .clone()
                .into(),
        }
    }
}

pub struct OneOffQueryResponseMessage {
    pub message_id: Vec<u8>,
    pub error: Option<String>,
    pub results: Vec<MemTable>,
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
                    table_name: table.head.table_name,
                    rows: table.data.into_iter().map(|row| row.data.elements).collect(),
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
                        table_name: table.head.table_name,
                        row: table
                            .data
                            .into_iter()
                            .map(|row| {
                                let mut row_bytes = Vec::new();
                                row.data.encode(&mut row_bytes);
                                row_bytes
                            })
                            .collect(),
                    })
                    .collect(),
            })),
        }
    }
}
