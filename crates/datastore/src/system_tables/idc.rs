use super::*;
use bytes::Bytes;

pub const ST_OUTBOUND_STREAM_ID: TableId = TableId(22);
pub const ST_OUTBOUND_STREAM_NAME: &str = "st_outbound_stream";
pub const ST_OUTBOUND_MSG_ID: TableId = TableId(23);
pub const ST_OUTBOUND_MSG_NAME: &str = "st_outbound_msg";

pub const ST_INBOUND_STREAM_ID: TableId = TableId(24);
pub const ST_INBOUND_STREAM_NAME: &str = "st_inbound_stream";
pub const ST_INBOUND_MSG_ID: TableId = TableId(25);
pub const ST_INBOUND_MSG_NAME: &str = "st_inbound_msg";
pub const ST_OUTBOX_ID: TableId = TableId(26);
pub const ST_OUTBOX_NAME: &str = "st_outbox";

/// Initial next sequence number for a newly-created outbound IDC stream.
pub const INITIAL_OUTBOUND_STREAM_NEXT_SEQ: u64 = 1;
/// Initial ack prefix for a newly-created outbound IDC stream.
pub const INITIAL_OUTBOUND_STREAM_ACK_PREFIX: u64 = 0;

st_fields_enum!(enum StOutboundStreamFields {
    "stream_id", StreamId = 0,
    "outbox_table_id", OutboxTableId = 1,
    "target_identity", TargetIdentity = 2,
    "next_seq", NextSeq = 3,
    "ack_prefix", AckPrefix = 4,
});

st_fields_enum!(enum StOutboundMsgFields {
    "stream_id", StreamId = 0,
    "msg_id", MsgId = 1,
    "seq", Seq = 2,
    "retry_count", RetryCount = 3,
    "last_transport_error", LastTransportError = 4,
    "result_status", ResultStatus = 5,
    "result_payload", ResultPayload = 6,
});

st_fields_enum!(enum StInboundStreamFields {
    "sender_identity", SenderIdentity = 0,
    "stream_id", StreamId = 1,
    "applied_prefix", AppliedPrefix = 2,
});

st_fields_enum!(enum StInboundMsgFields {
    "sender_identity", SenderIdentity = 0,
    "stream_id", StreamId = 1,
    "seq", Seq = 2,
    "result_status", ResultStatus = 3,
    "result_payload", ResultPayload = 4,
});

st_fields_enum!(enum StOutboxFields {
    "table_id", TableId = 0,
    "remote_reducer", RemoteReducer = 1,
    "target_column", TargetColumn = 2,
    "arg_columns", ArgColumns = 3,
    "on_result_reducer", OnResultReducer = 4,
    "signature_hash", SignatureHash = 5,
});

/// System Table [ST_OUTBOUND_STREAM_NAME]
/// One sender-side stream per (`outbox_table_id`, `target_identity`) pair.
/// Ordered and unordered streams use the same row; only the runtime drain policy differs.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StOutboundStreamRow {
    /// Stable sender-owned stream id used in outbound rows and on the receiver.
    pub stream_id: u64,
    /// Outbox table that owns this sender-side stream.
    pub outbox_table_id: TableId,
    /// Receiver database identity for this stream.
    pub target_identity: IdentityViaU256,
    /// Next dense sequence number to assign to a newly inserted outbox row.
    /// Initial value: `INITIAL_OUTBOUND_STREAM_NEXT_SEQ`.
    pub next_seq: u64,
    /// Highest contiguous result sequence whose on_result has run locally.
    /// Initial value: `INITIAL_OUTBOUND_STREAM_ACK_PREFIX`.
    /// Sent back to the receiver so it can drop older inbound result rows.
    pub ack_prefix: u64,
}

impl TryFrom<RowRef<'_>> for StOutboundStreamRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StOutboundStreamRow> for ProductValue {
    fn from(row: StOutboundStreamRow) -> Self {
        to_product_value(&row)
    }
}

/// System Table [ST_OUTBOUND_MSG_NAME]
/// One row per outbound call until delivery, result handling, and cleanup are done.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StOutboundMsgRow {
    /// Sender-owned stream id from `st_outbound_stream`.
    pub stream_id: u64,
    /// Primary key of the user-visible outbox row.
    pub msg_id: u64,
    /// Dense stream sequence number assigned from `st_outbound_stream.next_seq`.
    pub seq: u64,
    /// Number of failed delivery attempts for retry/backoff.
    pub retry_count: u32,
    /// Last transport-layer error observed while delivering this message.
    pub last_transport_error: Option<String>,
    /// Result status received from the target. Ordered streams buffer it until earlier results run.
    pub result_status: Option<StInboundMsgResultStatus>,
    /// Result payload received from the target, paired with `result_status`.
    pub result_payload: Option<Bytes>,
}

impl TryFrom<RowRef<'_>> for StOutboundMsgRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StOutboundMsgRow> for ProductValue {
    fn from(row: StOutboundMsgRow) -> Self {
        to_product_value(&row)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StInboundMsgResultStatus {
    Ok = 1,
    Err = 2,
}

impl TryFrom<u8> for StInboundMsgResultStatus {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Ok),
            2 => Ok(Self::Err),
            _ => Err("invalid st_inbound_msg result status"),
        }
    }
}

impl From<StInboundMsgResultStatus> for u8 {
    fn from(value: StInboundMsgResultStatus) -> Self {
        value as u8
    }
}

impl_st!([] StInboundMsgResultStatus, AlgebraicType::U8);
impl<'de> Deserialize<'de> for StInboundMsgResultStatus {
    fn deserialize<D: spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}
impl_serialize!([] StInboundMsgResultStatus, (self, ser) => u8::from(*self).serialize(ser));

/// System Table [ST_INBOUND_STREAM_NAME]
/// One receiver-side stream per (`sender_identity`, `stream_id`) pair.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StInboundStreamRow {
    /// Sender database identity for this receiver-side stream.
    pub sender_identity: IdentityViaU256,
    /// Stable sender-owned stream id from `st_outbound_stream`.
    pub stream_id: u64,
    /// Highest contiguous sequence applied by this receiver.
    /// In ordered mode, the next new message must be this value plus one.
    /// In unordered mode, gaps may be accepted, but this prefix only moves when contiguous.
    pub applied_prefix: u64,
}

impl TryFrom<RowRef<'_>> for StInboundStreamRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StInboundStreamRow> for ProductValue {
    fn from(row: StInboundStreamRow) -> Self {
        to_product_value(&row)
    }
}

/// System Table [ST_INBOUND_MSG_NAME]
/// Retained inbound results for replaying duplicate deliveries until the sender acks them.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StInboundMsgRow {
    /// Sender database identity for this retained result.
    pub sender_identity: IdentityViaU256,
    /// Stable sender-owned stream id from `st_outbound_stream`.
    pub stream_id: u64,
    /// Dense stream sequence number this result belongs to.
    pub seq: u64,
    /// Stored reducer outcome kind to replay if the sender retries this sequence.
    pub result_status: StInboundMsgResultStatus,
    /// Stored reducer return value or error payload for that replay.
    pub result_payload: Bytes,
}

impl TryFrom<RowRef<'_>> for StInboundMsgRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StInboundMsgRow> for ProductValue {
    fn from(row: StInboundMsgRow) -> Self {
        to_product_value(&row)
    }
}

/// System Table [ST_OUTBOX_NAME]
/// IDC outbox metadata for user tables.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StOutboxRow {
    /// User table this outbox metadata belongs to.
    pub table_id: TableId,
    /// Reducer to invoke on the receiver database.
    pub remote_reducer: NamespacedIdentifier,
    /// Column containing the receiver database identity.
    pub target_column: ColId,
    /// Columns to encode as reducer arguments.
    pub arg_columns: Vec<ColId>,
    /// Local reducer to invoke with the remote result.
    pub on_result_reducer: Option<NamespacedIdentifier>,
    /// Receiver reducer signature hash as seen by sender bindings.
    pub signature_hash: spacetimedb_lib::Hash,
}

impl TryFrom<RowRef<'_>> for StOutboxRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StOutboxRow> for ProductValue {
    fn from(row: StOutboxRow) -> Self {
        to_product_value(&row)
    }
}

impl From<(TableId, OutboxSchema)> for StOutboxRow {
    fn from((table_id, outbox): (TableId, OutboxSchema)) -> Self {
        Self {
            table_id,
            remote_reducer: outbox.remote_reducer.into(),
            target_column: outbox.target_column,
            arg_columns: outbox.arg_columns,
            on_result_reducer: outbox.on_result_reducer.map(Into::into),
            signature_hash: outbox.signature_hash,
        }
    }
}

impl From<StOutboxRow> for OutboxSchema {
    fn from(row: StOutboxRow) -> Self {
        Self {
            remote_reducer: ReducerName::new(row.remote_reducer),
            target_column: row.target_column,
            arg_columns: row.arg_columns,
            on_result_reducer: row.on_result_reducer.map(ReducerName::new),
            signature_hash: row.signature_hash,
        }
    }
}

pub(super) fn register_tables(builder: &mut RawModuleDefV9Builder) {
    let outbound_stream_type = builder.add_type::<StOutboundStreamRow>();
    let outbound_stream_cols = [
        StOutboundStreamFields::OutboxTableId.col_id(),
        StOutboundStreamFields::TargetIdentity.col_id(),
    ];
    builder
        .build_table(
            ST_OUTBOUND_STREAM_NAME,
            *outbound_stream_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_auto_inc_primary_key(StOutboundStreamFields::StreamId)
        .with_unique_constraint(outbound_stream_cols)
        .with_index_no_accessor_name(btree(StOutboundStreamFields::StreamId))
        .with_index_no_accessor_name(btree(outbound_stream_cols));

    let outbound_msg_type = builder.add_type::<StOutboundMsgRow>();
    let outbound_msg_stream_seq_cols = [
        StOutboundMsgFields::StreamId.col_id(),
        StOutboundMsgFields::Seq.col_id(),
    ];
    let outbound_msg_stream_msg_cols = [
        StOutboundMsgFields::StreamId.col_id(),
        StOutboundMsgFields::MsgId.col_id(),
    ];
    builder
        .build_table(
            ST_OUTBOUND_MSG_NAME,
            *outbound_msg_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_unique_constraint(outbound_msg_stream_seq_cols)
        .with_unique_constraint(outbound_msg_stream_msg_cols)
        .with_index_no_accessor_name(btree(outbound_msg_stream_seq_cols))
        .with_index_no_accessor_name(btree(outbound_msg_stream_msg_cols));

    let inbound_stream_type = builder.add_type::<StInboundStreamRow>();
    let inbound_stream_cols = [
        StInboundStreamFields::SenderIdentity.col_id(),
        StInboundStreamFields::StreamId.col_id(),
    ];
    builder
        .build_table(
            ST_INBOUND_STREAM_NAME,
            *inbound_stream_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_unique_constraint(inbound_stream_cols)
        .with_index_no_accessor_name(btree(inbound_stream_cols));

    let inbound_msg_type = builder.add_type::<StInboundMsgRow>();
    let inbound_msg_cols = [
        StInboundMsgFields::SenderIdentity.col_id(),
        StInboundMsgFields::StreamId.col_id(),
        StInboundMsgFields::Seq.col_id(),
    ];
    builder
        .build_table(
            ST_INBOUND_MSG_NAME,
            *inbound_msg_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_unique_constraint(inbound_msg_cols)
        .with_index_no_accessor_name(btree(inbound_msg_cols));

    let outbox_type = builder.add_type::<StOutboxRow>();
    builder
        .build_table(
            ST_OUTBOX_NAME,
            *outbox_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_primary_key(StOutboxFields::TableId)
        .with_unique_constraint(StOutboxFields::TableId)
        .with_index_no_accessor_name(btree(StOutboxFields::TableId));
}

pub(crate) fn st_outbound_stream_schema() -> TableSchema {
    st_schema(ST_OUTBOUND_STREAM_NAME, ST_OUTBOUND_STREAM_ID)
}

pub(crate) fn st_outbound_msg_schema() -> TableSchema {
    st_schema(ST_OUTBOUND_MSG_NAME, ST_OUTBOUND_MSG_ID)
}

pub(crate) fn st_inbound_stream_schema() -> TableSchema {
    st_schema(ST_INBOUND_STREAM_NAME, ST_INBOUND_STREAM_ID)
}

pub(crate) fn st_inbound_msg_schema() -> TableSchema {
    st_schema(ST_INBOUND_MSG_NAME, ST_INBOUND_MSG_ID)
}

pub(crate) fn st_outbox_schema() -> TableSchema {
    st_schema(ST_OUTBOX_NAME, ST_OUTBOX_ID)
}
