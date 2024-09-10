#![allow(unused)]

use spacetimedb_sdk::{
    anyhow,
    callbacks::DbCallbacks,
    client_cache::ClientCache,
    db_connection::DbContextImpl,
    spacetime_module::{self as impl_traits, InModule, SpacetimeModule, TableUpdate},
    spacetimedb_lib::{de::Deserialize, ser::Serialize},
    subscription::{SubscriptionBuilder, SubscriptionHandleImpl},
    DbConnectionBuilder, DbContext, Event,
};

pub mod message_table;
pub mod message_type;
pub mod send_message_reducer;
pub mod set_name_reducer;
pub mod user_table;
pub mod user_type;

pub use message_table::*;
pub use message_type::*;
pub use send_message_reducer::*;
pub use set_name_reducer::*;
pub use user_table::*;
pub use user_type::*;

pub struct RemoteModule;

impl InModule for RemoteModule {
    type Module = Self;
}

impl SpacetimeModule for RemoteModule {
    type DbConnection = DbConnection;
    type EventContext = EventContext;
    type Reducer = Reducer;
    type DbView = RemoteTables;
    type Reducers = RemoteReducers;
    type DbUpdate = DbUpdate;
    type SubscriptionHandle = SubscriptionHandle;
}

#[allow(unused)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Reducer {
    SendMessage(send_message_reducer::SendMessage),
    SetName(set_name_reducer::SetName),
}

impl TryFrom<spacetimedb_sdk::ws_messages::ReducerCallInfo> for Reducer {
    type Error = spacetimedb_sdk::anyhow::Error;
    fn try_from(value: spacetimedb_sdk::ws_messages::ReducerCallInfo) -> Result<Self, Self::Error> {
        match &value.reducer_name[..] {
            "send_message" => Ok(Self::SendMessage(impl_traits::parse_reducer_args(
                "send_message",
                &value.args,
            )?)),
            "set_name" => Ok(Self::SetName(impl_traits::parse_reducer_args("set_name", &value.args)?)),
            _ => Err(anyhow::anyhow!("Unknown reducer {:?}", value.reducer_name)),
        }
    }
}

impl InModule for Reducer {
    type Module = RemoteModule;
}

impl impl_traits::Reducer for Reducer {
    fn reducer_name(&self) -> &'static str {
        match self {
            Self::SendMessage(_) => "send_message",
            Self::SetName(_) => "set_name",
        }
    }
    fn reducer_args(&self) -> &dyn std::any::Any {
        match self {
            Self::SendMessage(args) => args,
            Self::SetName(args) => args,
        }
    }
}

pub struct RemoteReducers {
    imp: DbContextImpl<RemoteModule>,
}

impl InModule for RemoteReducers {
    type Module = RemoteModule;
}

pub struct RemoteTables {
    imp: DbContextImpl<RemoteModule>,
}

impl InModule for RemoteTables {
    type Module = RemoteModule;
}

pub struct DbConnection {
    pub db: RemoteTables,
    pub reducers: RemoteReducers,

    imp: DbContextImpl<RemoteModule>,
}

impl InModule for DbConnection {
    type Module = RemoteModule;
}

impl DbContext for DbConnection {
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

    type SubscriptionBuilder = SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {
        SubscriptionBuilder::new(&self.imp)
    }
}

impl DbConnection {
    pub fn builder() -> DbConnectionBuilder<RemoteModule> {
        DbConnectionBuilder::new()
    }

    pub fn advance_one_message(&self) -> anyhow::Result<bool> {
        self.imp.advance_one_message()
    }

    pub fn advance_one_message_blocking(&self) -> anyhow::Result<()> {
        self.imp.advance_one_message_blocking()
    }

    pub async fn advance_one_message_async(&self) -> anyhow::Result<()> {
        self.imp.advance_one_message_async().await
    }

    pub fn frame_tick(&self) -> anyhow::Result<()> {
        self.imp.frame_tick()
    }

    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {
        self.imp.run_threaded()
    }

    pub async fn run_async(&self) -> anyhow::Result<()> {
        self.imp.run_async().await
    }
}

impl impl_traits::DbConnection for DbConnection {
    fn new(imp: DbContextImpl<Self::Module>) -> Self {
        Self {
            db: RemoteTables { imp: imp.clone() },
            reducers: RemoteReducers { imp: imp.clone() },
            imp,
        }
    }
}

pub struct EventContext {
    pub db: RemoteTables,
    pub reducers: RemoteReducers,
    pub event: Event<Reducer>,
    imp: DbContextImpl<RemoteModule>,
}

impl InModule for EventContext {
    type Module = RemoteModule;
}

impl DbContext for EventContext {
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

    type SubscriptionBuilder = SubscriptionBuilder<RemoteModule>;

    fn subscription_builder(&self) -> Self::SubscriptionBuilder {
        SubscriptionBuilder::new(&self.imp)
    }
}

impl impl_traits::EventContext for EventContext {
    fn event(&self) -> &Event<Reducer> {
        &self.event
    }
    fn new(imp: DbContextImpl<RemoteModule>, event: Event<Reducer>) -> Self {
        Self {
            db: RemoteTables { imp: imp.clone() },
            reducers: RemoteReducers { imp: imp.clone() },
            event,
            imp,
        }
    }
}

#[derive(Default)]
pub struct DbUpdate {
    user: TableUpdate<User>,
    message: TableUpdate<Message>,
}

impl TryFrom<spacetimedb_sdk::ws_messages::DatabaseUpdate> for DbUpdate {
    type Error = spacetimedb_sdk::anyhow::Error;
    fn try_from(raw: spacetimedb_sdk::ws_messages::DatabaseUpdate) -> Result<Self, Self::Error> {
        let mut db_update = DbUpdate::default();
        for table_update in raw.tables {
            match &table_update.table_name[..] {
                "User" => db_update.user = user_table::parse_table_update(table_update.deletes, table_update.inserts)?,
                "Message" => {
                    db_update.message = message_table::parse_table_update(table_update.deletes, table_update.inserts)?
                }
                unknown => spacetimedb_sdk::anyhow::bail!("Unknown table {unknown:?} in DatabaseUpdate"),
            }
        }
        Ok(db_update)
    }
}

impl InModule for DbUpdate {
    type Module = RemoteModule;
}

impl impl_traits::DbUpdate for DbUpdate {
    fn apply_to_client_cache(&self, cache: &mut ClientCache<RemoteModule>) {
        cache.apply_diff_to_table::<User>("User", &self.user);
        cache.apply_diff_to_table::<Message>("Message", &self.message);
    }
    fn invoke_row_callbacks(&self, event: &EventContext, callbacks: &mut DbCallbacks<RemoteModule>) {
        callbacks.invoke_table_row_callbacks::<User>("User", &self.user, event);
        callbacks.invoke_table_row_callbacks::<Message>("Message", &self.message, event);
    }
}

pub struct SubscriptionHandle {
    imp: SubscriptionHandleImpl<RemoteModule>,
}

impl InModule for SubscriptionHandle {
    type Module = RemoteModule;
}

impl impl_traits::SubscriptionHandle for SubscriptionHandle {
    fn new(imp: SubscriptionHandleImpl<RemoteModule>) -> Self {
        Self { imp }
    }
}
