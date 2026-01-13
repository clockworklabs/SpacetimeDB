pub use super::common::{CallProcedureFlags, CallReducerFlags, QuerySetId};
use bytes::Bytes;
use spacetimedb_lib::{ConnectionId, Identity, Timestamp};
pub use spacetimedb_sats::SpacetimeType;

pub const BIN_PROTOCOL: &str = "v2.bsatn.spacetimedb";

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ClientMessage {
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    OneOffQuery(OneOffQuery),
    CallReducer(CallReducer),
    CallProcedure(CallProcedure),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct Subscribe {
    /// An identifier for a client request.
    pub request_id: u32,

    /// An identifier for this subscription,
    /// which should not be used for any other subscriptions on the same connection.
    ///
    /// This is used to refer to this subscription in [`Unsubscribe`] messages from the client
    /// and in various responses from the server.
    /// These only have meaning given a [`ConnectionId`]; they are not global.
    pub query_set_id: QuerySetId,

    /// A set of queries to subscribe to, each a single SQL `SELECT` statement.
    pub query_strings: Box<[Box<str>]>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct Unsubscribe {
    /// An identifier for a client request.
    pub request_id: u32,

    /// The ID used in the corresponding `Single` message.
    pub query_set_id: QuerySetId,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQuery {
    /// An identifier for a client request.
    pub request_id: u32,

    /// A single SQL `SELECT` statement.
    pub query_string: Box<str>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct CallReducer {
    /// An identifier for a client request.
    pub request_id: u32,

    /// Assorted flags that can be passed when calling a reducer.
    ///
    /// Currently accepts 0 or 1 where the latter means
    /// that the caller does not want to be notified about the reducer
    /// without being subscribed to any relevant queries.
    pub flags: CallReducerFlags,

    /// The name of the reducer to call.
    pub reducer: Box<str>,

    /// The arguments to the reducer.
    ///
    /// A BSATN-encoded [`ProductValue`] which meets the reducer's argument schema.
    pub args: Bytes,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct CallProcedure {
    /// An identifier for a client request.
    pub request_id: u32,

    /// Reserved 0.
    pub flags: CallProcedureFlags,

    /// The name of the procedure to call.
    pub procedure: Box<str>,

    /// The arguments to the procedure.
    ///
    /// A BSATN-encoded [`ProductValue`] which meets the procedure's argument schema.
    pub args: Bytes,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ServerMessage {
    InitialConnection(InitialConnection),
    SubscribeApplied(SubscribeApplied),
    UnsubscribeApplied(UnsubscribeApplied),
    SubscriptionError(SubscriptionError),
    TransactionUpdate(TransactionUpdate),
    OneOffQueryResult(OneOffQueryResult),
    ReducerResult(ReducerResult),
    ProcedureResult(ProcedureResult),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct InitialConnection {
    pub identity: Identity,
    pub connection_id: ConnectionId,
    pub token: Box<str>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeApplied {
    /// The request_id of the corresponding `SubscribeSingle` message.
    pub request_id: u32,
    /// An identifier for the subscribed query sent by the client.
    pub query_set_id: QuerySetId,
    /// The matching rows for this query.
    pub rows: QueryRows,
}

/// Server response to a client [`Unsubscribe`] request.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct UnsubscribeApplied {
    /// Provided by the client via the `Subscribe` message.
    /// TODO: switch to subscription id?
    pub request_id: u32,
    /// The ID included in the `SubscribeApplied` and `Unsubscribe` messages.
    pub query_set_id: QuerySetId,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct QueryRows {
    pub tables: Box<[SingleTableRows]>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SingleTableRows {
    pub table: Box<str>,
    pub rows: Box<[Bytes]>,
}

/// Server response to an error at any point of the subscription lifecycle.
/// If this error doesn't have a request_id, the client should drop all subscriptions.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscriptionError {
    /// Provided by the client via a [`Subscribe`] message.
    /// [`None`] if this occurred as the result of a [`TransactionUpdate`].
    pub request_id: Option<u32>,
    /// Provided by the client via a [`Subscribe`] message.
    ///
    /// After receiving this message, the client should drop all its rows from this [`QuerySetId`],
    /// and should not expect to receive any additional updates for that query set.
    pub query_set_id: QuerySetId,
    /// An error message describing the failure.
    ///
    /// This should reference specific fragments of the query where applicable,
    /// but should not include the full text of the query,
    /// as the client can retrieve that from the `request_id` or `query_set_id`.
    ///
    /// This is intended for diagnostic purposes.
    /// It need not have a predictable/parseable format.
    pub error: Box<str>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct TransactionUpdate {
    // TODO: Do we want a timestamp here? Or should we just tell users to emit an event with a timestamp if they want that.
    pub query_sets: Box<[QuerySetUpdate]>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct QuerySetUpdate {
    pub query_set_id: QuerySetId,
    pub tables: Box<[TableUpdate]>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct TableUpdate {
    pub table_name: Box<str>,
    pub rows: TableUpdateRows,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum TableUpdateRows {
    PersistentTable(PersistentTableRows),
    EventTable(EventTableRows),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct PersistentTableRows {
    pub inserts: Box<[Bytes]>,
    pub deletes: Box<[Bytes]>,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct EventTableRows {
    pub events: Box<[Bytes]>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQueryResult {
    /// The request_id of the corresponding `SubscribeSingle` message.
    pub request_id: u32,
    /// The matching rows for this query, or an error message if computation failed.
    ///
    /// This error message should follow the same format as [`SubscriptionError::error`].
    pub result: Result<QueryRows, Box<str>>,
}

/// Response to [`Subscribe`] containing the initial matching rows.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct ReducerResult {
    /// The request_id of the corresponding `SubscribeSingle` message.
    pub request_id: u32,
    /// The time when the reducer started.
    ///
    /// Note that [`Timestamp`] serializes as `i64` nanoseconds since the Unix epoch.
    pub timestamp: Timestamp,
    pub result: ReducerOutcome,
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ReducerOutcome {
    Ok(SubscriptionOk),
    Err(Bytes),
    InternalError(Box<str>),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscriptionOk {
    pub ret_value: Bytes,
    pub transaction_update: TransactionUpdate,
}

/// The result of running a procedure,
/// including the return value of the procedure on success.
///
/// Sent in response to a [`CallProcedure`] message.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct ProcedureResult<F: WebsocketFormat> {
    /// The status of the procedure run.
    ///
    /// Contains the return value if successful, or the error message if not.
    pub status: ProcedureStatus<F>,
    /// The time when the reducer started.
    ///
    /// Note that [`Timestamp`] serializes as `i64` nanoseconds since the Unix epoch.
    pub timestamp: Timestamp,
    /// The time the procedure took to run.
    pub total_host_execution_duration: TimeDuration,
    /// The same same client-provided identifier as in the original [`ProcedureCall`] request.
    ///
    /// Clients use this to correlate the response with the original request.
    pub request_id: u32,
}

/// The status of a procedure call,
/// including the return value on success.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub enum ProcedureStatus {
    /// The procedure ran and returned the enclosed value.
    ///
    /// All user error handling happens within here;
    /// the returned value may be a `Result` or `Option`,
    /// or any other type to which the user may ascribe arbitrary meaning.
    Returned(Bytes),
    /// The call failed in the host, e.g. due to a type error or unknown procedure name.
    InternalError(Box<str>),
}
