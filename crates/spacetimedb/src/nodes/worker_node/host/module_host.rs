use crate::db::messages::write::Write;
use crate::hash::Hash;
use crate::nodes::worker_node::client_api::client_connection::ClientActorId;
use crate::nodes::worker_node::host::host_controller::{Entity, EntityDescription, ReducerBudget, ReducerCallResult};
use spacetimedb_bindings::TupleDef;
use tokio::sync::{mpsc, oneshot};

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
    pub timestamp: u64,
    pub caller_identity: Hash,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: i64,
}

#[derive(Debug)]
pub enum ModuleHostCommand {
    CallConnectDisconnect {
        caller_identity: Hash,
        connected: bool,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    CallReducer {
        caller_identity: Hash,
        reducer_name: String,
        budget: ReducerBudget,
        arg_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<ReducerCallResult, anyhow::Error>>,
    },
    CallRepeatingReducer {
        reducer_name: String,
        prev_call_time: u64,
        respond_to: oneshot::Sender<Result<(u64, u64), anyhow::Error>>,
    },
    StartRepeatingReducers,
    InitDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
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
    Describe {
        entity: Entity,
        respond_to: oneshot::Sender<Option<TupleDef>>,
    },
    Catalog {
        respond_to: oneshot::Sender<Vec<EntityDescription>>,
    },
    Exit {},
}

pub trait ModuleHostActor {
    fn handle_message(&mut self, message: ModuleHostCommand) -> bool;
}

#[derive(Debug, Clone)]
pub struct ModuleHost {
    pub identity: Hash,
    tx: mpsc::Sender<ModuleHostCommand>,
}

impl ModuleHost {
    pub fn spawn<F>(identity: Hash, make_actor_fn: F) -> ModuleHost
    where
        F: FnOnce(ModuleHost) -> Result<Box<dyn ModuleHostActor + Send>, anyhow::Error>,
    {
        let (tx, mut rx) = mpsc::channel(8);
        let inner_tx = tx.clone();
        let module_host = ModuleHost { identity, tx: inner_tx };
        let mut actor = make_actor_fn(module_host).expect("Unable to instantiate ModuleHostActor");
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
                    break;
                }
            }
        });
        ModuleHost {identity,  tx }
    }

    pub async fn call_identity_connected_disconnected(
        &self,
        caller_identity: Hash,
        connected: bool,
    ) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallConnectDisconnect {
                caller_identity,
                connected,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn call_reducer(
        &self,
        caller_identity: Hash,
        reducer_name: String,
        budget: ReducerBudget,
        arg_bytes: Vec<u8>,
    ) -> Result<ReducerCallResult, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<ReducerCallResult, anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallReducer {
                caller_identity,
                reducer_name,
                budget,
                arg_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn call_repeating_reducer(
        &self,
        reducer_name: String,
        prev_call_time: u64,
    ) -> Result<(u64, u64), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(u64, u64), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallRepeatingReducer {
                reducer_name,
                prev_call_time,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn start_repeating_reducers(&self) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleHostCommand::StartRepeatingReducers).await?;
        Ok(())
    }

    pub async fn describe(&self, entity: Entity) -> Result<Option<TupleDef>, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Option<TupleDef>>();
        self.tx
            .send(ModuleHostCommand::Describe { entity, respond_to: tx })
            .await?;
        rx.await.map_err(|e| anyhow::Error::new(e))
    }

    pub async fn catalog(&self) -> Result<Vec<EntityDescription>, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Vec<EntityDescription>>();
        self.tx.send(ModuleHostCommand::Catalog { respond_to: tx }).await?;
        rx.await.map_err(|e| anyhow::Error::new(e))
    }

    pub async fn init_database(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx.send(ModuleHostCommand::InitDatabase { respond_to: tx }).await?;
        rx.await.unwrap()
    }

    pub async fn delete_database(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::DeleteDatabase { respond_to: tx })
            .await?;
        rx.await.unwrap()
    }

    pub async fn _migrate_database(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::_MigrateDatabase { respond_to: tx })
            .await?;
        rx.await.unwrap()
    }

    pub async fn exit(&self) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleHostCommand::Exit {}).await?;
        Ok(())
    }

    pub async fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::AddSubscriber {
                client_id,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::RemoveSubscriber {
                client_id,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }
}
