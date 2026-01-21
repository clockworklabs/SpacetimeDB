pub use super::common::{CallProcedureFlags, QuerySetId};
use bytes::Bytes;
use spacetimedb_lib::{ConnectionId, Identity, TimeDuration, Timestamp};
pub use spacetimedb_sats::SpacetimeType;
use spacetimedb_sats::{de::Error, impl_deserialize, impl_serialize, impl_st, AlgebraicType};

pub const BIN_PROTOCOL: &str = "v2.bsatn.spacetimedb";

/// Messages sent by the client to the server.
///
/// Each client message contains a `request_id`, a client-supplied integer ID.
/// The server assigns no meaning to this value, but encloses the same value in its response [`ServerMessage`].
/// Clients can use `request_id`s to correlate requests and responses.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ClientMessage {
    /// Add a new set of subscribed queries to construct a local materialized view of matching rows.
    Subscribe(Subscribe),
    /// Remove a previously-registered set of subscribed queries to stop receiving updates on its view.
    Unsubscribe(Unsubscribe),
    /// Run a query once and receive its results at a single point in time, without real-time updates.
    OneOffQuery(OneOffQuery),
    /// Invoke a reducer, a transactional non-side-effecting function which runs in the database.
    CallReducer(CallReducer),
    /// Invoke a procedure, a non-transactional side-effecting function which runs in the database.
    CallProcedure(CallProcedure),
}

/// Sent by client to register a subscription to a new query set
/// for which the client should receive [`QuerySetUpdate`]s in its [`TransactionUpdate`]s.
///
/// Each subscribed query set is identified by a client-supplied [`QuerySetId`],
/// which should be unique within that client's connection.
/// The server will include that [`QuerySetId`] in updates with the matching rows,
/// and the client can later send that [`QuerySetId`] in an [`Unsubscribe`] message to end the subscription.
///
/// If the enclosed queries are valid and compute successfully,
/// the server will respond with a [`SubscribeApplied`] message marked with the same `request_id` and [`QuerySetId`]
/// containing the initial matching rows,
/// and will then send matching inserts and deletes in [`QuerySetUpdate`]s enclosed in [`TransactionUpdate`] messages
/// as the changes occur.
///
/// If the enclosed queries are invalid or fail to compute, the server will respond with a [`SubscriptionError`] message.
/// If the queries become invalid after an initial successful application,
/// the server may send a [`SubscribeApplied`], some number of [`TransactionUpdate`]s, and then a [`SubscriptionError`].
/// After receiving a [`SubscriptionError`], the client should discard all previously-received rows for that [`QuerySetId`]
/// and should not expect to receive updates for it in the future.
/// That [`QuerySetId`] may then be re-used at the client's discretion.
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

/// Sent by client to end a subscription which was previously added in a [`Subscribe`] message.
///
/// After the server processes an unsubscribe message, it will send an [`UnsubscribeApplied`] as confirmation.
/// Following the [`UnsubscribeApplied`], the server will not reference the enclosed [`QuerySetId`] again,
/// and so it may be reused.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct Unsubscribe {
    /// An identifier for a client request.
    pub request_id: u32,

    /// The ID used in the corresponding [`Subscribe`] message.
    pub query_set_id: QuerySetId,
}

/// Sent by the client to perform a query at a single point in time.
///
/// Unlike subscriptions registered by [`Subscribe`], this query will not receive real-time updates.
///
/// The server will respond with a [`OneOffQueryResponse`] message containing the same `request_id`
/// and the status of the query, either the matching rows or an error message.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQuery {
    /// An identifier for a client request.
    pub request_id: u32,

    /// A single SQL `SELECT` statement.
    pub query_string: Box<str>,
}

/// Sent by the client to invoke a reducer, a transactional non-side-effecting database function.
///
/// After the reducer runs, the server will respond with a [`CallReducerResult`] message containing the same `request_id`
/// and the status of the run, either the return value and [`TransactionUpdate`] or an error.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct CallReducer {
    /// An identifier for a client request.
    pub request_id: u32,

    /// Reserved 0.
    pub flags: CallReducerFlags,

    /// The name of the reducer to call.
    pub reducer: Box<str>,

    /// The arguments to the reducer.
    ///
    /// A BSATN-encoded [`ProductValue`] which meets the reducer's argument schema.
    pub args: Bytes,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CallReducerFlags {
    #[default]
    Default,
}

impl_st!([] CallReducerFlags, AlgebraicType::U8);
impl_serialize!([] CallReducerFlags, (self, ser) => ser.serialize_u8(*self as u8));
impl_deserialize!([] CallReducerFlags, de => match de.deserialize_u8()? {
    0 => Ok(Self::Default),
    x => Err(D::Error::custom(format_args!("invalid call reducer flag {x}"))),
});

/// Sent by the client to invoke a procedure, a non-transactional side-effecting database function.
///
/// After the procedure runs, the server will respond with a [`CallProcedureResult`] message containing the same `request_id`
/// and the status of the run, either the return value or an error.
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

/// Messages sent by the server to the client in response to requests or database events.
///
/// Server messages which are responses to client messages will contain a `request_id`.
/// This will take the same value as the client supplied in their request.
/// Clients can use `request_id`s to correlate requests and responses.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ServerMessage {
    /// The first message sent upon a successful connection.
    /// Contains information about the client's identity and authentication.
    InitialConnection(InitialConnection),
    /// In response to a [`Subscribe`] message, after a new query set has been added, containing its initial matching rows.
    SubscribeApplied(SubscribeApplied),
    /// In response to an [`Unsubscribe`] message, confirming that a query set has been removed.
    UnsubscribeApplied(UnsubscribeApplied),
    /// Notifies the client that a subscription to a query set has failed, either during initial application
    /// or when computing a [`QuerySetUpdate`] for a [`TransactionUpdate`].
    SubscriptionError(SubscriptionError),
    /// Sent after the database runs a transaction, to notify the client of any changes to its subscribed query sets
    /// in [`QuerySetUpdate`]s.
    TransactionUpdate(TransactionUpdate),
    /// Sent in response to a [`OneOffQuery`] message, containing the matching rows or error message.
    OneOffQueryResult(OneOffQueryResult),
    /// Sent in response to a [`CallReducer`] message, containing the reducer's exit status and, if it committed,
    /// the [`TransactionUpdate`] for that reducer's transaction.
    ReducerResult(ReducerResult),
    /// Sent in response to a [`CallProcedure`] message, containing the procedure's exit status.
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
///
/// This message's `request_id` and `query_set_id` will match those the client provided in the [`Subscribe`] message.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SubscribeApplied {
    /// The request_id of the corresponding [`Subscribe`] message.
    pub request_id: u32,
    /// An identifier for the subscribed query set provided by the client.
    pub query_set_id: QuerySetId,
    /// The matching rows for this query.
    pub rows: QueryRows,
}

/// Matching rows resident in tables at the time a query ran,
/// used in contexts where we're not sending insert/delete deltas,
/// like [`SubscribeApplied`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct QueryRows {
    pub tables: Box<[SingleTableRows]>,
}

/// Matching rows resident in a table at the time a query ran,
/// used in contexts where we're not sending insert/delete deltas,
/// like the [`QueryRows`] of a [`SubscribeApplied`], and [`OneOffQueryResponse`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct SingleTableRows {
    pub table: Box<str>,
    pub rows: Box<[Bytes]>,
}

/// Server response to a client [`Unsubscribe`] request.
///
/// This message's `request_id` and `query_set_id` will match those the client provided in the [`Unsubscribe`] message.
///
/// After receiving this message, the client will no longer receive any [`QuerySetUpdate`]s for the included [`QuerySetId`].
/// That [`QuerySetId`] may then be re-used at the client's discretion.
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct UnsubscribeApplied {
    /// Provided by the client via the `Subscribe` message.
    /// TODO: switch to subscription id?
    pub request_id: u32,
    /// The ID included in the `SubscribeApplied` and `Unsubscribe` messages.
    pub query_set_id: QuerySetId,
}

/// Server response to an error at any point of the subscription lifecycle.
///
/// If initial compilation or computation of a query fails, the server will send this message
/// in lieu of a [`SubscribeApplied`].
/// In that case, the `request_id` will be `Some` and will match the one the client supplied in the [`Subscribe`] message.
///
/// If a query fails after being applied, e.g. during recompilation or incremental evaluation,
/// the server will send this message with `request_id` set to `None`.
///
/// In either case, this message will have its `query_set_id` set to the one provided by the client
/// to identify the failed query set.
/// After receiving this message, the client should consider the subscription to that query set to have ended,
/// should discard all previously-received matching rows,
/// and should not expect to receive any further [`QuerySetUpdate`]s for that [`QuerySetId`].
/// That [`QuerySetId`] may then be re-used at the client's discretion.
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

/// Sent by the server to the client after a transaction runs and commits successfully in the database,
/// containing [`QuerySetUpdate`]s for each of the client's subscribed query sets
/// whose results were affected by the transaction.
///
/// If a transaction does not affect a particular query set,
/// the transaction update will not contain a [`QuerySetUpdate`] for that set.
///
/// If none of a client's query sets were affected by a transaction,
/// they will not receive an empty [`TransactionUpdate`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct TransactionUpdate {
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

/// The rows of a [`TableUpdate`], separated based on the kind of table.
///
/// Regular "persistent" tables will include a list of inserted rows and a list of deleted rows.
/// Event tables, whose rows are not persistent, will instead include a single list of event rows.
///
/// In the future, we may add additional variants to this enum.
/// In particular, we may add a variant for in-place updates of rows for tables with primary keys.
/// Note that clients will need to opt in to using this new variant,
/// to preserve compatibility of clients which predate the new variant.
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

/// The result of running a reducer, including its return value and [`TransactionUpdate`] on success,
/// or its error on failure.
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
    /// The reducer returned successfully and its transaction committed.
    /// The return value and [`TransactionUpdate`] are included here.
    Ok(ReducerOk),
    /// The reducer returned successfully and its transaction committed,
    /// but its return value was zero bytes and its [`TransactionUpdate`] contained zero [`QuerySetUpdate`]s.
    ///
    /// This variant is an optimization which saves 8 bytes of wire size,
    /// due to the BSATN format's using 4 bytes for the length of a variable-length object,
    /// such as the `ret_value` of [`ReducerOk`] and the `query_sets` of [`TransactionUpdate`].
    Okmpty,
    /// The reducer returned an expected, structured error,
    /// and its transaction did not commit.
    ///
    /// The payload is a BSATN-encoded value of the reducer's error return type.
    Err(Bytes),
    /// The reducer panicked, returned an unexpected and unstructured error, or failed to run due to a SpacetimeDB internal error.
    ///
    /// The payload is an error message, which is intended for diagnostic purposes only,
    /// and is not intended to have a stable or parseable format.
    InternalError(Box<str>),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct ReducerOk {
    pub ret_value: Bytes,
    pub transaction_update: TransactionUpdate,
}

/// The result of running a procedure,
/// including the return value of the procedure on success.
///
/// Sent in response to a [`CallProcedure`] message.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct ProcedureResult {
    /// The status of the procedure run.
    ///
    /// Contains the return value if successful, or the error message if not.
    pub status: ProcedureStatus,
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
