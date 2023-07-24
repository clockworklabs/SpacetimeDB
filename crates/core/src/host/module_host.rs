use crate::client::ClientConnectionSender;
use crate::database_logger::LogLevel;
use crate::db::datastore::traits::{TableId, TxData, TxOp};
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::hash::Hash;
use crate::identity::Identity;
use crate::json::client_api::{SubscriptionUpdateJson, TableRowOperationJson, TableUpdateJson};
use crate::protobuf::client_api::{table_row_operation, SubscriptionUpdate, TableRowOperation, TableUpdate};
use crate::subscription::module_subscription_actor::ModuleSubscriptionManager;
use indexmap::IndexMap;
use spacetimedb_lib::{ReducerDef, TableDef};
use spacetimedb_sats::{ProductValue, TypeInSpace, Typespace};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use super::{ArgsTuple, EnergyDiff, InvalidReducerArguments, ReducerArgs, ReducerCallResult, Timestamp};

#[derive(Debug, Default, Clone)]
pub struct DatabaseUpdate {
    pub tables: Vec<DatabaseTableUpdate>,
}

impl DatabaseUpdate {
    pub fn is_empty(&self) -> bool {
        if self.tables.len() == 0 {
            return true;
        }
        false
    }

    pub fn from_writes(stdb: &RelationalDB, tx_data: &TxData) -> Self {
        let mut map: HashMap<TableId, Vec<TableOp>> = HashMap::new();
        //TODO: This should be wrapped with .auto_commit
        let tx = stdb.begin_tx();
        for record in tx_data.records.iter() {
            let op = match record.op {
                TxOp::Delete => 0,
                TxOp::Insert(_) => 1,
            };

            let vec = if let Some(vec) = map.get_mut(&record.table_id) {
                vec
            } else {
                map.insert(record.table_id, Vec::new());
                map.get_mut(&record.table_id).unwrap()
            };

            let (row, row_pk) = (record.product_value.clone(), record.key.to_bytes());

            vec.push(TableOp {
                op_type: op,
                row_pk,
                row,
            });
        }

        let mut table_name_map: HashMap<TableId, String> = HashMap::new();
        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            let table_name = if let Some(name) = table_name_map.get(&table_id) {
                name.clone()
            } else {
                let table_name = stdb.table_name_from_id(&tx, table_id.0).unwrap().unwrap();
                table_name_map.insert(table_id, table_name.clone());
                table_name
            };
            table_updates.push(DatabaseTableUpdate {
                table_id: table_id.0,
                table_name,
                ops: table_row_operations,
            });
        }
        stdb.rollback_tx(tx);

        DatabaseUpdate { tables: table_updates }
    }

    pub fn into_protobuf(self) -> SubscriptionUpdate {
        SubscriptionUpdate {
            table_updates: self
                .tables
                .into_iter()
                .map(|table| TableUpdate {
                    table_id: table.table_id,
                    table_name: table.table_name,
                    table_row_operations: table
                        .ops
                        .into_iter()
                        .map(|op| {
                            let mut row_bytes = Vec::new();
                            op.row.encode(&mut row_bytes);
                            TableRowOperation {
                                op: if op.op_type == 1 {
                                    table_row_operation::OperationType::Insert.into()
                                } else {
                                    table_row_operation::OperationType::Delete.into()
                                },
                                row_pk: op.row_pk,
                                row: row_bytes,
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn into_json(self) -> SubscriptionUpdateJson {
        // For all tables, push all state
        // TODO: We need some way to namespace tables so we don't send all the internal tables and stuff
        SubscriptionUpdateJson {
            table_updates: self
                .tables
                .into_iter()
                .map(|table| TableUpdateJson {
                    table_id: table.table_id,
                    table_name: table.table_name,
                    table_row_operations: table
                        .ops
                        .into_iter()
                        .map(|op| {
                            let row_pk = base64::encode(&op.row_pk);
                            TableRowOperationJson {
                                op: if op.op_type == 1 {
                                    "insert".into()
                                } else {
                                    "delete".into()
                                },
                                row_pk,
                                row: op.row.elements,
                            }
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseTableUpdate {
    pub table_id: u32,
    pub table_name: String,
    pub ops: Vec<TableOp>,
}

#[derive(Debug, Clone)]
pub struct TableOp {
    pub op_type: u8,
    pub row_pk: Vec<u8>,
    pub row: ProductValue,
}

#[derive(Debug, Clone)]
pub enum EventStatus {
    Committed(DatabaseUpdate),
    Failed(String),
    OutOfEnergy,
}

impl EventStatus {
    pub fn database_update(&self) -> Option<&DatabaseUpdate> {
        match self {
            EventStatus::Committed(upd) => Some(upd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleFunctionCall {
    pub reducer: String,
    pub args: ArgsTuple,
}

#[derive(Debug, Clone)]
pub struct ModuleEvent {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: EnergyDiff,
    pub host_execution_duration: Duration,
}

#[derive(Debug)]
enum ModuleHostCommand {
    CallConnectDisconnect {
        caller_identity: Identity,
        connected: bool,
        respond_to: oneshot::Sender<()>,
    },
    CallReducer {
        caller_identity: Identity,
        client: Option<ClientConnectionSender>,
        reducer_id: usize,
        args: ArgsTuple,
        respond_to: oneshot::Sender<ReducerCallResult>,
    },
    InitDatabase {
        args: ArgsTuple,
        respond_to: oneshot::Sender<anyhow::Result<ReducerCallResult>>,
    },
    UpdateDatabase {
        respond_to: oneshot::Sender<Result<UpdateDatabaseResult, anyhow::Error>>,
    },
    #[cfg(feature = "tracelogging")]
    GetTrace {
        respond_to: oneshot::Sender<Option<bytes::Bytes>>,
    },
    #[cfg(feature = "tracelogging")]
    StopTrace {
        respond_to: oneshot::Sender<anyhow::Result<()>>,
    },
    InjectLogs {
        respond_to: oneshot::Sender<()>,
        log_level: LogLevel,
        message: String,
    },
}

impl ModuleHostCommand {
    fn dispatch<T: ModuleHostActor + ?Sized>(self, actor: &mut T) {
        match self {
            ModuleHostCommand::CallConnectDisconnect {
                caller_identity,
                connected,
                respond_to,
            } => actor.call_connect_disconnect(caller_identity, connected, respond_to),
            ModuleHostCommand::CallReducer {
                caller_identity,
                client,
                reducer_id,
                args,
                respond_to,
            } => actor.call_reducer(caller_identity, client, reducer_id, args, respond_to),
            ModuleHostCommand::InitDatabase { args, respond_to } => actor.init_database(args, respond_to),
            ModuleHostCommand::UpdateDatabase { respond_to } => actor.update_database(respond_to),
            #[cfg(feature = "tracelogging")]
            ModuleHostCommand::GetTrace { respond_to } => {
                let _ = respond_to.send(actor.get_trace());
            }
            #[cfg(feature = "tracelogging")]
            ModuleHostCommand::StopTrace { respond_to } => {
                let _ = respond_to.send(actor.stop_trace());
            }
            ModuleHostCommand::InjectLogs { respond_to, log_level, message } => actor.inject_logs(respond_to, log_level, message),
        }
    }
}

#[derive(Debug)]
enum CmdOrExit {
    Cmd(ModuleHostCommand),
    Exit,
}

#[derive(Debug)]
pub struct ModuleInfo {
    pub identity: Identity,
    pub module_hash: Hash,
    pub typespace: Typespace,
    pub reducers: IndexMap<String, ReducerDef>,
    pub catalog: HashMap<String, EntityDef>,
    pub log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
    pub subscription: ModuleSubscriptionManager,
}

pub trait ModuleHostActor: Send + 'static {
    fn info(&self) -> Arc<ModuleInfo>;
    fn call_connect_disconnect(&mut self, caller_identity: Identity, connected: bool, respond_to: oneshot::Sender<()>);
    fn call_reducer(
        &mut self,
        caller_identity: Identity,
        client: Option<ClientConnectionSender>,
        reducer_id: usize,
        args: ArgsTuple,
        respond_to: oneshot::Sender<ReducerCallResult>,
    );
    fn init_database(&mut self, args: ArgsTuple, respond_to: oneshot::Sender<Result<ReducerCallResult, anyhow::Error>>);
    fn update_database(&mut self, respond_to: oneshot::Sender<Result<UpdateDatabaseResult, anyhow::Error>>);
    #[cfg(feature = "tracelogging")]
    fn get_trace(&self) -> Option<bytes::Bytes>;
    #[cfg(feature = "tracelogging")]
    fn stop_trace(&mut self) -> Result<(), anyhow::Error>;
    fn inject_logs(&self, respond_to: oneshot::Sender<()>, log_level: LogLevel, message: String);
    fn close(self);
}

#[derive(Debug, Clone)]
pub struct ModuleHost {
    info: Arc<ModuleInfo>,
    tx: mpsc::Sender<CmdOrExit>,
}

pub struct WeakModuleHost {
    info: Arc<ModuleInfo>,
    tx: mpsc::WeakSender<CmdOrExit>,
}

pub type UpdateDatabaseResult = Result<UpdateDatabaseSuccess, UpdateDatabaseError>;

#[derive(Debug)]
pub struct UpdateDatabaseSuccess {
    /// Outcome of calling the module's __update__ reducer, `None` if none is
    /// defined.
    pub update_result: Option<ReducerCallResult>,
    /// Outcome of calling the module's pending __migrate__ reducers, empty if
    /// none are defined or pending.
    ///
    /// Currently always empty, as __migrate__ is not yet supported.
    pub migrate_results: Vec<ReducerCallResult>,
}

#[derive(thiserror::Error, Debug)]
pub enum UpdateDatabaseError {
    #[error("incompatible schema changes for: {tables:?}")]
    IncompatibleSchema { tables: Vec<String> },
    #[error(transparent)]
    Database(#[from] DBError),
}

#[derive(thiserror::Error, Debug)]
#[error("no such module")]
pub struct NoSuchModule;

#[derive(thiserror::Error, Debug)]
pub enum ReducerCallError {
    #[error(transparent)]
    Args(#[from] InvalidReducerArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error("no such reducer")]
    NoSuchReducer,
}

#[derive(thiserror::Error, Debug)]
pub enum InitDatabaseError {
    #[error(transparent)]
    Args(#[from] InvalidReducerArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error(transparent)]
    Other(anyhow::Error),
}

pub struct ModuleStarter {
    tx: oneshot::Sender<Infallible>,
}

impl ModuleStarter {
    pub fn start(self) {
        drop(self.tx)
    }
}

impl ModuleHost {
    pub fn spawn(actor: impl ModuleHostActor) -> (Self, ModuleStarter) {
        let (tx, rx) = mpsc::channel(8);
        let (start_tx, start_rx) = oneshot::channel();
        let info = actor.info();
        tokio::task::spawn_blocking(|| {
            let _ = start_rx.blocking_recv();
            Self::run_actor(rx, actor)
        });
        (ModuleHost { info, tx }, ModuleStarter { tx: start_tx })
    }

    fn run_actor(mut rx: mpsc::Receiver<CmdOrExit>, mut actor: impl ModuleHostActor) {
        while let Some(command) = rx.blocking_recv() {
            match command {
                CmdOrExit::Cmd(command) => command.dispatch(&mut actor),
                CmdOrExit::Exit => rx.close(),
            }
        }
        actor.close()
    }

    #[inline]
    pub fn info(&self) -> &ModuleInfo {
        &self.info
    }

    #[inline]
    pub fn subscription(&self) -> &ModuleSubscriptionManager {
        &self.info.subscription
    }

    async fn call<T>(&self, f: impl FnOnce(oneshot::Sender<T>) -> ModuleHostCommand) -> Result<T, NoSuchModule> {
        let permit = self.tx.reserve().await.map_err(|_| NoSuchModule)?;
        let (tx, rx) = oneshot::channel();
        permit.send(CmdOrExit::Cmd(f(tx)));
        Ok(rx.await.expect("task panicked"))
    }

    pub async fn call_identity_connected_disconnected(
        &self,
        caller_identity: Identity,
        connected: bool,
    ) -> Result<(), NoSuchModule> {
        self.call(|respond_to| ModuleHostCommand::CallConnectDisconnect {
            caller_identity,
            connected,
            respond_to,
        })
        .await
    }

    pub async fn call_reducer(
        &self,
        caller_identity: Identity,
        client: Option<ClientConnectionSender>,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let found_reducer = self
            .info
            .reducers
            .get_full(reducer_name)
            .ok_or(ReducerCallError::NoSuchReducer);
        let (reducer_id, _, schema) = match found_reducer {
            Ok(ok) => ok,
            Err(err) => {
                let _ = self.inject_logs(LogLevel::Error, format!(
                    "External attempt to call nonexistent reducer \"{}\" failed. Have you run `spacetime generate` recently?",
                    reducer_name
                )).await;
                Err(err)?
            }
        };

        let args = args.into_tuple(self.info.typespace.with_type(schema));
        let args = match args {
            Ok(ok) => ok,
            Err(err) => {
                let _ = self.inject_logs(LogLevel::Error, format!(
                    "External attempt to call reducer \"{}\" failed, invalid arguments.\nThis is likely due to a mismatched client schema, have you run `spacetime generate` recently?",
                    reducer_name,
                )).await;
                Err(err)?
            }
        };

        self.call(|respond_to| ModuleHostCommand::CallReducer {
            caller_identity,
            client,
            reducer_id,
            args,
            respond_to,
        })
        .await
        .map_err(Into::into)
    }

    pub fn catalog(&self) -> Catalog {
        Catalog(self.info.clone())
    }

    pub fn subscribe_to_logs(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.info().log_tx.subscribe())
    }

    pub async fn init_database(&self, args: ReducerArgs) -> Result<ReducerCallResult, InitDatabaseError> {
        let args = match self.catalog().get_reducer("__init__") {
            Some(schema) => args.into_tuple(schema)?,
            _ => ArgsTuple::default(),
        };
        self.call(|respond_to| ModuleHostCommand::InitDatabase { args, respond_to })
            .await?
            .map_err(InitDatabaseError::Other)
    }

    pub async fn update_database(&self) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::UpdateDatabase { respond_to })
            .await?
            .map_err(Into::into)
    }

    pub async fn exit(&self) {
        // if we can't send, it's already closed :P
        if self.tx.send(CmdOrExit::Exit).await.is_ok() {
            self.tx.closed().await;
        }
    }

    pub async fn exited(&self) {
        self.tx.closed().await
    }

    #[cfg(feature = "tracelogging")]
    pub async fn get_trace(&self) -> Result<Option<bytes::Bytes>, NoSuchModule> {
        self.call(|respond_to| ModuleHostCommand::GetTrace { respond_to }).await
    }

    #[cfg(feature = "tracelogging")]
    pub async fn stop_trace(&self) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::StopTrace { respond_to })
            .await?
    }

    pub async fn inject_logs(&self, log_level: LogLevel, message: String) -> Result<(), NoSuchModule> {
        self.call(|respond_to| ModuleHostCommand::InjectLogs { respond_to, log_level, message })
            .await
    }

    pub fn downgrade(&self) -> WeakModuleHost {
        WeakModuleHost {
            info: self.info.clone(),
            tx: self.tx.downgrade(),
        }
    }
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        let tx = self.tx.upgrade()?;
        Some(ModuleHost {
            info: self.info.clone(),
            tx,
        })
    }
}

#[derive(Debug)]
pub enum EntityDef {
    Reducer(ReducerDef),
    Table(TableDef),
}
impl EntityDef {
    pub fn as_reducer(&self) -> Option<&ReducerDef> {
        match self {
            Self::Reducer(x) => Some(x),
            _ => None,
        }
    }
    pub fn as_table(&self) -> Option<&TableDef> {
        match self {
            Self::Table(x) => Some(x),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Catalog(Arc<ModuleInfo>);
impl Catalog {
    pub fn typespace(&self) -> &Typespace {
        &self.0.typespace
    }

    pub fn get(&self, name: &str) -> Option<TypeInSpace<'_, EntityDef>> {
        self.0.catalog.get(name).map(|ty| self.0.typespace.with_type(ty))
    }
    pub fn get_reducer(&self, name: &str) -> Option<TypeInSpace<'_, ReducerDef>> {
        let schema = self.get(name)?;
        Some(schema.with(schema.ty().as_reducer()?))
    }
    pub fn get_table(&self, name: &str) -> Option<TypeInSpace<'_, TableDef>> {
        let schema = self.get(name)?;
        Some(schema.with(schema.ty().as_table()?))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, TypeInSpace<'_, EntityDef>)> + '_ {
        self.0
            .catalog
            .iter()
            .map(|(name, e)| (&**name, self.0.typespace.with_type(e)))
    }
}
