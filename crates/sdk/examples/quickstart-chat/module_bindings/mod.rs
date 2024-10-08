// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

pub mod identity_connected_reducer;
pub mod identity_disconnected_reducer;
pub mod init_reducer;
pub mod message_table;
pub mod message_type;
pub mod send_message_reducer;
pub mod set_name_reducer;
pub mod user_table;
pub mod user_type;

pub use identity_connected_reducer::*;
pub use identity_disconnected_reducer::*;
pub use init_reducer::*;
pub use message_table::*;
pub use message_type::*;
pub use send_message_reducer::*;
pub use set_name_reducer::*;
pub use user_table::*;
pub use user_type::*;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]

/// One of the reducers defined by this module.
///
/// Contained within a [`__sdk::ReducerEvent`] in [`EventContext`]s for reducer events
/// to indicate which reducer caused the event.

pub enum Reducer {
    IdentityConnected(identity_connected_reducer::IdentityConnected),
    IdentityDisconnected(identity_disconnected_reducer::IdentityDisconnected),
    Init(init_reducer::Init),
    SendMessage(send_message_reducer::SendMessage),
    SetName(set_name_reducer::SetName),
}

impl __sdk::spacetime_module::InModule for Reducer {
    type Module = RemoteModule;
}

impl __sdk::spacetime_module::Reducer for Reducer {
    fn reducer_name(&self) -> &'static str {
        match self {
            Reducer::IdentityConnected(_) => "__identity_connected__",
            Reducer::IdentityDisconnected(_) => "__identity_disconnected__",
            Reducer::Init(_) => "__init__",
            Reducer::SendMessage(_) => "send_message",
            Reducer::SetName(_) => "set_name",
        }
    }
    fn reducer_args(&self) -> &dyn std::any::Any {
        match self {
            Reducer::IdentityConnected(args) => args,
            Reducer::IdentityDisconnected(args) => args,
            Reducer::Init(args) => args,
            Reducer::SendMessage(args) => args,
            Reducer::SetName(args) => args,
        }
    }
}
impl TryFrom<__ws::ReducerCallInfo<__ws::BsatnFormat>> for Reducer {
    type Error = __anyhow::Error;
    fn try_from(value: __ws::ReducerCallInfo<__ws::BsatnFormat>) -> __anyhow::Result<Self> {
        match &value.reducer_name[..] {
            "__identity_connected__" => Ok(Reducer::IdentityConnected(__sdk::spacetime_module::parse_reducer_args(
                "__identity_connected__",
                &value.args,
            )?)),
            "__identity_disconnected__" => Ok(Reducer::IdentityDisconnected(
                __sdk::spacetime_module::parse_reducer_args("__identity_disconnected__", &value.args)?,
            )),
            "__init__" => Ok(Reducer::Init(__sdk::spacetime_module::parse_reducer_args(
                "__init__",
                &value.args,
            )?)),
            "send_message" => Ok(Reducer::SendMessage(__sdk::spacetime_module::parse_reducer_args(
                "send_message",
                &value.args,
            )?)),
            "set_name" => Ok(Reducer::SetName(__sdk::spacetime_module::parse_reducer_args(
                "set_name",
                &value.args,
            )?)),
            _ => Err(__anyhow::anyhow!("Unknown reducer {:?}", value.reducer_name)),
        }
    }
}

#[derive(Default)]
#[allow(non_snake_case)]
#[doc(hidden)]
pub struct DbUpdate {
    message: __sdk::spacetime_module::TableUpdate<Message>,
    user: __sdk::spacetime_module::TableUpdate<User>,
}

impl TryFrom<__ws::DatabaseUpdate<__ws::BsatnFormat>> for DbUpdate {
    type Error = __anyhow::Error;
    fn try_from(raw: __ws::DatabaseUpdate<__ws::BsatnFormat>) -> Result<Self, Self::Error> {
        let mut db_update = DbUpdate::default();
        for table_update in raw.tables {
            match &table_update.table_name[..] {
                "message" => db_update.message = message_table::parse_table_update(table_update)?,
                "user" => db_update.user = user_table::parse_table_update(table_update)?,

                unknown => __anyhow::bail!("Unknown table {unknown:?} in DatabaseUpdate"),
            }
        }
        Ok(db_update)
    }
}

impl __sdk::spacetime_module::InModule for DbUpdate {
    type Module = RemoteModule;
}

impl __sdk::spacetime_module::DbUpdate for DbUpdate {
    fn apply_to_client_cache(&self, cache: &mut __sdk::client_cache::ClientCache<RemoteModule>) {
        cache.apply_diff_to_table::<Message>("message", &self.message);
        cache.apply_diff_to_table::<User>("user", &self.user);
    }
    fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut __sdk::callbacks::DbCallbacks<RemoteModule>) {
        callbacks.invoke_table_row_callbacks::<Message>("message", &self.message, event);
        callbacks.invoke_table_row_callbacks::<User>("user", &self.user, event);
    }
}

#[doc(hidden)]
pub struct RemoteModule;

impl __sdk::spacetime_module::InModule for RemoteModule {
    type Module = Self;
}

impl __sdk::spacetime_module::SpacetimeModule for RemoteModule {
    type DbConnection = DbConnection;
    type EventContext = EventContext;
    type Reducer = Reducer;
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type DbUpdate = DbUpdate;
    type SubscriptionHandle = SubscriptionHandle;
}

/// The `reducers` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each reducer defined by the module.
pub struct RemoteReducers {
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}

impl __sdk::spacetime_module::InModule for RemoteReducers {
    type Module = RemoteModule;
}

/// The `db` field of [`EventContext`] and [`DbConnection`],
/// with methods provided by extension traits for each table defined by the module.
pub struct RemoteTables {
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}

impl __sdk::spacetime_module::InModule for RemoteTables {
    type Module = RemoteModule;
}

/// A connection to a remote module, including a materialized view of a subset of the database.
///
/// Connect to a remote module by calling [`DbConnection::builder`]
/// and using the [`__sdk::DbConnectionBuilder`] builder-pattern constructor.
///
/// You must explicitly advance the connection by calling any one of:
///
/// - [`DbConnection::frame_tick`].
/// - [`DbConnection::run_threaded`].
/// - [`DbConnection::run_async`].
/// - [`DbConnection::advance_one_message`].
/// - [`DbConnection::advance_one_message_blocking`].
/// - [`DbConnection::advance_one_message_async`].
///
/// Which of these methods you should call depends on the specific needs of your application,
/// but you must call one of them, or else the connection will never progress.
pub struct DbConnection {
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,

    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}

impl __sdk::spacetime_module::InModule for DbConnection {
    type Module = RemoteModule;
}

impl __sdk::db_context::DbContext for DbConnection {
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;

    fn db(&self) -> &Self::DbView {
        &self.db
    }
    fn reducers(&self) -> &Self::Reducers {
        &self.reducers
    }

    fn is_active(&self) -> bool {
        self.imp.is_active()
    }

    fn disconnect(&self) -> __anyhow::Result<()> {
        self.imp.disconnect()
    }

    type SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {
        __sdk::subscription::SubscriptionBuilder::new(&self.imp)
    }

    fn try_identity(&self) -> Option<__sdk::Identity> {
        self.imp.try_identity()
    }
    fn address(&self) -> __sdk::Address {
        self.imp.address()
    }
}

impl DbConnection {
    /// Builder-pattern constructor for a connection to a remote module.
    ///
    /// See [`__sdk::DbConnectionBuilder`] for required and optional configuration for the new connection.
    pub fn builder() -> __sdk::DbConnectionBuilder<RemoteModule> {
        __sdk::db_connection::DbConnectionBuilder::new()
    }

    /// If any WebSocket messages are waiting, process one of them.
    ///
    /// Returns `true` if a message was processed, or `false` if the queue is empty.
    /// Callers should invoke this message in a loop until it returns `false`
    /// or for as much time is available to process messages.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::frame_tick`] each frame
    /// to fully exhaust the queue whenever time is available.
    pub fn advance_one_message(&self) -> __anyhow::Result<bool> {
        self.imp.advance_one_message()
    }

    /// Process one WebSocket message, potentially blocking the current thread until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_threaded`] to spawn a thread
    /// which advances the connection automatically.
    pub fn advance_one_message_blocking(&self) -> __anyhow::Result<()> {
        self.imp.advance_one_message_blocking()
    }

    /// Process one WebSocket message, `await`ing until one is received.
    ///
    /// Returns an error if the connection is disconnected.
    /// If the disconnection in question was normal,
    ///  i.e. the result of a call to [`__sdk::DbContext::disconnect`],
    /// the returned error will be downcastable to [`__sdk::DisconnectedError`].
    ///
    /// This is a low-level primitive exposed for power users who need significant control over scheduling.
    /// Most applications should call [`Self::run_async`] to run an `async` loop
    /// which advances the connection when polled.
    pub async fn advance_one_message_async(&self) -> __anyhow::Result<()> {
        self.imp.advance_one_message_async().await
    }

    /// Process all WebSocket messages waiting in the queue,
    /// then return without `await`ing or blocking the current thread.
    pub fn frame_tick(&self) -> __anyhow::Result<()> {
        self.imp.frame_tick()
    }

    /// Spawn a thread which processes WebSocket messages as they are received.
    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {
        self.imp.run_threaded()
    }

    /// Run an `async` loop which processes WebSocket messages when polled.
    pub async fn run_async(&self) -> __anyhow::Result<()> {
        self.imp.run_async().await
    }
}

impl __sdk::spacetime_module::DbConnection for DbConnection {
    fn new(imp: __sdk::db_connection::DbContextImpl<RemoteModule>) -> Self {
        Self {
            db: RemoteTables { imp: imp.clone() },
            reducers: RemoteReducers { imp: imp.clone() },
            imp,
        }
    }
}

/// A [`DbConnection`] augmented with an [`__sdk::Event`],
/// passed to various callbacks invoked by the SDK.
pub struct EventContext {
    /// Access to tables defined by the module via extension traits implemented for [`RemoteTables`].
    pub db: RemoteTables,
    /// Access to reducers defined by the module via extension traits implemented for [`RemoteReducers`].
    pub reducers: RemoteReducers,
    /// The event which caused these callbacks to run.
    pub event: __sdk::event::Event<Reducer>,
    imp: __sdk::db_connection::DbContextImpl<RemoteModule>,
}

impl __sdk::spacetime_module::InModule for EventContext {
    type Module = RemoteModule;
}

impl __sdk::db_context::DbContext for EventContext {
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;

    fn db(&self) -> &Self::DbView {
        &self.db
    }
    fn reducers(&self) -> &Self::Reducers {
        &self.reducers
    }

    fn is_active(&self) -> bool {
        self.imp.is_active()
    }

    fn disconnect(&self) -> spacetimedb_sdk::anyhow::Result<()> {
        self.imp.disconnect()
    }

    type SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {
        __sdk::subscription::SubscriptionBuilder::new(&self.imp)
    }

    fn try_identity(&self) -> Option<__sdk::Identity> {
        self.imp.try_identity()
    }
    fn address(&self) -> __sdk::Address {
        self.imp.address()
    }
}

impl __sdk::spacetime_module::EventContext for EventContext {
    fn event(&self) -> &__sdk::event::Event<Reducer> {
        &self.event
    }
    fn new(imp: __sdk::db_connection::DbContextImpl<RemoteModule>, event: __sdk::event::Event<Reducer>) -> Self {
        Self {
            db: RemoteTables { imp: imp.clone() },
            reducers: RemoteReducers { imp: imp.clone() },
            event,
            imp,
        }
    }
}

/// A handle on a subscribed query.
// TODO: Document this better after implementing the new subscription API.
pub struct SubscriptionHandle {
    imp: __sdk::subscription::SubscriptionHandleImpl<RemoteModule>,
}

impl __sdk::spacetime_module::InModule for SubscriptionHandle {
    type Module = RemoteModule;
}

impl __sdk::spacetime_module::SubscriptionHandle for SubscriptionHandle {
    fn new(imp: __sdk::subscription::SubscriptionHandleImpl<RemoteModule>) -> Self {
        Self { imp }
    }
}

/// Alias trait for a [`__sdk::DbContext`] connected to this module,
/// with that trait's associated types bounded to this module's concrete types.
///
/// Users can use this trait as a boundary on definitions which should accept
/// either a [`DbConnection`] or an [`EventContext`] and operate on either.
pub trait RemoteDbContext:
    __sdk::DbContext<
    DbView = RemoteTables,
    Reducers = RemoteReducers,
    SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>,
>
{
}
impl<
        Ctx: __sdk::DbContext<
            DbView = RemoteTables,
            Reducers = RemoteReducers,
            SubscriptionBuilder = __sdk::subscription::SubscriptionBuilder<RemoteModule>,
        >,
    > RemoteDbContext for Ctx
{
}
