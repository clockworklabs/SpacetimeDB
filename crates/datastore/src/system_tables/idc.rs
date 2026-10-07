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

/// Initial next sequence number for a newly-created outbound IDC stream.
pub const INITIAL_OUTBOUND_STREAM_NEXT_SEQ: u64 = 1;
/// Initial ack prefix for a newly-created outbound IDC stream.
pub const INITIAL_OUTBOUND_STREAM_ACK_PREFIX: u64 = 0;

st_fields_enum!(enum StOutboundStreamFields {
    "outbox_table_id", OutboxTableId = 0,
    "target_identity", TargetIdentity = 1,
    "next_seq", NextSeq = 2,
    "ack_prefix", AckPrefix = 3,
});

st_fields_enum!(enum StOutboundMsgFields {
    "outbox_table_id", OutboxTableId = 0,
    "msg_id", MsgId = 1,
    "target_identity", TargetIdentity = 2,
    "seq", Seq = 3,
    "retry_count", RetryCount = 4,
    "last_transport_error", LastTransportError = 5,
    "result_status", ResultStatus = 6,
    "result_payload", ResultPayload = 7,
});

st_fields_enum!(enum StInboundStreamFields {
    "sender_identity", SenderIdentity = 0,
    "sender_outbox_table_id", SenderOutboxTableId = 1,
    "applied_prefix", AppliedPrefix = 2,
});

st_fields_enum!(enum StInboundMsgFields {
    "sender_identity", SenderIdentity = 0,
    "sender_outbox_table_id", SenderOutboxTableId = 1,
    "seq", Seq = 2,
    "result_status", ResultStatus = 3,
    "result_payload", ResultPayload = 4,
});

/// System Table [ST_OUTBOUND_STREAM_NAME]
/// One sender-side stream per (`outbox_table_id`, `target_identity`) pair.
/// Ordered and unordered streams use the same row; only the runtime drain policy differs.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StOutboundStreamRow {
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
    /// Outbox table that contains the user-visible message row.
    pub outbox_table_id: TableId,
    /// Primary key of the user-visible outbox row.
    pub msg_id: u64,
    /// Receiver database identity copied from the outbox row.
    pub target_identity: IdentityViaU256,
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
/// One receiver-side stream per (`sender_identity`, `sender_outbox_table_id`) pair.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StInboundStreamRow {
    /// Sender database identity for this receiver-side stream.
    pub sender_identity: IdentityViaU256,
    /// Sender outbox table that owns this stream.
    pub sender_outbox_table_id: TableId,
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
    /// Sender outbox table that owns this stream.
    pub sender_outbox_table_id: TableId,
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
        .with_unique_constraint(outbound_stream_cols)
        .with_index_no_accessor_name(btree(outbound_stream_cols));

    let outbound_msg_type = builder.add_type::<StOutboundMsgRow>();
    let outbound_msg_cols = [
        StOutboundMsgFields::OutboxTableId.col_id(),
        StOutboundMsgFields::Seq.col_id(),
    ];
    builder
        .build_table(
            ST_OUTBOUND_MSG_NAME,
            *outbound_msg_type.as_ref().expect("system row must be a product"),
        )
        .with_type(TableType::System)
        .with_access(v9::TableAccess::Private)
        .with_unique_constraint(outbound_msg_cols)
        .with_index_no_accessor_name(btree(outbound_msg_cols));

    let inbound_stream_type = builder.add_type::<StInboundStreamRow>();
    let inbound_stream_cols = [
        StInboundStreamFields::SenderIdentity.col_id(),
        StInboundStreamFields::SenderOutboxTableId.col_id(),
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
        StInboundMsgFields::SenderOutboxTableId.col_id(),
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
