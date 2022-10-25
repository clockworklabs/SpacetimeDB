use crate::client::ClientActorId;
use crate::db::messages::write::Write;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult};
use anyhow::Context;
use spacetimedb_lib::EntityDef;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

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
    pub timestamp: u64,
    pub caller_identity: Hash,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: i64,
    pub host_execution_duration: Duration,
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
        args: ReducerArgs,
        respond_to: oneshot::Sender<Result<ReducerCallResult, anyhow::Error>>,
    },
    CallRepeatingReducer {
        id: usize,
        prev_call_time: u64,
        respond_to: oneshot::Sender<Result<(u64, u64), anyhow::Error>>,
    },
    GetRepeatingReducers {
        respond_to: oneshot::Sender<Vec<(String, usize)>>,
    },
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
        entity_name: String,
        respond_to: oneshot::Sender<Option<EntityDef>>,
    },
    Catalog {
        respond_to: oneshot::Sender<Vec<(String, EntityDef)>>,
    },
    #[cfg(feature = "tracelogging")]
    GetTrace {
        respond_to: oneshot::Sender<Option<bytes::Bytes>>,
    },
    #[cfg(feature = "tracelogging")]
    StopTrace {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
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
    pub fn spawn<F>(identity: Hash, make_actor_fn: F) -> anyhow::Result<ModuleHost>
    where
        F: FnOnce(ModuleHost) -> Result<Box<dyn ModuleHostActor + Send>, anyhow::Error>,
    {
        let (tx, mut rx) = mpsc::channel(8);
        let inner_tx = tx.clone();
        let module_host = ModuleHost { identity, tx: inner_tx };
        let mut actor = make_actor_fn(module_host).context("Unable to instantiate ModuleHostActor")?;
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
                    break;
                }
            }
        });
        Ok(ModuleHost { identity, tx })
    }

    async fn call<T>(&self, f: impl FnOnce(oneshot::Sender<T>) -> ModuleHostCommand) -> anyhow::Result<T> {
        let (tx, rx) = oneshot::channel();
        // TODO: is it worth it to bubble up? if send/rx fails it means that the task panicked.
        //       we should either panic or respawn it
        self.tx.send(f(tx)).await?;
        rx.await.context("sender dropped")
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
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, anyhow::Error> {
        self.call(|tx| ModuleHostCommand::CallReducer {
            caller_identity,
            reducer_name,
            budget,
            args,
            respond_to: tx,
        })
        .await?
    }

    async fn get_repeating_reducers(&self) -> anyhow::Result<Vec<(String, usize)>> {
        self.call(|tx| ModuleHostCommand::GetRepeatingReducers { respond_to: tx })
            .await
    }

    async fn call_repeating_reducer(&self, id: usize, prev_call_time: u64) -> Result<(u64, u64), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(u64, u64), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallRepeatingReducer {
                id,
                prev_call_time,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn start_repeating_reducers(&self) -> Result<(), anyhow::Error> {
        let repeaters = self.get_repeating_reducers().await?;
        for (name, id) in repeaters {
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
            let mut prev_call_time = timestamp - 20;

            let module_host = self.clone();
            tokio::spawn(async move {
                loop {
                    match module_host.call_repeating_reducer(id, prev_call_time).await {
                        Ok((repeat_duration, call_timestamp)) => {
                            prev_call_time = call_timestamp;
                            tokio::time::sleep(Duration::from_millis(repeat_duration)).await;
                        }
                        Err(err) => {
                            // If we get an error trying to call this, then the module host has probably restarted
                            // just break out of the loop and end this task
                            // TODO: is the above correct?
                            log::debug!("Error calling repeating reducer {name}: {}", err);
                            break;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    pub async fn describe(&self, entity_name: String) -> Result<Option<EntityDef>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ModuleHostCommand::Describe {
                entity_name,
                respond_to: tx,
            })
            .await?;
        rx.await.map_err(anyhow::Error::new)
    }

    pub async fn catalog(&self) -> Result<Vec<(String, EntityDef)>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(ModuleHostCommand::Catalog { respond_to: tx }).await?;
        rx.await.map_err(anyhow::Error::new)
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

    #[cfg(feature = "tracelogging")]
    pub async fn get_trace(&self) -> Result<Option<bytes::Bytes>, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Option<bytes::Bytes>>();
        self.tx.send(ModuleHostCommand::GetTrace { respond_to: tx }).await?;
        rx.await.map_err(|e| anyhow::Error::new(e))
    }

    #[cfg(feature = "tracelogging")]
    pub async fn stop_trace(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx.send(ModuleHostCommand::StopTrace { respond_to: tx }).await?;
        rx.await.unwrap()
    }
}
