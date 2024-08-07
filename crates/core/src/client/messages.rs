use brotli::CompressorReader;
use derive_more::From;
use flate2::write::GzEncoder;
use flate2::Compression;
use spacetimedb_client_api_messages::websocket::EncodedValue;
use spacetimedb_lib::bsatn::ser::BsatnError;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::ser::Serialize;
use spacetimedb_table::table::RowRef;
use spacetimedb_vm::relation::{MemTable, RelValue};
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Instant;

use crate::execution_context::WorkloadType;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::host::ArgsTuple;
use crate::messages::websocket as ws;
use spacetimedb_lib::{bsatn, Address, ProductValue};

use super::message_handlers::MessageExecutionError;
use super::{DataMessage, Protocol, ProtocolCodec};

fn encode_gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder
        .finish()
        .expect("Failed to Gz compress `SubscriptionUpdateMessage`")
}

fn encode_brotli(bytes: &[u8]) -> Vec<u8> {
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

    let reader = &mut &bytes[..];
    let mut encoder = CompressorReader::new(reader, BUFFER_SIZE, COMPRESSION_LEVEL, LG_WIN);

    let mut out = Vec::new();
    encoder
        .read_to_end(&mut out)
        .expect("Failed to Brotli compress `SubscriptionUpdateMessage");
    out
}

/// A server-to-client message which can be encoded according to a [`Protocol`],
/// resulting in a [`ToProtocol::Encoded`] message.
pub trait ToProtocol {
    type Encoded;
    /// Convert `self` into a [`Self::Encoded`] where rows and arguments are encoded with `protocol`.
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded;
}

/// Serialize `msg` into a [`DataMessage`] containing a [`ws::ServerMessage`].
///
/// If `protocol` is [`Protocol::Binary`], the message will be compressed by this method.
pub fn serialize(
    msg: impl ToProtocol<Encoded = ws::ServerMessage>,
    protocol: Protocol,
    codec: ProtocolCodec,
) -> DataMessage {
    let msg = msg.to_protocol(protocol);
    match protocol {
        Protocol::Text => serde_json::to_string(&SerializeWrapper::new(msg)).unwrap().into(),
        Protocol::Binary => {
            let msg_bytes = bsatn::to_vec(&msg).unwrap();
            match codec {
                ProtocolCodec::None => msg_bytes.into(),
                ProtocolCodec::Gzip => encode_gzip(&msg_bytes).into(),
                ProtocolCodec::Brotli => encode_brotli(&msg_bytes).into(),
            }
        }
    }
}

#[derive(Debug, From)]
pub enum SerializableMessage {
    Query(OneOffQueryResponseMessage),
    Error(MessageExecutionError),
    Identity(IdentityTokenMessage),
    Subscribe(SubscriptionUpdateMessage),
    DatabaseUpdate(TransactionUpdateMessage<DatabaseUpdate>),
    ProtocolUpdate(TransactionUpdateMessage<ws::DatabaseUpdate>),
}

impl ToProtocol for ws::DatabaseUpdate {
    type Encoded = ws::DatabaseUpdate;
    fn to_protocol(self, _: Protocol) -> Self::Encoded {
        self
    }
}

impl SerializableMessage {
    pub fn num_rows(&self) -> Option<usize> {
        match self {
            Self::Query(msg) => Some(msg.num_rows()),
            Self::Subscribe(msg) => Some(msg.num_rows()),
            Self::DatabaseUpdate(msg) => Some(msg.num_rows()),
            Self::ProtocolUpdate(msg) => Some(msg.num_rows()),
            _ => None,
        }
    }

    pub fn workload(&self) -> Option<WorkloadType> {
        match self {
            Self::Query(_) => Some(WorkloadType::Sql),
            Self::Subscribe(_) => Some(WorkloadType::Subscribe),
            Self::DatabaseUpdate(_) | Self::ProtocolUpdate(_) => Some(WorkloadType::Update),
            _ => None,
        }
    }
}

impl ToProtocol for SerializableMessage {
    type Encoded = ws::ServerMessage;
    fn to_protocol(self, protocol: Protocol) -> ws::ServerMessage {
        match self {
            SerializableMessage::Query(msg) => msg.to_protocol(protocol),
            SerializableMessage::Error(msg) => msg.to_protocol(protocol),
            SerializableMessage::Identity(msg) => msg.to_protocol(protocol),
            SerializableMessage::Subscribe(msg) => msg.to_protocol(protocol),
            SerializableMessage::DatabaseUpdate(msg) => msg.to_protocol(protocol),
            SerializableMessage::ProtocolUpdate(msg) => msg.to_protocol(protocol),
        }
    }
}

pub type IdentityTokenMessage = ws::IdentityToken;

impl ToProtocol for IdentityTokenMessage {
    type Encoded = ws::ServerMessage;
    fn to_protocol(self, _: Protocol) -> ws::ServerMessage {
        ws::ServerMessage::IdentityToken(self)
    }
}

#[derive(Debug)]
pub struct TransactionUpdateMessage<U> {
    pub event: Arc<ModuleEvent>,
    pub database_update: SubscriptionUpdate<U>,
}

impl TransactionUpdateMessage<DatabaseUpdate> {
    fn num_rows(&self) -> usize {
        self.database_update.database_update.num_rows()
    }
}

impl TransactionUpdateMessage<ws::DatabaseUpdate> {
    fn num_rows(&self) -> usize {
        self.database_update.database_update.num_rows()
    }
}

/// Types that can be encoded to BSATN.
///
/// Implementations of this trait may be more efficient than directly calling [`bsatn::to_vec`].
/// In particular, for [`RowRef`], this method will use a [`StaticBsatnLayout`] if one is available,
/// avoiding expensive runtime type dispatch.
pub trait ToBsatn {
    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError>;

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError>;
}

impl ToBsatn for RowRef<'_> {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        self.to_bsatn_vec()
    }

    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        self.to_bsatn_extend(buf)
    }
}

impl ToBsatn for ProductValue {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        bsatn::to_vec(self)
    }

    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        bsatn::to_writer(buf, self)
    }
}

impl ToBsatn for RelValue<'_> {
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        match self {
            RelValue::Row(this) => this.to_bsatn_vec(),
            RelValue::Projection(this) => this.to_bsatn_vec(),
            RelValue::ProjRef(this) => this.to_bsatn_vec(),
        }
    }
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        match self {
            RelValue::Row(this) => this.to_bsatn_extend(buf),
            RelValue::Projection(this) => this.to_bsatn_extend(buf),
            RelValue::ProjRef(this) => this.to_bsatn_extend(buf),
        }
    }
}

// TODO(perf): Revise WS schema for BSATN to store a single concatenated `Bytes` as `inserts`/`deletes`,
// rather than a separate `Bytes` for each row.
// Possibly JSON stores a single `ByteString` list of rows, or just concatenates them the same.
// Revise this function to append into a given sink.
pub(crate) fn encode_row<Row: Serialize + ToBsatn>(row: &Row, protocol: Protocol) -> EncodedValue {
    match protocol {
        Protocol::Binary => EncodedValue::Binary(row.to_bsatn_vec().unwrap().into()),
        Protocol::Text => EncodedValue::Text(serde_json::to_string(&SerializeWrapper::new(row)).unwrap().into()),
    }
}

impl ToProtocol for ArgsTuple {
    type Encoded = EncodedValue;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        match protocol {
            Protocol::Binary => EncodedValue::Binary(self.get_bsatn().clone()),
            Protocol::Text => EncodedValue::Text(self.get_json().clone()),
        }
    }
}

impl<U: ToProtocol<Encoded = ws::DatabaseUpdate>> ToProtocol for TransactionUpdateMessage<U> {
    type Encoded = ws::ServerMessage;

    fn to_protocol(self, protocol: Protocol) -> ws::ServerMessage {
        let Self { event, database_update } = self;
        let status = match &event.status {
            EventStatus::Committed(_) => {
                ws::UpdateStatus::Committed(database_update.database_update.to_protocol(protocol))
            }
            EventStatus::Failed(errmsg) => ws::UpdateStatus::Failed(errmsg.clone()),
            EventStatus::OutOfEnergy => ws::UpdateStatus::OutOfEnergy,
        };

        let args = event.function_call.args.clone().to_protocol(protocol);

        let tx_update = ws::TransactionUpdate {
            timestamp: event.timestamp,
            status,
            caller_identity: event.caller_identity,
            reducer_call: ws::ReducerCallInfo {
                reducer_name: event.function_call.reducer.to_owned(),
                reducer_id: event.function_call.reducer_id.into(),
                args,
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
    pub subscription_update: SubscriptionUpdate<ws::DatabaseUpdate>,
}

impl SubscriptionUpdateMessage {
    fn num_rows(&self) -> usize {
        self.subscription_update.database_update.num_rows()
    }
}

impl ToProtocol for SubscriptionUpdateMessage {
    type Encoded = ws::ServerMessage;
    fn to_protocol(self, protocol: Protocol) -> ws::ServerMessage {
        let upd = self.subscription_update;
        ws::ServerMessage::InitialSubscription(ws::InitialSubscription {
            database_update: upd.database_update.to_protocol(protocol),
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

fn memtable_to_protocol(table: MemTable, protocol: Protocol) -> ws::OneOffTable {
    ws::OneOffTable {
        table_name: table.head.table_name.clone().into(),
        rows: table.data.into_iter().map(|row| encode_row(&row, protocol)).collect(),
    }
}

impl ToProtocol for OneOffQueryResponseMessage {
    type Encoded = ws::ServerMessage;

    fn to_protocol(self, protocol: Protocol) -> ws::ServerMessage {
        ws::ServerMessage::OneOffQueryResponse(ws::OneOffQueryResponse {
            message_id: self.message_id,
            error: self.error,
            tables: self
                .results
                .into_iter()
                .map(|table| memtable_to_protocol(table, protocol))
                .collect(),
            total_host_execution_duration_micros: self.total_host_execution_duration,
        })
    }
}
