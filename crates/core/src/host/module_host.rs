use crate::client::ClientActorId;
use crate::db::messages::write::Write;
use crate::db::relational_db::RelationalDBWrapper;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult};
use crate::identity::Identity;
use crate::json::client_api::{SubscriptionUpdateJson, TableRowOperationJson, TableUpdateJson};
use crate::protobuf::client_api::{table_row_operation, SubscriptionUpdate, TableRowOperation, TableUpdate};
use crate::subscription::module_subscription_actor::ModuleSubscriptionManager;
use anyhow::Context;
use spacetimedb_lib::{EntityDef, ReducerDef, TableDef, TupleDef, TupleValue};
use spacetimedb_sats::ProductValue;
use spacetimedb_sats::{TypeInSpace, Typespace};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use super::timestamp::Timestamp;
use super::ReducerArgs;

#[derive(Debug, Clone)]
pub struct DatabaseUpdate {
    pub tables: Vec<DatabaseTableUpdate>,
}

impl DatabaseUpdate {
    pub fn from_writes(relational_db: &RelationalDBWrapper, writes: &Vec<Write>) -> Self {
        let mut schemas: HashMap<u32, TupleDef> = HashMap::new();
        let mut map: HashMap<u32, Vec<TableOp>> = HashMap::new();
        for write in writes {
            let op = match write.operation {
                crate::db::messages::write::Operation::Delete => 0,
                crate::db::messages::write::Operation::Insert => 1,
            };

            let tuple_def = if let Some(tuple_def) = schemas.get(&write.set_id) {
                tuple_def
            } else {
                let mut stdb = relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let tuple_def = stdb.schema_for_table(tx, write.set_id).unwrap().unwrap();
                tx_.rollback();
                schemas.insert(write.set_id, tuple_def);
                schemas.get(&write.set_id).unwrap()
            };

            let vec = if let Some(vec) = map.get_mut(&write.set_id) {
                vec
            } else {
                map.insert(write.set_id, Vec::new());
                map.get_mut(&write.set_id).unwrap()
            };

            let (row, row_pk) = {
                let stdb = relational_db.lock().unwrap();
                let tuple = stdb
                    .txdb
                    .from_data_key(&write.data_key, |data| TupleValue::decode(tuple_def, &mut { data }))
                    .unwrap();
                let tuple = match tuple {
                    Ok(tuple) => tuple,
                    Err(e) => {
                        log::error!("Failed to decode row: {}", e);
                        continue;
                    }
                };

                (tuple, write.data_key.to_bytes())
            };

            vec.push(TableOp {
                op_type: op,
                row_pk,
                row,
            });
        }

        let mut table_name_map: HashMap<u32, String> = HashMap::new();
        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            let table_name = if let Some(name) = table_name_map.get(&table_id) {
                name.clone()
            } else {
                let mut stdb = relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let table_name = stdb.table_name_from_id(tx, table_id).unwrap().unwrap();
                let table_name = table_name.to_string();
                tx_.rollback();
                table_name_map.insert(table_id, table_name.clone());
                table_name
            };
            table_updates.push(DatabaseTableUpdate {
                table_id,
                table_name,
                ops: table_row_operations,
            });
        }

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

#[derive(Debug, Clone)]
pub struct ModuleFunctionCall {
    pub reducer: String,
    pub arg_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ModuleEvent {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: i64,
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
        reducer_name: String,
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<ReducerCallResult>,
    },
    InitDatabase {
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<Result<Option<ReducerCallResult>, anyhow::Error>>,
    },
    DeleteDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    _MigrateDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    AddSubscriber {
        client_id: ClientActorId,
        query_string: String,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    RemoveSubscriber {
        client_id: ClientActorId,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    #[cfg(feature = "tracelogging")]
    GetTrace {
        respond_to: oneshot::Sender<Option<bytes::Bytes>>,
    },
    #[cfg(feature = "tracelogging")]
    StopTrace {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
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
                reducer_name,
                budget,
                args,
                respond_to,
            } => actor.call_reducer(caller_identity, reducer_name, budget, args, respond_to),
            ModuleHostCommand::InitDatabase {
                budget,
                args,
                respond_to,
            } => actor.init_database(budget, args, respond_to),
            ModuleHostCommand::DeleteDatabase { respond_to } => {
                let _ = respond_to.send(actor.delete_database());
            }
            ModuleHostCommand::_MigrateDatabase { respond_to } => actor._migrate_database(respond_to),
            ModuleHostCommand::AddSubscriber {
                client_id,
                query_string,
                respond_to,
            } => {
                let _ = respond_to.send(actor.subscription().add_subscriber(client_id, query_string));
            }
            ModuleHostCommand::RemoveSubscriber { client_id, respond_to } => {
                let _ = respond_to.send(actor.subscription().remove_subscriber(client_id));
            }
            #[cfg(feature = "tracelogging")]
            ModuleHostCommand::GetTrace { respond_to } => {
                let _ = respond_to.send(actor.get_trace());
            }
            #[cfg(feature = "tracelogging")]
            ModuleHostCommand::StopTrace { respond_to } => {
                let _ = respond_to.send(actor.stop_trace());
            }
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
    pub catalog: HashMap<String, EntityDef>,
    pub log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
}

pub trait ModuleHostActor: Send + 'static {
    fn info(&self) -> Arc<ModuleInfo>;
    fn subscription(&self) -> &ModuleSubscriptionManager;
    fn call_connect_disconnect(&mut self, caller_identity: Identity, connected: bool, respond_to: oneshot::Sender<()>);
    fn call_reducer(
        &mut self,
        caller_identity: Identity,
        reducer_name: String,
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<ReducerCallResult>,
    );
    fn init_database(
        &mut self,
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<Result<Option<ReducerCallResult>, anyhow::Error>>,
    );
    fn delete_database(&mut self) -> Result<(), anyhow::Error>;
    fn _migrate_database(&mut self, respond_to: oneshot::Sender<Result<(), anyhow::Error>>);
    #[cfg(feature = "tracelogging")]
    fn get_trace(&self) -> Option<bytes::Bytes>;
    #[cfg(feature = "tracelogging")]
    fn stop_trace(&mut self) -> Result<(), anyhow::Error>;
}

#[derive(Debug, Clone)]
pub struct ModuleHost {
    info: Arc<ModuleInfo>,
    tx: mpsc::Sender<CmdOrExit>,
}

impl ModuleHost {
    pub fn spawn(actor: Box<impl ModuleHostActor>) -> Self {
        let (tx, rx) = mpsc::channel(8);
        let info = actor.info();
        tokio::task::spawn_blocking(|| Self::run_actor(rx, actor));
        ModuleHost { info, tx }
    }

    #[allow(clippy::boxed_local)] // I don't wanna risk passing on stack
    fn run_actor(mut rx: mpsc::Receiver<CmdOrExit>, mut actor: Box<impl ModuleHostActor>) {
        let actor = &mut *actor;
        while let Some(command) = rx.blocking_recv() {
            match command {
                CmdOrExit::Cmd(command) => command.dispatch(actor),
                CmdOrExit::Exit => rx.close(),
            }
        }
    }

    #[inline]
    pub fn info(&self) -> &ModuleInfo {
        &self.info
    }

    async fn call<T>(&self, f: impl FnOnce(oneshot::Sender<T>) -> ModuleHostCommand) -> anyhow::Result<T> {
        let permit = self.tx.reserve().await.context("module closed")?;
        let (tx, rx) = oneshot::channel();
        permit.send(CmdOrExit::Cmd(f(tx)));
        // TODO: is it worth it to bubble up? if rx fails it means that the task panicked.
        //       we should either panic or respawn it
        rx.await.context("sender dropped")
    }

    pub async fn call_identity_connected_disconnected(
        &self,
        caller_identity: Identity,
        connected: bool,
    ) -> Result<(), anyhow::Error> {
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
        reducer_name: String,
        budget: ReducerBudget,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let args = match self.catalog().get_reducer(&reducer_name) {
            Some(schema) => args.into_tuple(schema)?,
            None => return Ok(None),
        };
        self.call(|respond_to| ModuleHostCommand::CallReducer {
            caller_identity,
            reducer_name,
            budget,
            args,
            respond_to,
        })
        .await
        .map(Some)
    }

    pub fn catalog(&self) -> Catalog {
        Catalog(self.info.clone())
    }

    pub async fn init_database(
        &self,
        budget: ReducerBudget,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let args = match self.catalog().get_reducer("__init__") {
            Some(schema) => args.into_tuple(schema)?,
            _ => TupleValue { elements: vec![] },
        };
        self.call(|respond_to| ModuleHostCommand::InitDatabase {
            budget,
            args,
            respond_to,
        })
        .await?
    }

    pub async fn delete_database(&self) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::DeleteDatabase { respond_to })
            .await?
    }

    pub async fn _migrate_database(&self) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::_MigrateDatabase { respond_to })
            .await?
    }

    pub async fn exit(&self) -> Result<(), anyhow::Error> {
        self.tx.send(CmdOrExit::Exit).await?;
        self.tx.closed().await;
        Ok(())
    }

    pub async fn add_subscriber(&self, client_id: ClientActorId, query_string: String) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::AddSubscriber {
            client_id,
            query_string,
            respond_to,
        })
        .await?
    }

    pub async fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::RemoveSubscriber { client_id, respond_to })
            .await?
    }

    #[cfg(feature = "tracelogging")]
    pub async fn get_trace(&self) -> Result<Option<bytes::Bytes>, anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::GetTrace { respond_to }).await
    }

    #[cfg(feature = "tracelogging")]
    pub async fn stop_trace(&self) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::StopTrace { respond_to })
            .await?
    }
}

pub struct Catalog(Arc<ModuleInfo>);
impl Catalog {
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
