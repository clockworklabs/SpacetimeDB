use crate::client::ClientActorId;
use crate::db::messages::write::Write;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult};
use crate::module_subscription_actor::ModuleSubscription;
use anyhow::Context;
use spacetimedb_lib::{EntityDef, TupleValue};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use super::timestamp::Timestamp;
use super::ReducerArgs;

#[derive(Debug, Clone)]
pub enum EventStatus {
    Committed(Vec<Write>),
    Failed,
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
    pub caller_identity: Hash,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: i64,
    pub host_execution_duration: Duration,
}

#[derive(Debug)]
enum ModuleHostCommand {
    CallConnectDisconnect {
        caller_identity: Hash,
        connected: bool,
        respond_to: oneshot::Sender<()>,
    },
    CallReducer {
        caller_identity: Hash,
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
            ModuleHostCommand::AddSubscriber { client_id, respond_to } => {
                let _ = respond_to.send(actor.subscription().add_subscriber(client_id));
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
    pub identity: Hash,
    pub module_hash: Hash,
    pub catalog: HashMap<String, EntityDef>,
}

pub trait ModuleHostActor: Send + 'static {
    fn info(&self) -> Arc<ModuleInfo>;
    fn subscription(&self) -> &ModuleSubscription;
    fn call_connect_disconnect(&mut self, caller_identity: Hash, connected: bool, respond_to: oneshot::Sender<()>);
    fn call_reducer(
        &mut self,
        caller_identity: Hash,
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
        caller_identity: Hash,
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
        caller_identity: Hash,
        reducer_name: String,
        budget: ReducerBudget,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let Some(EntityDef::Reducer(schema)) = self.info().catalog.get(&reducer_name) else { return Ok(None) };
        let args = args.into_tuple(schema)?;
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
        let args = match self.info().catalog.get("__init__") {
            Some(EntityDef::Reducer(schema)) => args.into_tuple(schema)?,
            _ => TupleValue { elements: Box::new([]) },
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

    pub async fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.call(|respond_to| ModuleHostCommand::AddSubscriber { client_id, respond_to })
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
impl Deref for Catalog {
    type Target = HashMap<String, EntityDef>;
    fn deref(&self) -> &Self::Target {
        &self.0.catalog
    }
}
