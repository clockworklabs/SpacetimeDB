use bytes::Bytes;
use spacetimedb_lib::{Address, Identity};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::SpacetimeType;

use crate::energy::EnergyQuanta;
use crate::timestamp::Timestamp;

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ClientMessage<Args> {
    /// Request a reducer run.
    CallReducer(CallReducer<Args>),
    /// Register SQL queries on which to receive updates.
    Subscribe(Subscribe),
    /// Send a one-off SQL query without establishing a subscription.
    OneOffQuery(OneOffQuery),
}

impl<Args> ClientMessage<Args> {
    pub fn map_args<Args2>(self, f: impl FnOnce(Args) -> Args2) -> ClientMessage<Args2> {
        match self {
            ClientMessage::CallReducer(CallReducer {
                reducer,
                args,
                request_id,
            }) => ClientMessage::CallReducer(CallReducer {
                reducer,
                args: f(args),
                request_id,
            }),
            ClientMessage::Subscribe(x) => ClientMessage::Subscribe(x),
            ClientMessage::OneOffQuery(x) => ClientMessage::OneOffQuery(x),
        }
    }
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct CallReducer<Args> {
    /// The name or id of the reducer to call.
    pub reducer: ReducerId,
    /// The arguments to the reducer, encoded as ????.
    pub args: Args,
    /// An identifier for a client request
    pub request_id: u32,
}

/// A specification of the reducer to call for [`CallReducer`].
#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum ReducerId {
    /// A reducer specified by its name.
    Name(String),
    // /// A reducer specified by its numerical ID.
    // Id(u32),
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
    pub query_strings: Vec<String>,
    pub request_id: u32,
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
    pub message_id: Vec<u8>,
    pub query_string: String,
}

#[derive(SpacetimeType, derive_more::From)]
#[sats(crate = spacetimedb_lib)]
pub enum ServerMessage {
    /// Informs of changes to subscribed rows.
    InitialSubscription(InitialSubscription),
    /// Upon reducer run.
    TransactionUpdate(TransactionUpdate),
    /// After connecting, to inform client of its identity.
    IdentityToken(IdentityToken),
    /// Return results to a one off SQL query.
    OneOffQueryResponse(OneOffQueryResponse),
}

#[derive(SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct InitialSubscription {
    pub database_update: DatabaseUpdate,
    /// An identifier sent by the client in requests.
    /// The server will include the same request_id in the response.
    pub request_id: u32,
    /// The overall time between the server receiving a request and sending the response.
    pub total_host_execution_duration_micros: u64,
}

/// Received by database from client to inform of user's identity, token and client address.
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
    pub token: String,
    pub address: Address,
}

// TODO: Evaluate if it makes sense for this to also include the
// address of the database this is calling

/// Received by client from database upon a reducer run.
///
/// Clients receive `TransactionUpdate`s only for reducers
/// which update at least one of their subscribed rows,
/// or for their own `Failed` or `OutOfEnergy` reducer invocations.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct TransactionUpdate {
    /// The status of the transaction. Contains the updated rows, if successful.
    pub status: UpdateStatus,
    /// The time when the reducer started, as microseconds since the Unix epoch.
    pub timestamp: Timestamp,
    /// The identity of the user who requested the reducer run. For event-driven and
    /// scheduled reducers, it is the identity of the database owner.                     
    pub caller_identity: Identity,
    /// The 16-byte address of the user who requested the reducer run.
    /// The all-zeros address is a sentinel which denotes no address.
    /// `init` and `update` reducers will have a `caller_address`
    /// if and only if one was provided to the `publish` HTTP endpoint.
    /// Scheduled reducers will never have a `caller_address`.
    /// Reducers invoked by HTTP will have a `caller_address`
    /// if and only if one was provided to the `call` HTTP endpoint.
    /// Reducers invoked by WebSocket will always have a `caller_address`.
    pub caller_address: Address,
    /// The original CallReducer request that triggered this reducer.
    pub reducer_call: ReducerCallInfo,
    /// The amount of energy credits consumed by running the reducer.
    pub energy_quanta_used: EnergyQuanta,
    /// How long the reducer took to run.
    pub host_execution_duration_micros: u64,
}

#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct ReducerCallInfo {
    /// The name of the reducer that was called.
    pub reducer_name: String,
    /// The numerical id of the reducer that was called.
    pub reducer_id: u32,
    /// The arguments to the reducer.
    pub args: Row,
    /// An identifier for a client request
    pub request_id: u32,
}

/// The status of a [`TransactionUpdate`].
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub enum UpdateStatus {
    /// The reducer ran successfully and its changes were committed to the database.
    /// The rows altered in the database/ will be recorded in the `DatabaseUpdate`.
    Committed(DatabaseUpdate),
    /// The reducer errored, and any changes it attempted to were rolled back.
    /// This is the error message.
    Failed(String),
    /// The reducer was interrupted due to insufficient energy/funds,
    /// and any changes it attempted to make were rolled back.
    OutOfEnergy,
}

#[derive(SpacetimeType, Debug, Clone, Default)]
#[sats(crate = spacetimedb_lib)]
pub struct DatabaseUpdate {
    pub tables: Vec<TableUpdate>,
}

impl DatabaseUpdate {
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

impl FromIterator<TableUpdate> for DatabaseUpdate {
    fn from_iter<T: IntoIterator<Item = TableUpdate>>(iter: T) -> Self {
        DatabaseUpdate {
            tables: iter.into_iter().collect(),
        }
    }
}

/// Part of a `SubscriptionUpdate` received by client from database for alterations to a
/// single table.
#[derive(SpacetimeType, Debug, Clone)]
#[sats(crate = spacetimedb_lib)]
pub struct TableUpdate {
    /// The id of the table. Clients should prefer `table_name`, as it is a stable part of a module's API,
    /// whereas `table_id` may change between runs.
    pub table_id: TableId,
    /// The name of the table.
    pub table_name: String,
    pub deletes: Vec<Row>,
    pub inserts: Vec<Row>,
}

#[derive(SpacetimeType, Debug, Clone, derive_more::From)]
#[sats(crate = spacetimedb_lib, transparent)]
pub struct Row(pub Bytes);

impl From<Vec<u8>> for Row {
    fn from(b: Vec<u8>) -> Self {
        Row(b.into())
    }
}

/// A one-off query response.
/// Will contain either one error or multiple response rows.
/// At most one of these messages will be sent in reply to any query.
///
/// The messageId will be identical to the one sent in the original query.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffQueryResponse {
    pub message_id: Vec<u8>,
    pub error: Option<String>,
    pub tables: Vec<OneOffTable>,
    pub total_host_execution_duration_micros: u64,
}

/// A table included as part of a one-off query.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub struct OneOffTable {
    pub table_name: String,
    pub rows: Vec<Row>,
}
