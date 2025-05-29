//! Messages sent over the SpacetimeDB WebSocket protocol.
//!
//! Client -> Server messages are encoded as [`ClientMessage`].
//! Server -> Client messages are encoded as [`ServerMessage`].
//!
//! Any changes to this file must be paired with a change to the WebSocket protocol identifiers
//! defined in `crates/client-api/src/routes/subscribe.rs`,
//! and be paired with changes to all of:
//!
//! - The C# SDK.
//! - The TypeScript SDK.
//! - The SpacetimeDB website.
//!
//! Changes to the Rust SDK are not necessarily required, as it depends on this crate
//! rather than using an external mirror of this schema.

use crate::energy::EnergyQuanta;
use bytes::Bytes;
use bytestring::ByteString;
use core::{
    fmt::Debug,
    ops::{Deref, Range},
};
use enum_as_inner::EnumAsInner;
use smallvec::SmallVec;
use spacetimedb_lib::{ConnectionId, Identity, TimeDuration, Timestamp};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{
    bsatn::{self, ToBsatn},
    de::{Deserialize, Error},
    impl_deserialize, impl_serialize, impl_st,
    ser::{serde::SerializeWrapper, Serialize},
    AlgebraicType, SpacetimeType,
};
use std::{
    io::{self, Read as _, Write as _},
    sync::Arc,
};

pub const TEXT_PROTOCOL: &str = "v1.json.spacetimedb";
pub const BIN_PROTOCOL: &str = "v1.bsatn.spacetimedb";

pub trait RowListLen {
    /// Returns the length of the list.
    fn len(&self) -> usize;
    /// Returns whether the list is empty or not.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T, L: Deref<Target = [T]>> RowListLen for L {
    fn len(&self) -> usize {
        self.deref().len()
    }
    fn is_empty(&self) -> bool {
        self.deref().is_empty()
    }
}

pub trait ByteListLen {
    /// Returns the uncompressed size of the list in bytes
    fn num_bytes(&self) -> usize;
}

impl ByteListLen for Vec<ByteString> {
    fn num_bytes(&self) -> usize {
        self.iter().map(|str| str.len()).sum()
    }
}

/// A format / codec used by the websocket API.
///
/// This can be e.g., BSATN, JSON.
pub trait WebsocketFormat: Sized {
    /// The type used for the encoding of a single item.
    type Single: SpacetimeType + for<'de> Deserialize<'de> + Serialize + Debug + Clone;

    /// The type used for the encoding of a list of items.
    type List: SpacetimeType
        + for<'de> Deserialize<'de>
        + Serialize
        + RowListLen
        + ByteListLen
        + Debug
        + Clone
        + Default;

    /// Encodes the `elems` to a list in the format and also returns the length of the list.
    fn encode_list<R: ToBsatn + Serialize>(elems: impl Iterator<Item = R>) -> (Self::List, u64);

    /// The type used to encode query updates.
    /// This type exists so that some formats, e.g., BSATN, can compress an update.
    type QueryUpdate: SpacetimeType + for<'de> Deserialize<'de> + Serialize + Debug + Clone + Send;

    /// Convert a `QueryUpdate` into `Self::QueryUpdate`.
    /// This allows some formats to e.g., compress the update.
    fn into_query_update(qu: QueryUpdate<Self>, compression: Compression) -> Self::QueryUpdate;
}

/// Messages sent from the client to the server.
///
/// Parametric over the reducer argument type to enable [`ClientMessage::map_args`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ClientMessage<Args> {
    /// Request a reducer run.
    CallReducer(CallReducer<Args>),
    /// Register SQL queries on which to receive updates.
    Subscribe(Subscribe),
    /// Send a one-off SQL query without establishing a subscription.
    OneOffQuery(OneOffQuery),
    /// Register a SQL query to to subscribe to updates. This does not affect other subscriptions.
    SubscribeSingle(SubscribeSingle),
    SubscribeMulti(SubscribeMulti),
    /// Remove a subscription to a SQL query that was added with SubscribeSingle.
    Unsubscribe(Unsubscribe),
    UnsubscribeMulti(UnsubscribeMulti),
}

impl<Args> ClientMessage<Args> {
    pub fn map_args<Args2>(self, f: impl FnOnce(Args) -> Args2) -> ClientMessage<Args2> {
        match self {
            ClientMessage::CallReducer(CallReducer {
                reducer,
                args,
                request_id,
                flags,
            }) => ClientMessage::CallReducer(CallReducer {
                reducer,
                args: f(args),
                request_id,
                flags,
            }),
            ClientMessage::OneOffQuery(x) => ClientMessage::OneOffQuery(x),
            ClientMessage::SubscribeSingle(x) => ClientMessage::SubscribeSingle(x),
            ClientMessage::Unsubscribe(x) => ClientMessage::Unsubscribe(x),
            ClientMessage::Subscribe(x) => ClientMessage::Subscribe(x),
            ClientMessage::SubscribeMulti(x) => ClientMessage::SubscribeMulti(x),
            ClientMessage::UnsubscribeMulti(x) => ClientMessage::UnsubscribeMulti(x),
        }
    }
}

/// Request a reducer run.
///
/// Parametric over the argument type to enable [`ClientMessage::map_args`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct CallReducer<Args> {
    /// The name of the reducer to call.
    pub reducer: Box<str>,
    /// The arguments to the reducer.
    ///
    /// In the wire format, this will be a [`Bytes`], BSATN or JSON encoded according to the reducer's argument schema
    /// and the enclosing message format.
    pub args: Args,
    /// An identifier for a client request.
    ///
    /// The server will include the same ID in the response [`TransactionUpdate`].
    pub request_id: u32,
    /// Assorted flags that can be passed when calling a reducer.
    ///
    /// Currently accepts 0 or 1 where the latter means
    /// that the caller does not want to be notified about the reducer
    /// without being subscribed to any relevant queries.
    pub flags: CallReducerFlags,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CallReducerFlags {
    /// The reducer's caller does want to be notified about the reducer completing successfully
    /// regardless of whether the caller had subscribed to a relevant query.
    ///
    /// Note that updates to a reducer's caller are always sent as full updates
    /// whether subscribed to a relevant query or not.
    /// That is, the light tx mode setting does not apply to the reducer's caller.
    ///
    /// This is the default flag.
    #[default]
    FullUpdate,
    /// The reducer's caller does not want to be notified about the reducer completing successfully
    /// without having subscribed to any of the relevant queries.
    NoSuccessNotify,
}

impl_st!([] CallReducerFlags, AlgebraicType::U8);
impl_serialize!([] CallReducerFlags, (self, ser) => ser.serialize_u8(*self as u8));
impl_deserialize!([] CallReducerFlags, de => match de.deserialize_u8()? {
    0 => Ok(Self::FullUpdate),
    1 => Ok(Self::NoSuccessNotify),
    x => Err(D::Error::custom(format_args!("invalid call reducer flag {x}"))),
});

/// An opaque id generated by the client to refer to a subscription.
/// This is used in Unsubscribe messages and errors.
#[derive(SpacetimeType, Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[sats(crate = spacetimedb_lib)]
pub struct QueryId {
    pub id: u32,
}

impl QueryId {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

/// Sent by client to database to register a set of queries, about which the client will
/// receive `TransactionUpdate`s.
///
/// After issuing a `Subscribe` message, the client will receive a single
/// `SubscriptionUpdate` message containing every current row of every table which matches
/// the subscribed queries. Then, after each reducer run which updates one or more
/// subscribed rows, the client will receive a `TransactionUpdate` containing the updates.
///
/// A `Subscribe` message sets or replaces the entire set of queries to which the client
/// is subscribed. If the client is previously subscribed to some set of queries `A`, and
/// then sends a `Subscribe` message to subscribe to a set `B`, afterwards, the client
/// will be subscribed to `B` but not `A`. In this case, the client will receive a
/// `SubscriptionUpdate` containing every existing row that matches `B`, even if some were
/// already in `A`.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct Subscribe {
    /// A sequence of SQL queries.
    pub query_strings: Box<[Box<str>]>,
    pub request_id: u32,
}

/// Sent by client to register a subscription to single query, for which the client should receive
/// receive relevant `TransactionUpdate`s.
///
/// After issuing a `SubscribeSingle` message, the client will receive a single
/// `SubscribeApplied` message containing every current row which matches the query. Then, any
/// time a reducer updates the query's results, the client will receive a `TransactionUpdate`
/// containing the relevant updates.
///
/// If a client subscribes to queries with overlapping results, the client will receive
/// multiple copies of rows that appear in multiple queries.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeSingle {
    /// A single SQL `SELECT` query to subscribe to.
    pub query: Box<str>,
    /// An identifier for a client request.
    pub request_id: u32,

    /// An identifier for this subscription, which should not be used for any other subscriptions on the same connection.
    /// This is used to refer to this subscription in Unsubscribe messages from the client and errors sent from the server.
    /// These only have meaning given a ConnectionId.
    pub query_id: QueryId,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeMulti {
    /// A single SQL `SELECT` query to subscribe to.
    pub query_strings: Box<[Box<str>]>,
    /// An identifier for a client request.
    pub request_id: u32,

    /// An identifier for this subscription, which should not be used for any other subscriptions on the same connection.
    /// This is used to refer to this subscription in Unsubscribe messages from the client and errors sent from the server.
    /// These only have meaning given a ConnectionId.
    pub query_id: QueryId,
}

/// Client request for removing a query from a subscription.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct Unsubscribe {
    /// An identifier for a client request.
    pub request_id: u32,

    /// The ID used in the corresponding `SubscribeSingle` message.
    pub query_id: QueryId,
}

/// Client request for removing a query from a subscription.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct UnsubscribeMulti {
    /// An identifier for a client request.
    pub request_id: u32,

    /// The ID used in the corresponding `SubscribeSingle` message.
    pub query_id: QueryId,
}

/// A one-off query submission.
///
/// Query should be a "SELECT * FROM Table WHERE ...". Other types of queries will be rejected.
/// Multiple such semicolon-delimited queries are allowed.
///
/// One-off queries are identified by a client-generated messageID.
/// To avoid data leaks, the server will NOT cache responses to messages based on UUID!
/// It also will not check for duplicate IDs. They are just a way to match responses to messages.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQuery {
    pub message_id: Box<[u8]>,
    pub query_string: Box<str>,
}

/// The tag recognized by the host and SDKs to mean no compression of a [`ServerMessage`].
pub const SERVER_MSG_COMPRESSION_TAG_NONE: u8 = 0;

/// The tag recognized by the host and SDKs to mean brotli compression  of a [`ServerMessage`].
pub const SERVER_MSG_COMPRESSION_TAG_BROTLI: u8 = 1;

/// The tag recognized by the host and SDKs to mean brotli compression  of a [`ServerMessage`].
pub const SERVER_MSG_COMPRESSION_TAG_GZIP: u8 = 2;

/// Messages sent from the server to the client.
#[derive(SpacetimeType, derive_more::From)]
#[sats(crate = spacetimedb_lib)]
pub enum ServerMessage<F: WebsocketFormat> {
    /// Informs of changes to subscribed rows.
    /// This will be removed when we switch to `SubscribeSingle`.
    InitialSubscription(InitialSubscription<F>),
    /// Upon reducer run.
    TransactionUpdate(TransactionUpdate<F>),
    /// Upon reducer run, but limited to just the table updates.
    TransactionUpdateLight(TransactionUpdateLight<F>),
    /// After connecting, to inform client of its identity.
    IdentityToken(IdentityToken),
    /// Return results to a one off SQL query.
    OneOffQueryResponse(OneOffQueryResponse<F>),
    /// Sent in response to a `SubscribeSingle` message. This contains the initial matching rows.
    SubscribeApplied(SubscribeApplied<F>),
    /// Sent in response to an `Unsubscribe` message. This contains the matching rows.
    UnsubscribeApplied(UnsubscribeApplied<F>),
    /// Communicate an error in the subscription lifecycle.
    SubscriptionError(SubscriptionError),
    /// Sent in response to a `SubscribeMulti` message. This contains the initial matching rows.
    SubscribeMultiApplied(SubscribeMultiApplied<F>),
    /// Sent in response to an `UnsubscribeMulti` message. This contains the matching rows.
    UnsubscribeMultiApplied(UnsubscribeMultiApplied<F>),
}

/// The matching rows of a subscription query.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeRows<F: WebsocketFormat> {
    /// The table ID of the query.
    pub table_id: TableId,
    /// The table name of the query.
    pub table_name: Box<str>,
    /// The BSATN row values.
    pub table_rows: TableUpdate<F>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeApplied<F: WebsocketFormat> {
    /// The request_id of the corresponding `SubscribeSingle` message.
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
    /// An identifier for the subscribed query sent by the client.
    pub query_id: QueryId,
    /// The matching rows for this query.
    pub rows: SubscribeRows<F>,
}

/// Server response to a client [`Unsubscribe`] request.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct UnsubscribeApplied<F: WebsocketFormat> {
    /// Provided by the client via the `Subscribe` message.
    /// TODO: switch to subscription id?
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
    /// The ID included in the `SubscribeApplied` and `Unsubscribe` messages.
    pub query_id: QueryId,
    /// The matching rows for this query.
    /// Note, this makes unsubscribing potentially very expensive.
    /// To remove this in the future, we would need to send query_ids with rows in transaction updates,
    /// and we would need clients to track which rows exist in which queries.
    pub rows: SubscribeRows<F>,
}

/// Server response to an error at any point of the subscription lifecycle.
/// If this error doesn't have a request_id, the client should drop all subscriptions.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscriptionError {
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
    /// Provided by the client via a [`Subscribe`] or [`Unsubscribe`] message.
    /// [`None`] if this occurred as the result of a [`TransactionUpdate`].
    pub request_id: Option<u32>,
    /// Provided by the client via a [`Subscribe`] or [`Unsubscribe`] message.
    /// [`None`] if this occurred as the result of a [`TransactionUpdate`].
    pub query_id: Option<u32>,
    /// The return table of the query in question.
    /// The server is not required to set this field.
    /// It has been added to avoid a breaking change post 1.0.
    ///
    /// If unset, an error results in the entire subscription being dropped.
    /// Otherwise only queries of this table type must be dropped.
    pub table_id: Option<TableId>,
    /// An error message describing the failure.
    ///
    /// This should reference specific fragments of the query where applicable,
    /// but should not include the full text of the query,
    /// as the client can retrieve that from the `request_id`.
    ///
    /// This is intended for diagnostic purposes.
    /// It need not have a predictable/parseable format.
    pub error: Box<str>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeMultiApplied<F: WebsocketFormat> {
    /// The request_id of the corresponding `SubscribeSingle` message.
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
    /// An identifier for the subscribed query sent by the client.
    pub query_id: QueryId,
    /// The matching rows for this query.
    pub update: DatabaseUpdate<F>,
}

/// Server response to a client [`Unsubscribe`] request.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct UnsubscribeMultiApplied<F: WebsocketFormat> {
    /// Provided by the client via the `Subscribe` message.
    /// TODO: switch to subscription id?
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
    /// The ID included in the `SubscribeApplied` and `Unsubscribe` messages.
    pub query_id: QueryId,
    /// The matching rows for this query set.
    /// Note, this makes unsubscribing potentially very expensive.
    /// To remove this in the future, we would need to send query_ids with rows in transaction updates,
    /// and we would need clients to track which rows exist in which queries.
    pub update: DatabaseUpdate<F>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscriptionUpdate<F: WebsocketFormat> {
    /// A [`DatabaseUpdate`] containing only inserts, the rows which match the subscription queries.
    pub database_update: DatabaseUpdate<F>,
    /// An identifier sent by the client in requests.
    /// The server will include the same request_id in the response.
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct InitialSubscription<F: WebsocketFormat> {
    /// A [`DatabaseUpdate`] containing only inserts, the rows which match the subscription queries.
    pub database_update: DatabaseUpdate<F>,
    /// An identifier sent by the client in requests.
    /// The server will include the same request_id in the response.
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration: TimeDuration,
}

/// Received by database from client to inform of user's identity, token and client connection id.
///
/// The database will always send an `IdentityToken` message
/// as the first message for a new WebSocket connection.
/// If the client is re-connecting with existing credentials,
/// the message will include those credentials.
/// If the client connected anonymously,
/// the database will generate new credentials to identify it.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct IdentityToken {
    pub identity: Identity,
    pub token: Box<str>,
    pub connection_id: ConnectionId,
}

/// Received by client from database upon a reducer run.
///
/// Clients receive `TransactionUpdate`s only for reducers
/// which update at least one of their subscribed rows,
/// or for their own `Failed` or `OutOfEnergy` reducer invocations.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct TransactionUpdate<F: WebsocketFormat> {
    /// The status of the transaction. Contains the updated rows, if successful.
    pub status: UpdateStatus<F>,
    /// The time when the reducer started.
    ///
    /// Note that [`Timestamp`] serializes as `i64` nanoseconds since the Unix epoch.
    pub timestamp: Timestamp,
    /// The identity of the user who requested the reducer run. For event-driven and
    /// scheduled reducers, it is the identity of the database owner.
    pub caller_identity: Identity,

    /// The 16-byte [`ConnectionId`] of the user who requested the reducer run.
    ///
    /// The all-zeros id is a sentinel which denotes no meaningful value.
    /// This can occur in the following situations:
    /// - `init` and `update` reducers will have a `caller_connection_id`
    ///   if and only if one was provided to the `publish` HTTP endpoint.
    /// - Scheduled reducers will never have a `caller_connection_id`.
    /// - Reducers invoked by WebSocket or the HTTP API will always have a `caller_connection_id`.
    pub caller_connection_id: ConnectionId,
    /// The original CallReducer request that triggered this reducer.
    pub reducer_call: ReducerCallInfo<F>,
    /// The amount of energy credits consumed by running the reducer.
    pub energy_quanta_used: EnergyQuanta,
    /// How long the reducer took to run.
    pub total_host_execution_duration: TimeDuration,
}

/// Received by client from database upon a reducer run.
///
/// Clients receive `TransactionUpdateLight`s only for reducers
/// which update at least one of their subscribed rows.
/// Failed reducers result in full [`TransactionUpdate`]s
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct TransactionUpdateLight<F: WebsocketFormat> {
    /// An identifier for a client request
    pub request_id: u32,

    /// The reducer ran successfully and its changes were committed to the database.
    /// The rows altered in the database/ are recorded in this `DatabaseUpdate`.
    pub update: DatabaseUpdate<F>,
}

/// Contained in a [`TransactionUpdate`], metadata about a reducer invocation.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct ReducerCallInfo<F: WebsocketFormat> {
    /// The name of the reducer that was called.
    ///
    /// NOTE(centril, 1.0): For bandwidth resource constrained clients
    /// this can encourage them to have poor naming of reducers like `a`.
    /// We should consider not sending this at all and instead
    /// having a startup message where the name <-> id bindings
    /// are established between the host and the client.
    pub reducer_name: Box<str>,
    /// The numerical id of the reducer that was called.
    pub reducer_id: u32,
    /// The arguments to the reducer, encoded as BSATN or JSON according to the reducer's argument schema
    /// and the client's requested protocol.
    pub args: F::Single,
    /// An identifier for a client request
    pub request_id: u32,
}

/// The status of a [`TransactionUpdate`].
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub enum UpdateStatus<F: WebsocketFormat> {
    /// The reducer ran successfully and its changes were committed to the database.
    /// The rows altered in the database/ will be recorded in the `DatabaseUpdate`.
    Committed(DatabaseUpdate<F>),
    /// The reducer errored, and any changes it attempted to were rolled back.
    /// This is the error message.
    Failed(Box<str>),
    /// The reducer was interrupted due to insufficient energy/funds,
    /// and any changes it attempted to make were rolled back.
    OutOfEnergy,
}

/// A collection of inserted and deleted rows, contained in a [`TransactionUpdate`] or [`SubscriptionUpdate`].
#[derive(SpacetimeType, Debug, Clone, Default)]
#[sats(crate = spacetimedb_lib)]
pub struct DatabaseUpdate<F: WebsocketFormat> {
    pub tables: Vec<TableUpdate<F>>,
}

impl<F: WebsocketFormat> DatabaseUpdate<F> {
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    pub fn num_rows(&self) -> usize {
        self.tables.iter().map(|t| t.num_rows()).sum()
    }
}

impl<F: WebsocketFormat> FromIterator<TableUpdate<F>> for DatabaseUpdate<F> {
    fn from_iter<T: IntoIterator<Item = TableUpdate<F>>>(iter: T) -> Self {
        DatabaseUpdate {
            tables: iter.into_iter().collect(),
        }
    }
}

/// Part of a [`DatabaseUpdate`] received by client from database for alterations to a single table.
///
/// NOTE(centril): in 0.12 we added `num_rows` and `table_name` to the struct.
/// These inflate the size of messages, which for some customers is the wrong default.
/// We might want to consider `v1.spacetimedb.bsatn.lightweight`
#[derive(SpacetimeType, Debug, Clone)]
#[sats(crate = spacetimedb_lib)]
pub struct TableUpdate<F: WebsocketFormat> {
    /// The id of the table. Clients should prefer `table_name`, as it is a stable part of a module's API,
    /// whereas `table_id` may change between runs.
    pub table_id: TableId,
    /// The name of the table.
    ///
    /// NOTE(centril, 1.0): we might want to remove this and instead
    /// tell clients about changes to table_name <-> table_id mappings.
    pub table_name: Box<str>,
    /// The sum total of rows in `self.updates`,
    pub num_rows: u64,
    /// The actual insert and delete updates for this table.
    pub updates: SmallVec<[F::QueryUpdate; 1]>,
}

/// Computed update for a single query, annotated with the number of matching rows.
pub struct SingleQueryUpdate<F: WebsocketFormat> {
    pub update: F::QueryUpdate,
    pub num_rows: u64,
}

impl<F: WebsocketFormat> TableUpdate<F> {
    pub fn new(table_id: TableId, table_name: Box<str>, update: SingleQueryUpdate<F>) -> Self {
        Self {
            table_id,
            table_name,
            num_rows: update.num_rows,
            updates: [update.update].into(),
        }
    }

    pub fn empty(table_id: TableId, table_name: Box<str>) -> Self {
        Self {
            table_id,
            table_name,
            num_rows: 0,
            updates: SmallVec::new(),
        }
    }

    pub fn push(&mut self, update: SingleQueryUpdate<F>) {
        self.updates.push(update.update);
        self.num_rows += update.num_rows;
    }

    pub fn num_rows(&self) -> usize {
        self.num_rows as usize
    }
}

#[derive(SpacetimeType, Debug, Clone, EnumAsInner)]
#[sats(crate = spacetimedb_lib)]
pub enum CompressableQueryUpdate<F: WebsocketFormat> {
    Uncompressed(QueryUpdate<F>),
    Brotli(Bytes),
    Gzip(Bytes),
}

impl CompressableQueryUpdate<BsatnFormat> {
    pub fn maybe_decompress(self) -> QueryUpdate<BsatnFormat> {
        match self {
            Self::Uncompressed(qu) => qu,
            Self::Brotli(bytes) => {
                let bytes = brotli_decompress(&bytes).unwrap();
                bsatn::from_slice(&bytes).unwrap()
            }
            Self::Gzip(bytes) => {
                let bytes = gzip_decompress(&bytes).unwrap();
                bsatn::from_slice(&bytes).unwrap()
            }
        }
    }
}

#[derive(SpacetimeType, Debug, Clone)]
#[sats(crate = spacetimedb_lib)]
pub struct QueryUpdate<F: WebsocketFormat> {
    /// When in a [`TransactionUpdate`], the matching rows of this table deleted by the transaction.
    ///
    /// Rows are encoded as BSATN or JSON according to the table's schema
    /// and the client's requested protocol.
    ///
    /// Always empty when in an [`InitialSubscription`].
    pub deletes: F::List,
    /// When in a [`TransactionUpdate`], the matching rows of this table inserted by the transaction.
    /// When in an [`InitialSubscription`], the matching rows of this table in the entire committed state.
    ///
    /// Rows are encoded as BSATN or JSON according to the table's schema
    /// and the client's requested protocol.
    pub inserts: F::List,
}

/// A response to a [`OneOffQuery`].
/// Will contain either one error or some number of response rows.
/// At most one of these messages will be sent in reply to any query.
///
/// The messageId will be identical to the one sent in the original query.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQueryResponse<F: WebsocketFormat> {
    pub message_id: Box<[u8]>,
    /// If query compilation or evaluation errored, an error message.
    pub error: Option<Box<str>>,

    /// If query compilation and evaluation succeeded, a set of resulting rows, grouped by table.
    pub tables: Box<[OneOffTable<F>]>,

    /// The total duration of query compilation and evaluation on the server, in microseconds.
    pub total_host_execution_duration: TimeDuration,
}

/// A table included as part of a [`OneOffQueryResponse`].
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffTable<F: WebsocketFormat> {
    /// The name of the table.
    pub table_name: Box<str>,
    /// The set of rows which matched the query, encoded as BSATN or JSON according to the table's schema
    /// and the client's requested protocol.
    ///
    /// TODO(centril, 1.0): Evalutate whether we want to conditionally compress these.
    pub rows: F::List,
}

/// Used whenever different formats need to coexist.
#[derive(Debug, Clone)]
pub enum FormatSwitch<B, J> {
    Bsatn(B),
    Json(J),
}

impl<B1, J1> FormatSwitch<B1, J1> {
    /// Zips together two switches.
    pub fn zip_mut<B2, J2>(&mut self, other: FormatSwitch<B2, J2>) -> FormatSwitch<(&mut B1, B2), (&mut J1, J2)> {
        match (self, other) {
            (FormatSwitch::Bsatn(a), FormatSwitch::Bsatn(b)) => FormatSwitch::Bsatn((a, b)),
            (FormatSwitch::Json(a), FormatSwitch::Json(b)) => FormatSwitch::Json((a, b)),
            _ => panic!("format should be the same for both sides of the zip"),
        }
    }
}

#[derive(Clone, Copy, Default, Debug, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct JsonFormat;

impl WebsocketFormat for JsonFormat {
    type Single = ByteString;

    type List = Vec<ByteString>;

    fn encode_list<R: ToBsatn + Serialize>(elems: impl Iterator<Item = R>) -> (Self::List, u64) {
        let mut count = 0;
        let list = elems
            .map(|elem| serde_json::to_string(&SerializeWrapper::new(elem)).unwrap().into())
            .inspect(|_| count += 1)
            .collect();
        (list, count)
    }

    type QueryUpdate = QueryUpdate<Self>;

    fn into_query_update(qu: QueryUpdate<Self>, _: Compression) -> Self::QueryUpdate {
        qu
    }
}

#[derive(Clone, Copy, Default, Debug, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct BsatnFormat;

impl WebsocketFormat for BsatnFormat {
    type Single = Box<[u8]>;

    type List = BsatnRowList;

    fn encode_list<R: ToBsatn + Serialize>(mut elems: impl Iterator<Item = R>) -> (Self::List, u64) {
        // For an empty list, the size of a row is unknown, so use `RowOffsets`.
        let Some(first) = elems.next() else {
            return (BsatnRowList::row_offsets(), 0);
        };
        // We have at least one row. Determine the static size from that, if available.
        let (mut list, mut scratch) = match first.static_bsatn_size() {
            Some(size) => (BsatnRowListBuilder::fixed(size), Vec::with_capacity(size as usize)),
            None => (BsatnRowListBuilder::row_offsets(), Vec::new()),
        };
        // Add the first element and then the rest.
        // We assume that the schema of rows yielded by `elems` stays the same,
        // so once the size is fixed, it will stay that way.
        let mut count = 0;
        let mut push = |elem: R| {
            elem.to_bsatn_extend(&mut scratch).unwrap();
            list.push(&scratch);
            scratch.clear();
            count += 1;
        };
        push(first);
        for elem in elems {
            push(elem);
        }
        (list.finish(), count)
    }

    type QueryUpdate = CompressableQueryUpdate<Self>;

    fn into_query_update(qu: QueryUpdate<Self>, compression: Compression) -> Self::QueryUpdate {
        let qu_len_would_have_been = bsatn::to_len(&qu).unwrap();

        match decide_compression(qu_len_would_have_been, compression) {
            Compression::None => CompressableQueryUpdate::Uncompressed(qu),
            Compression::Brotli => {
                let bytes = bsatn::to_vec(&qu).unwrap();
                let mut out = Vec::new();
                brotli_compress(&bytes, &mut out);
                CompressableQueryUpdate::Brotli(out.into())
            }
            Compression::Gzip => {
                let bytes = bsatn::to_vec(&qu).unwrap();
                let mut out = Vec::new();
                gzip_compress(&bytes, &mut out);
                CompressableQueryUpdate::Gzip(out.into())
            }
        }
    }
}

/// A specification of either a desired or decided compression algorithm.
#[derive(serde::Deserialize, Default, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Compression {
    /// No compression ever.
    None,
    /// Compress using brotli if a certain size threshold was met.
    #[default]
    Brotli,
    /// Compress using gzip if a certain size threshold was met.
    Gzip,
}

pub fn decide_compression(len: usize, compression: Compression) -> Compression {
    /// The threshold beyond which we start to compress messages.
    /// 1KiB was chosen without measurement.
    /// TODO(perf): measure!
    const COMPRESS_THRESHOLD: usize = 1024;

    if len > COMPRESS_THRESHOLD {
        compression
    } else {
        Compression::None
    }
}

pub fn brotli_compress(bytes: &[u8], out: &mut Vec<u8>) {
    let reader = &mut &bytes[..];

    // The default Brotli buffer size.
    const BUFFER_SIZE: usize = 4096;
    // We are optimizing for compression speed,
    // so we choose the lowest (fastest) level of compression.
    // Experiments on internal workloads have shown compression ratios between 7:1 and 10:1
    // for large `SubscriptionUpdate` messages at this level.
    const COMPRESSION_LEVEL: u32 = 1;
    // The default value for an internal compression parameter.
    // See `BrotliEncoderParams` for more details.
    const LG_WIN: u32 = 22;

    let mut encoder = brotli::CompressorReader::new(reader, BUFFER_SIZE, COMPRESSION_LEVEL, LG_WIN);

    encoder
        .read_to_end(out)
        .expect("Failed to Brotli compress `SubscriptionUpdateMessage`");
}

pub fn brotli_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut &bytes[..], &mut decompressed)?;
    Ok(decompressed)
}

pub fn gzip_compress(bytes: &[u8], out: &mut Vec<u8>) {
    let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().expect("Failed to gzip compress `bytes`");
}

pub fn gzip_decompress(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut decompressed = Vec::new();
    let _ = flate2::read::GzDecoder::new(bytes).read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

type RowSize = u16;
type RowOffset = u64;

/// A packed list of BSATN-encoded rows.
#[derive(SpacetimeType, Debug, Clone)]
#[sats(crate = spacetimedb_lib)]
pub struct BsatnRowList<B = Bytes, I = Arc<[RowOffset]>> {
    /// A size hint about `rows_data`
    /// intended to facilitate parallel decode purposes on large initial updates.
    size_hint: RowSizeHint<I>,
    /// The flattened byte array for a list of rows.
    rows_data: B,
}

impl Default for BsatnRowList {
    fn default() -> Self {
        Self::row_offsets()
    }
}

/// NOTE(centril, 1.0): We might want to add a `None` variant to this
/// where the client has to decode in a loop until `rows_data` has been exhausted.
/// The use-case for this is clients who are bandwidth limited and where every byte counts.
#[derive(SpacetimeType, Debug, Clone)]
#[sats(crate = spacetimedb_lib)]
pub enum RowSizeHint<I> {
    /// Each row in `rows_data` is of the same fixed size as specified here.
    FixedSize(RowSize),
    /// The offsets into `rows_data` defining the boundaries of each row.
    /// Only stores the offset to the start of each row.
    /// The ends of each row is inferred from the start of the next row, or `rows_data.len()`.
    /// The behavior of this is identical to that of `PackedStr`.
    RowOffsets(I),
}

impl<I: AsRef<[RowOffset]>> RowSizeHint<I> {
    fn index_to_range(&self, index: usize, data_end: usize) -> Option<Range<usize>> {
        match self {
            Self::FixedSize(size) => {
                let size = *size as usize;
                let start = index * size;
                if start >= data_end {
                    // We've reached beyond `data_end`,
                    // so this is a row that doesn't exist, so we are beyond the count.
                    return None;
                }
                let end = (index + 1) * size;
                Some(start..end)
            }
            Self::RowOffsets(offsets) => {
                let offsets = offsets.as_ref();
                let start = *offsets.get(index)? as usize;
                // The end is either the start of the next element or the end.
                let end = offsets.get(index + 1).map(|e| *e as usize).unwrap_or(data_end);
                Some(start..end)
            }
        }
    }
}

impl<B: Default, I> BsatnRowList<B, I> {
    pub fn fixed(row_size: RowSize) -> Self {
        Self {
            size_hint: RowSizeHint::FixedSize(row_size),
            rows_data: <_>::default(),
        }
    }

    /// Returns a new empty list using indices
    pub fn row_offsets() -> Self
    where
        I: From<[RowOffset; 0]>,
    {
        Self {
            size_hint: RowSizeHint::RowOffsets([].into()),
            rows_data: <_>::default(),
        }
    }
}

impl<B: AsRef<[u8]>, I: AsRef<[RowOffset]>> RowListLen for BsatnRowList<B, I> {
    /// Returns the length of the row list.
    fn len(&self) -> usize {
        match &self.size_hint {
            RowSizeHint::FixedSize(size) => self.rows_data.as_ref().len() / *size as usize,
            RowSizeHint::RowOffsets(offsets) => offsets.as_ref().len(),
        }
    }
}

impl<B: AsRef<[u8]>, I> ByteListLen for BsatnRowList<B, I> {
    /// Returns the uncompressed size of the list in bytes
    fn num_bytes(&self) -> usize {
        self.rows_data.as_ref().len()
    }
}

impl BsatnRowList {
    /// Returns the element at `index` in the list.
    pub fn get(&self, index: usize) -> Option<Bytes> {
        let data_end = self.rows_data.len();
        let data_range = self.size_hint.index_to_range(index, data_end)?;
        Some(self.rows_data.slice(data_range))
    }
}

/// An iterator over all the elements in a [`BsatnRowList`].
pub struct BsatnRowListIter<'a> {
    list: &'a BsatnRowList,
    index: usize,
}

impl<'a> IntoIterator for &'a BsatnRowList {
    type IntoIter = BsatnRowListIter<'a>;
    type Item = Bytes;
    fn into_iter(self) -> Self::IntoIter {
        BsatnRowListIter { list: self, index: 0 }
    }
}

impl Iterator for BsatnRowListIter<'_> {
    type Item = Bytes;
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.index;
        self.index += 1;
        self.list.get(index)
    }
}

/// A [`BsatnRowList`] that can be added to.
pub type BsatnRowListBuilder = BsatnRowList<Vec<u8>, Vec<RowOffset>>;

impl BsatnRowListBuilder {
    /// Adds `row`, BSATN-encoded to this list.
    #[inline]
    pub fn push(&mut self, row: &[u8]) {
        if let RowSizeHint::RowOffsets(offsets) = &mut self.size_hint {
            offsets.push(self.rows_data.len() as u64);
        }
        self.rows_data.extend_from_slice(row);
    }

    /// Finish the in flight list, throwing away the capability to mutate.
    pub fn finish(self) -> BsatnRowList {
        let Self { size_hint, rows_data } = self;
        let rows_data = rows_data.into();
        let size_hint = match size_hint {
            RowSizeHint::FixedSize(fs) => RowSizeHint::FixedSize(fs),
            RowSizeHint::RowOffsets(ro) => RowSizeHint::RowOffsets(ro.into()),
        };
        BsatnRowList { size_hint, rows_data }
    }
}
