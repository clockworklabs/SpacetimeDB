use super::{ClientConfig, DataMessage, Protocol};
use crate::client::consume_each_list::ConsumeEachBuffer;
use crate::host::module_host::{EventStatus, ModuleEvent, ProcedureCallError};
use crate::host::{ArgsTuple, ProcedureCallResult};
use crate::subscription::row_list_builder_pool::BsatnRowListBuilderPool;
use crate::subscription::websocket_building::{brotli_compress, decide_compression, gzip_compress};
use bytes::{BufMut, Bytes, BytesMut};
use bytestring::ByteString;
use derive_more::From;
use spacetimedb_client_api_messages::websocket::common::{self as ws_common, RowListLen as _};
use spacetimedb_client_api_messages::websocket::v1::{self as ws_v1};
use spacetimedb_client_api_messages::websocket::v2 as ws_v2;
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::ser::serde::SerializeWrapper;
use spacetimedb_lib::{AlgebraicValue, ConnectionId, TimeDuration, Timestamp};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::bsatn;
use spacetimedb_schema::table_name::TableName;
use std::sync::Arc;
use std::time::Instant;

/// A server-to-client message which can be encoded according to a [`Protocol`],
/// resulting in a [`ToProtocol::Encoded`] message.
pub trait ToProtocol {
    type Encoded;
    /// Convert `self` into a [`Self::Encoded`] where rows and arguments are encoded with `protocol`.
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded;
}

pub type SwitchedServerMessage =
    ws_v1::FormatSwitch<ws_v1::ServerMessage<ws_v1::BsatnFormat>, ws_v1::ServerMessage<ws_v1::JsonFormat>>;
pub(super) type SwitchedDbUpdate =
    ws_v1::FormatSwitch<ws_v1::DatabaseUpdate<ws_v1::BsatnFormat>, ws_v1::DatabaseUpdate<ws_v1::JsonFormat>>;

/// The initial size of a `serialize` buffer.
/// Currently 4k to align with the linux page size
/// and this should be more than enough in the common case.
const SERIALIZE_BUFFER_INIT_CAP: usize = 4096;

/// A buffer used by [`serialize`]
pub struct SerializeBuffer {
    uncompressed: BytesMut,
    compressed: BytesMut,
}

impl SerializeBuffer {
    pub fn new(config: ClientConfig) -> Self {
        let uncompressed_capacity = SERIALIZE_BUFFER_INIT_CAP;
        let compressed_capacity = if config.compression == ws_v1::Compression::None || config.protocol == Protocol::Text
        {
            0
        } else {
            SERIALIZE_BUFFER_INIT_CAP
        };
        Self {
            uncompressed: BytesMut::with_capacity(uncompressed_capacity),
            compressed: BytesMut::with_capacity(compressed_capacity),
        }
    }

    /// Take the uncompressed message as the one to use.
    fn uncompressed(self) -> (InUseSerializeBuffer, Bytes) {
        let uncompressed = self.uncompressed.freeze();
        let in_use = InUseSerializeBuffer::Uncompressed {
            uncompressed: uncompressed.clone(),
            compressed: self.compressed,
        };
        (in_use, uncompressed)
    }

    /// Write uncompressed data with a leading tag.
    fn write_with_tag<F>(&mut self, tag: u8, write: F) -> &[u8]
    where
        F: FnOnce(bytes::buf::Writer<&mut BytesMut>),
    {
        self.uncompressed.put_u8(tag);
        write((&mut self.uncompressed).writer());
        &self.uncompressed[1..]
    }

    /// Compress the data from a `write_with_tag` call, and change the tag.
    fn compress_with_tag(
        self,
        tag: u8,
        write: impl FnOnce(&[u8], &mut bytes::buf::Writer<BytesMut>),
    ) -> (InUseSerializeBuffer, Bytes) {
        let mut writer = self.compressed.writer();
        writer.get_mut().put_u8(tag);
        write(&self.uncompressed[1..], &mut writer);
        let compressed = writer.into_inner().freeze();
        let in_use = InUseSerializeBuffer::Compressed {
            uncompressed: self.uncompressed,
            compressed: compressed.clone(),
        };
        (in_use, compressed)
    }
}

type BytesMutWriter<'a> = bytes::buf::Writer<&'a mut BytesMut>;

pub enum InUseSerializeBuffer {
    Uncompressed { uncompressed: Bytes, compressed: BytesMut },
    Compressed { uncompressed: BytesMut, compressed: Bytes },
}

impl InUseSerializeBuffer {
    pub fn try_reclaim(self) -> Option<SerializeBuffer> {
        let (mut uncompressed, mut compressed) = match self {
            Self::Uncompressed {
                uncompressed,
                compressed,
            } => (uncompressed.try_into_mut().ok()?, compressed),
            Self::Compressed {
                uncompressed,
                compressed,
            } => (uncompressed, compressed.try_into_mut().ok()?),
        };
        uncompressed.clear();
        compressed.clear();
        Some(SerializeBuffer {
            uncompressed,
            compressed,
        })
    }

    pub fn is_unique(&self) -> bool {
        match self {
            InUseSerializeBuffer::Uncompressed { uncompressed, .. } => uncompressed.is_unique(),
            InUseSerializeBuffer::Compressed { compressed, .. } => compressed.is_unique(),
        }
    }
}

/// Serialize `msg` into a [`DataMessage`] containing a [`ws_v1::ServerMessage`].
///
/// If `protocol` is [`Protocol::Binary`],
/// the message will be conditionally compressed by this method according to `compression`.
pub fn serialize(
    bsatn_rlb_pool: &BsatnRowListBuilderPool,
    mut buffer: SerializeBuffer,
    msg: impl ToProtocol<Encoded = SwitchedServerMessage>,
    config: ClientConfig,
) -> (InUseSerializeBuffer, DataMessage) {
    match msg.to_protocol(config.protocol) {
        ws_v1::FormatSwitch::Json(msg) => {
            let out: BytesMutWriter<'_> = (&mut buffer.uncompressed).writer();
            serde_json::to_writer(out, &SerializeWrapper::new(msg))
                .expect("should be able to json encode a `ServerMessage`");

            let (in_use, out) = buffer.uncompressed();
            // SAFETY: `serde_json::to_writer` states that:
            // > "Serialization guarantees it only feeds valid UTF-8 sequences to the writer."
            let msg_json = unsafe { ByteString::from_bytes_unchecked(out) };
            (in_use, msg_json.into())
        }
        ws_v1::FormatSwitch::Bsatn(msg) => {
            // First write the tag so that we avoid shifting the entire message at the end.
            let srv_msg = buffer.write_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_NONE, |w| {
                bsatn::to_writer(w.into_inner(), &msg).unwrap()
            });

            // At this point, we no longer have a use for `msg`,
            // so try to reclaim its buffers.
            msg.consume_each_list(&mut |buffer| bsatn_rlb_pool.try_put(buffer));

            // Conditionally compress the message.
            let (in_use, msg_bytes) = match decide_compression(srv_msg.len(), config.compression) {
                ws_v1::Compression::None => buffer.uncompressed(),
                ws_v1::Compression::Brotli => {
                    buffer.compress_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_BROTLI, brotli_compress)
                }
                ws_v1::Compression::Gzip => {
                    buffer.compress_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_GZIP, gzip_compress)
                }
            };
            (in_use, msg_bytes.into())
        }
    }
}

/// Serialize `msg` into a [`DataMessage`] containing a [`ws_v2::ServerMessage`].
///
/// This mirrors the v1 framing by prepending the compression tag and applying
/// conditional compression when configured.
pub fn serialize_v2(
    bsatn_rlb_pool: &BsatnRowListBuilderPool,
    mut buffer: SerializeBuffer,
    msg: ws_v2::ServerMessage,
    compression: ws_v1::Compression,
) -> (InUseSerializeBuffer, Bytes) {
    let srv_msg = buffer.write_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_NONE, |w| {
        bsatn::to_writer(w.into_inner(), &msg).expect("should be able to bsatn encode v2 message");
    });

    // At this point, we no longer have a use for `msg`,
    // so try to reclaim its buffers.
    msg.consume_each_list(&mut |buffer| bsatn_rlb_pool.try_put(buffer));

    match decide_compression(srv_msg.len(), compression) {
        ws_v1::Compression::None => buffer.uncompressed(),
        ws_v1::Compression::Brotli => {
            buffer.compress_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_BROTLI, brotli_compress)
        }
        ws_v1::Compression::Gzip => buffer.compress_with_tag(ws_common::SERVER_MSG_COMPRESSION_TAG_GZIP, gzip_compress),
    }
}

#[derive(Debug, From)]
pub enum SerializableMessage {
    QueryBinary(OneOffQueryResponseMessage<ws_v1::BsatnFormat>),
    QueryText(OneOffQueryResponseMessage<ws_v1::JsonFormat>),
    Identity(IdentityTokenMessage),
    Subscribe(SubscriptionUpdateMessage),
    Subscription(SubscriptionMessage),
    TxUpdate(TransactionUpdateMessage),
    ProcedureResult(ProcedureResultMessage),
}

impl SerializableMessage {
    pub fn num_rows(&self) -> Option<usize> {
        match self {
            Self::QueryBinary(msg) => Some(msg.num_rows()),
            Self::QueryText(msg) => Some(msg.num_rows()),
            Self::Subscribe(msg) => Some(msg.num_rows()),
            Self::Subscription(msg) => Some(msg.num_rows()),
            Self::TxUpdate(msg) => Some(msg.num_rows()),
            Self::Identity(_) | Self::ProcedureResult(_) => None,
        }
    }

    pub fn workload(&self) -> Option<WorkloadType> {
        match self {
            Self::QueryBinary(_) | Self::QueryText(_) => Some(WorkloadType::Sql),
            Self::Subscribe(_) => Some(WorkloadType::Subscribe),
            Self::Subscription(msg) => match &msg.result {
                SubscriptionResult::Subscribe(_) => Some(WorkloadType::Subscribe),
                SubscriptionResult::Unsubscribe(_) => Some(WorkloadType::Unsubscribe),
                SubscriptionResult::Error(_) => None,
                SubscriptionResult::SubscribeMulti(_) => Some(WorkloadType::Subscribe),
                SubscriptionResult::UnsubscribeMulti(_) => Some(WorkloadType::Unsubscribe),
            },
            Self::TxUpdate(_) => Some(WorkloadType::Update),
            Self::Identity(_) => None,
            Self::ProcedureResult(_) => Some(WorkloadType::Procedure),
        }
    }
}

impl ToProtocol for SerializableMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        match self {
            SerializableMessage::QueryBinary(msg) => msg.to_protocol(protocol),
            SerializableMessage::QueryText(msg) => msg.to_protocol(protocol),
            SerializableMessage::Identity(msg) => msg.to_protocol(protocol),
            SerializableMessage::Subscribe(msg) => msg.to_protocol(protocol),
            SerializableMessage::TxUpdate(msg) => msg.to_protocol(protocol),
            SerializableMessage::Subscription(msg) => msg.to_protocol(protocol),
            SerializableMessage::ProcedureResult(msg) => msg.to_protocol(protocol),
        }
    }
}

#[derive(Debug, derive_more::From)]
pub enum OutboundMessage {
    V1(SerializableMessage),
    V2(ws_v2::ServerMessage),
}

impl OutboundMessage {
    pub fn num_rows(&self) -> Option<usize> {
        match self {
            Self::V1(message) => message.num_rows(),
            Self::V2(message) => v2_message_num_rows(message),
        }
    }

    pub fn workload(&self) -> Option<WorkloadType> {
        match self {
            Self::V1(message) => message.workload(),
            Self::V2(message) => match message {
                ws_v2::ServerMessage::InitialConnection(_) => None,
                ws_v2::ServerMessage::SubscribeApplied(_) => Some(WorkloadType::Subscribe),
                ws_v2::ServerMessage::UnsubscribeApplied(_) => Some(WorkloadType::Unsubscribe),
                ws_v2::ServerMessage::SubscriptionError(_) => None,
                ws_v2::ServerMessage::TransactionUpdate(_) => Some(WorkloadType::Update),
                ws_v2::ServerMessage::OneOffQueryResult(_) => Some(WorkloadType::Sql),
                ws_v2::ServerMessage::ReducerResult(_) => Some(WorkloadType::Reducer),
                ws_v2::ServerMessage::ProcedureResult(_) => Some(WorkloadType::Procedure),
            },
        }
    }
}

fn v2_message_num_rows(message: &ws_v2::ServerMessage) -> Option<usize> {
    match message {
        ws_v2::ServerMessage::InitialConnection(_) => None,
        ws_v2::ServerMessage::SubscribeApplied(message) => Some(count_query_rows(&message.rows)),
        ws_v2::ServerMessage::UnsubscribeApplied(message) => {
            Some(message.rows.as_ref().map(count_query_rows).unwrap_or_default())
        }
        ws_v2::ServerMessage::SubscriptionError(_) => None,
        ws_v2::ServerMessage::TransactionUpdate(message) => Some(count_transaction_update(message)),
        ws_v2::ServerMessage::OneOffQueryResult(message) => match &message.result {
            Ok(rows) => Some(count_query_rows(rows)),
            Err(_) => None,
        },
        ws_v2::ServerMessage::ReducerResult(message) => match &message.result {
            ws_v2::ReducerOutcome::Ok(ok) => Some(count_transaction_update(&ok.transaction_update)),
            ws_v2::ReducerOutcome::OkEmpty => Some(0),
            ws_v2::ReducerOutcome::Err(_) | ws_v2::ReducerOutcome::InternalError(_) => None,
        },
        ws_v2::ServerMessage::ProcedureResult(_) => None,
    }
}

fn count_query_rows(rows: &ws_v2::QueryRows) -> usize {
    rows.tables.iter().map(|table| table.rows.len()).sum()
}

fn count_transaction_update(update: &ws_v2::TransactionUpdate) -> usize {
    update
        .query_sets
        .iter()
        .flat_map(|qs| qs.tables.iter())
        .map(|table| table.rows.iter().map(count_table_update_rows).sum::<usize>())
        .sum()
}

fn count_table_update_rows(rows: &ws_v2::TableUpdateRows) -> usize {
    match rows {
        ws_v2::TableUpdateRows::PersistentTable(rows) => rows.inserts.len() + rows.deletes.len(),
        ws_v2::TableUpdateRows::EventTable(rows) => rows.events.len(),
    }
}

pub type IdentityTokenMessage = ws_v1::IdentityToken;

impl ToProtocol for IdentityTokenMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        match protocol {
            Protocol::Text => ws_v1::FormatSwitch::Json(ws_v1::ServerMessage::IdentityToken(self)),
            Protocol::Binary => ws_v1::FormatSwitch::Bsatn(ws_v1::ServerMessage::IdentityToken(self)),
        }
    }
}

#[derive(Debug)]
pub struct TransactionUpdateMessage {
    /// The event that caused this update.
    /// When `None`, this is a light update.
    pub event: Option<Arc<ModuleEvent>>,
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
        fn convert<F: ws_v1::WebsocketFormat>(
            event: Option<Arc<ModuleEvent>>,
            request_id: u32,
            update: ws_v1::DatabaseUpdate<F>,
            conv_args: impl FnOnce(&ArgsTuple) -> F::Single,
        ) -> ws_v1::ServerMessage<F> {
            let Some(event) = event else {
                return ws_v1::ServerMessage::TransactionUpdateLight(ws_v1::TransactionUpdateLight {
                    request_id,
                    update,
                });
            };

            let status = match &event.status {
                EventStatus::Committed(_) => ws_v1::UpdateStatus::Committed(update),
                EventStatus::FailedUser(errmsg) | EventStatus::FailedInternal(errmsg) => {
                    ws_v1::UpdateStatus::Failed(errmsg.clone().into())
                }
                EventStatus::OutOfEnergy => ws_v1::UpdateStatus::OutOfEnergy,
            };

            let args = conv_args(&event.function_call.args);

            let tx_update = ws_v1::TransactionUpdate {
                timestamp: event.timestamp,
                status,
                caller_identity: event.caller_identity,
                reducer_call: ws_v1::ReducerCallInfo {
                    reducer_name: event
                        .function_call
                        .reducer
                        .clone()
                        .map(Into::into)
                        .unwrap_or_else(|| "".into()),
                    reducer_id: event.function_call.reducer_id.into(),
                    args,
                    request_id,
                },
                energy_quanta_used: event.energy_quanta_used,
                total_host_execution_duration: event.host_execution_duration.into(),
                caller_connection_id: event.caller_connection_id.unwrap_or(ConnectionId::ZERO),
            };

            ws_v1::ServerMessage::TransactionUpdate(tx_update)
        }

        let TransactionUpdateMessage { event, database_update } = self;
        let update = database_update.database_update;
        protocol.assert_matches_format_switch(&update);
        let mut request_id = database_update.request_id.unwrap_or(0);
        request_id = event.clone().map_or(request_id, |e| e.request_id.unwrap_or(request_id));
        match update {
            ws_v1::FormatSwitch::Bsatn(update) => {
                ws_v1::FormatSwitch::Bsatn(convert(event, request_id, update, |args| {
                    Vec::from(args.get_bsatn().clone()).into()
                }))
            }
            ws_v1::FormatSwitch::Json(update) => {
                ws_v1::FormatSwitch::Json(convert(event, request_id, update, |args| args.get_json().clone()))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdateMessage {
    pub database_update: SwitchedDbUpdate,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
}

impl SubscriptionUpdateMessage {
    pub(crate) fn default_for_protocol(protocol: Protocol, request_id: Option<RequestId>) -> Self {
        Self {
            database_update: match protocol {
                Protocol::Text => ws_v1::FormatSwitch::Json(<_>::default()),
                Protocol::Binary => ws_v1::FormatSwitch::Bsatn(<_>::default()),
            },
            request_id,
            timer: None,
        }
    }

    pub(crate) fn from_event_and_update(event: &ModuleEvent, update: SwitchedDbUpdate) -> Self {
        Self {
            database_update: update,
            request_id: event.request_id,
            timer: event.timer,
        }
    }

    fn num_rows(&self) -> usize {
        match &self.database_update {
            ws_v1::FormatSwitch::Bsatn(x) => x.num_rows(),
            ws_v1::FormatSwitch::Json(x) => x.num_rows(),
        }
    }
}

impl ToProtocol for SubscriptionUpdateMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        let request_id = self.request_id.unwrap_or(0);
        let total_host_execution_duration = self.timer.map_or(TimeDuration::ZERO, |t| t.elapsed().into());

        protocol.assert_matches_format_switch(&self.database_update);
        match self.database_update {
            ws_v1::FormatSwitch::Bsatn(database_update) => {
                ws_v1::FormatSwitch::Bsatn(ws_v1::ServerMessage::InitialSubscription(ws_v1::InitialSubscription {
                    database_update,
                    request_id,
                    total_host_execution_duration,
                }))
            }
            ws_v1::FormatSwitch::Json(database_update) => {
                ws_v1::FormatSwitch::Json(ws_v1::ServerMessage::InitialSubscription(ws_v1::InitialSubscription {
                    database_update,
                    request_id,
                    total_host_execution_duration,
                }))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionData {
    pub data: ws_v1::FormatSwitch<ws_v1::DatabaseUpdate<ws_v1::BsatnFormat>, ws_v1::DatabaseUpdate<ws_v1::JsonFormat>>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionRows {
    pub table_id: TableId,
    pub table_name: TableName,
    pub table_rows: ws_v1::FormatSwitch<ws_v1::TableUpdate<ws_v1::BsatnFormat>, ws_v1::TableUpdate<ws_v1::JsonFormat>>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionError {
    pub table_id: Option<TableId>,
    pub message: Box<str>,
}

#[derive(Debug, Clone)]
pub enum SubscriptionResult {
    Subscribe(SubscriptionRows),
    Unsubscribe(SubscriptionRows),
    Error(SubscriptionError),
    SubscribeMulti(SubscriptionData),
    UnsubscribeMulti(SubscriptionData),
}

#[derive(Debug, Clone)]
pub struct SubscriptionMessage {
    pub timer: Option<Instant>,
    pub request_id: Option<RequestId>,
    pub query_id: Option<ws_v1::QueryId>,
    pub result: SubscriptionResult,
}

fn num_rows_in(rows: &SubscriptionRows) -> usize {
    match &rows.table_rows {
        ws_v1::FormatSwitch::Bsatn(x) => x.num_rows(),
        ws_v1::FormatSwitch::Json(x) => x.num_rows(),
    }
}

fn subscription_data_rows(rows: &SubscriptionData) -> usize {
    match &rows.data {
        ws_v1::FormatSwitch::Bsatn(x) => x.num_rows(),
        ws_v1::FormatSwitch::Json(x) => x.num_rows(),
    }
}

impl SubscriptionMessage {
    fn num_rows(&self) -> usize {
        match &self.result {
            SubscriptionResult::Subscribe(x) => num_rows_in(x),
            SubscriptionResult::SubscribeMulti(x) => subscription_data_rows(x),
            SubscriptionResult::UnsubscribeMulti(x) => subscription_data_rows(x),
            SubscriptionResult::Unsubscribe(x) => num_rows_in(x),
            _ => 0,
        }
    }
}

impl ToProtocol for SubscriptionMessage {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        let request_id = self.request_id.unwrap_or(0);
        let query_id = self.query_id.unwrap_or(ws_v1::QueryId::new(0));
        let total_host_execution_duration_micros = self.timer.map_or(0, |t| t.elapsed().as_micros() as u64);

        match self.result {
            SubscriptionResult::Subscribe(result) => {
                protocol.assert_matches_format_switch(&result.table_rows);
                match result.table_rows {
                    ws_v1::FormatSwitch::Bsatn(table_rows) => ws_v1::FormatSwitch::Bsatn(
                        ws_v1::SubscribeApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            rows: ws_v1::SubscribeRows {
                                table_id: result.table_id,
                                table_name: result.table_name.into(),
                                table_rows,
                            },
                        }
                        .into(),
                    ),
                    ws_v1::FormatSwitch::Json(table_rows) => ws_v1::FormatSwitch::Json(
                        ws_v1::SubscribeApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            rows: ws_v1::SubscribeRows {
                                table_id: result.table_id,
                                table_name: result.table_name.into(),
                                table_rows,
                            },
                        }
                        .into(),
                    ),
                }
            }
            SubscriptionResult::Unsubscribe(result) => {
                protocol.assert_matches_format_switch(&result.table_rows);
                match result.table_rows {
                    ws_v1::FormatSwitch::Bsatn(table_rows) => ws_v1::FormatSwitch::Bsatn(
                        ws_v1::UnsubscribeApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            rows: ws_v1::SubscribeRows {
                                table_id: result.table_id,
                                table_name: result.table_name.into(),
                                table_rows,
                            },
                        }
                        .into(),
                    ),
                    ws_v1::FormatSwitch::Json(table_rows) => ws_v1::FormatSwitch::Json(
                        ws_v1::UnsubscribeApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            rows: ws_v1::SubscribeRows {
                                table_id: result.table_id,
                                table_name: result.table_name.into(),
                                table_rows,
                            },
                        }
                        .into(),
                    ),
                }
            }
            SubscriptionResult::Error(error) => {
                let msg = ws_v1::SubscriptionError {
                    total_host_execution_duration_micros,
                    request_id: self.request_id,           // Pass Option through
                    query_id: self.query_id.map(|x| x.id), // Pass Option through
                    table_id: error.table_id,
                    error: error.message,
                };
                match protocol {
                    Protocol::Binary => ws_v1::FormatSwitch::Bsatn(msg.into()),
                    Protocol::Text => ws_v1::FormatSwitch::Json(msg.into()),
                }
            }
            SubscriptionResult::SubscribeMulti(result) => {
                protocol.assert_matches_format_switch(&result.data);
                match result.data {
                    ws_v1::FormatSwitch::Bsatn(data) => ws_v1::FormatSwitch::Bsatn(
                        ws_v1::SubscribeMultiApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            update: data,
                        }
                        .into(),
                    ),
                    ws_v1::FormatSwitch::Json(data) => ws_v1::FormatSwitch::Json(
                        ws_v1::SubscribeMultiApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            update: data,
                        }
                        .into(),
                    ),
                }
            }
            SubscriptionResult::UnsubscribeMulti(result) => {
                protocol.assert_matches_format_switch(&result.data);
                match result.data {
                    ws_v1::FormatSwitch::Bsatn(data) => ws_v1::FormatSwitch::Bsatn(
                        ws_v1::UnsubscribeMultiApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            update: data,
                        }
                        .into(),
                    ),
                    ws_v1::FormatSwitch::Json(data) => ws_v1::FormatSwitch::Json(
                        ws_v1::UnsubscribeMultiApplied {
                            total_host_execution_duration_micros,
                            request_id,
                            query_id,
                            update: data,
                        }
                        .into(),
                    ),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct OneOffQueryResponseMessage<F: ws_v1::WebsocketFormat> {
    pub message_id: Vec<u8>,
    pub error: Option<String>,
    pub results: Vec<ws_v1::OneOffTable<F>>,
    pub total_host_execution_duration: TimeDuration,
}

impl<F: ws_v1::WebsocketFormat> OneOffQueryResponseMessage<F> {
    fn num_rows(&self) -> usize {
        self.results.iter().map(|table| table.rows.len()).sum()
    }
}

impl ToProtocol for OneOffQueryResponseMessage<ws_v1::BsatnFormat> {
    type Encoded = SwitchedServerMessage;

    fn to_protocol(self, _: Protocol) -> Self::Encoded {
        ws_v1::FormatSwitch::Bsatn(convert(self))
    }
}

impl ToProtocol for OneOffQueryResponseMessage<ws_v1::JsonFormat> {
    type Encoded = SwitchedServerMessage;
    fn to_protocol(self, _: Protocol) -> Self::Encoded {
        ws_v1::FormatSwitch::Json(convert(self))
    }
}

fn convert<F: ws_v1::WebsocketFormat>(msg: OneOffQueryResponseMessage<F>) -> ws_v1::ServerMessage<F> {
    ws_v1::ServerMessage::OneOffQueryResponse(ws_v1::OneOffQueryResponse {
        message_id: msg.message_id.into(),
        error: msg.error.map(Into::into),
        tables: msg.results.into_boxed_slice(),
        total_host_execution_duration: msg.total_host_execution_duration,
    })
}

/// Result of a procedure run.
#[derive(Debug)]
pub enum ProcedureStatus {
    /// The procedure ran to completion and returned this value.
    Returned(AlgebraicValue),
    /// The procedure was terminated due to running out of energy.
    OutOfEnergy,
    /// The procedure failed to run to completion. This string describes the failure.
    InternalError(String),
}

/// Will be sent to the caller of a procedure after that procedure finishes running.
#[derive(Debug)]
pub struct ProcedureResultMessage {
    status: ProcedureStatus,
    timestamp: Timestamp,
    total_host_execution_duration: TimeDuration,
    request_id: u32,
}

impl ProcedureResultMessage {
    pub fn from_result(res: &Result<ProcedureCallResult, ProcedureCallError>, request_id: RequestId) -> Self {
        let (status, timestamp, execution_duration) = match res {
            Ok(ProcedureCallResult {
                return_val,
                execution_duration,
                start_timestamp,
            }) => (
                ProcedureStatus::Returned(return_val.clone()),
                *start_timestamp,
                TimeDuration::from(*execution_duration),
            ),
            Err(err) => (
                match err {
                    ProcedureCallError::OutOfEnergy => ProcedureStatus::OutOfEnergy,
                    _ => ProcedureStatus::InternalError(format!("{err}")),
                },
                Timestamp::UNIX_EPOCH,
                TimeDuration::ZERO,
            ),
        };

        ProcedureResultMessage {
            status,
            timestamp,
            total_host_execution_duration: execution_duration,
            request_id,
        }
    }
}

impl ToProtocol for ProcedureResultMessage {
    type Encoded = SwitchedServerMessage;

    fn to_protocol(self, protocol: Protocol) -> Self::Encoded {
        fn convert<F: ws_v1::WebsocketFormat>(
            msg: ProcedureResultMessage,
            serialize_value: impl Fn(AlgebraicValue) -> F::Single,
        ) -> ws_v1::ServerMessage<F> {
            let ProcedureResultMessage {
                status,
                timestamp,
                total_host_execution_duration,
                request_id,
            } = msg;
            let status = match status {
                ProcedureStatus::InternalError(msg) => ws_v1::ProcedureStatus::InternalError(msg),
                ProcedureStatus::OutOfEnergy => ws_v1::ProcedureStatus::OutOfEnergy,
                ProcedureStatus::Returned(val) => ws_v1::ProcedureStatus::Returned(serialize_value(val)),
            };
            ws_v1::ServerMessage::ProcedureResult(ws_v1::ProcedureResult {
                status,
                timestamp,
                total_host_execution_duration,
                request_id,
            })
        }

        // Note that procedure returns are sent only to the caller, not broadcast to all subscribers,
        // so we don't have to bother with memoizing the serialization the way we do for reducer args.
        match protocol {
            Protocol::Binary => ws_v1::FormatSwitch::Bsatn(convert(self, |val| {
                bsatn::to_vec(&val)
                    .expect("Procedure return value failed to serialize to BSATN")
                    .into()
            })),
            Protocol::Text => ws_v1::FormatSwitch::Json(convert(self, |val| {
                serde_json::to_string(&SerializeWrapper(val))
                    .expect("Procedure return value failed to serialize to JSON")
                    .into()
            })),
        }
    }
}
