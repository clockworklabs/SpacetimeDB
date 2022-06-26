use std::{
    collections::HashMap,
    sync::{Arc, Mutex}, time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    clients::{client_connection_index::ClientActorId, module_subscription_actor::ModuleSubscription},
    db::{relational_db::RelationalDB, transactional_db::Tx, messages::write::Write},
    hash::Hash,
};
use tokio::sync::{mpsc, oneshot};
use wasmer::{imports, Array, Function, Instance, LazyInit, Module, Store, WasmPtr};
use wasmer_middlewares::metering::{get_remaining_points, set_remaining_points, MeteringPoints};

use super::instance_env::InstanceEnv;

#[derive(Debug, Clone)]
pub enum EventStatus {
    Committed(Vec<Write>),
    Failed,
}

#[derive(Debug, Clone)]
pub struct ModuleFunctionCall {
    pub reducer: String,
    pub arg_bytes: Vec<u8>
}

#[derive(Debug, Clone)]
pub struct ModuleEvent {
    pub timestamp: u64,
    pub caller_identity: String, // hex identity
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus, 
}

#[derive(Debug)]
enum ModuleHostCommand {
    CallReducer {
        reducer_name: String,
        arg_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    InitDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    MigrateDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    AddSubscriber {
        client_id: ClientActorId,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    Exit {},
}

#[derive(Debug, Clone)]
pub struct ModuleHost {
    tx: mpsc::Sender<ModuleHostCommand>,
}

impl ModuleHost {
    pub fn spawn(identity: Hash, name: String, module_hash: Hash, module: Module, store: Store) -> ModuleHost {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut actor = ModuleHostActor::new(identity, name, module_hash, module, store);
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
                    break;
                }
            }
        });
        ModuleHost { tx }
    }

    pub async fn call_reducer(&self, reducer_name: String, arg_bytes: Vec<u8>) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallReducer {
                reducer_name,
                arg_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn init_database(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx.send(ModuleHostCommand::InitDatabase { respond_to: tx }).await?;
        rx.await.unwrap()
    }

    pub async fn migrate_database(&self) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::MigrateDatabase { respond_to: tx })
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
}

const REDUCE_DUNDER: &str = "__reducer__";
const INIT_PANIC_DUNDER: &str = "__init_panic__";
const CREATE_TABLE_DUNDER: &str = "__create_table__";
const MIGRATE_DATABASE_DUNDER: &str = "__migrate_database__";

fn get_remaining_points_value(instance: &Instance) -> u64 {
    let remaining_points = get_remaining_points(instance);
    let remaining_points = match remaining_points {
        MeteringPoints::Remaining(x) => x,
        MeteringPoints::Exhausted => 0,
    };
    return remaining_points;
}

struct ModuleHostActor {
    identity: Hash,
    name: String,
    _module_hash: Hash,
    module: Module,
    store: Store,
    instances: Vec<(u32, Instance)>,
    instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
    relational_db: Arc<Mutex<RelationalDB>>,
    subscription: ModuleSubscription,
}

impl ModuleHostActor {
    pub fn new(identity: Hash, name: String, module_hash: Hash, module: Module, store: Store) -> Self {
        let hex_identity = hex::encode(identity);
        let relational_db = Arc::new(Mutex::new(RelationalDB::open(format!(
            "/stdb/dbs/{hex_identity}/{name}"
        ))));
        let subscription = ModuleSubscription::spawn(relational_db.clone());
        let mut host = Self {
            relational_db,
            identity,
            name,
            module,
            // TODO
            instance_tx_map: Arc::new(Mutex::new(HashMap::new())),
            _module_hash: module_hash,
            store,
            instances: Vec::new(),
            subscription,
        };
        host.create_instance().unwrap();
        host
    }

    fn handle_message(&mut self, message: ModuleHostCommand) -> bool {
        match message {
            ModuleHostCommand::CallReducer {
                reducer_name,
                arg_bytes,
                respond_to,
            } => {
                respond_to.send(self.call_reducer(&reducer_name, &arg_bytes)).unwrap();
                false
            }
            ModuleHostCommand::InitDatabase { respond_to } => {
                respond_to.send(self.init_database()).unwrap();
                false
            }
            ModuleHostCommand::MigrateDatabase { respond_to } => {
                respond_to.send(self.migrate_database()).unwrap();
                false
            }
            ModuleHostCommand::Exit {} => true,
            ModuleHostCommand::AddSubscriber { client_id, respond_to } => {
                respond_to.send(self.add_subscriber(client_id)).unwrap();
                false
            }
        }
    }

    fn create_instance(&mut self) -> Result<u32, anyhow::Error> {
        let instance_id = self.instances.len() as u32;
        let identity = self.identity;
        let name = self.name.clone();
        let env = InstanceEnv {
            instance_id,
            identity,
            name,
            relational_db: self.relational_db.clone(),
            instance_tx_map: self.instance_tx_map.clone(),
            memory: LazyInit::new(),
            alloc: LazyInit::new(),
        };
        let import_object = imports! {
            "env" => {
                "_insert" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::insert,
                ),
                "_create_table" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::create_table,
                ),
                "_iter" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::iter
                ),
                "_console_log" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::console_log
                ),
            }
        };

        let instance = Instance::new(&self.module, &import_object)?;
        let points = 1_000_000;
        set_remaining_points(&instance, points);

        // Init panic if available
        let init_panic = instance.exports.get_native_function::<(), ()>(INIT_PANIC_DUNDER);
        if let Some(init_panic) = init_panic.ok() {
            let _ = init_panic.call();
        }

        self.instances.push((instance_id, instance));
        Ok(instance_id)
    }

    fn init_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            if !f.name().starts_with(CREATE_TABLE_DUNDER) {
                continue;
            }
            self.execute_reducer(f.name(), Vec::new())?;
        }

        // TODO: call __create_index__IndexName
        Ok(())
    }

    fn migrate_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            if !f.name().starts_with(MIGRATE_DATABASE_DUNDER) {
                continue;
            }
            self.execute_reducer(f.name(), Vec::new())?;
        }

        // TODO: call __create_index__IndexName
        Ok(())
    }

    fn call_reducer(&self, reducer_name: &str, arg_bytes: &[u8]) -> Result<(), anyhow::Error> {
        // TODO: validate arg_bytes
        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);
        self.execute_reducer(&reducer_symbol, arg_bytes)?;

        Ok(())
    }

    fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.subscription.add_subscriber(client_id)
    }

    fn execute_reducer(&self, reducer_symbol: &str, arg_bytes: impl AsRef<[u8]>) -> Result<(), anyhow::Error> {
        // TODO: disallow calling non-reducer dunder functions
        let tx = {
            let mut stdb = self.relational_db.lock().unwrap();
            stdb.begin_tx()
        };

        // TODO: choose one at random or whatever
        let (instance_id, instance) = &self.instances[0];
        self.instance_tx_map.lock().unwrap().insert(*instance_id, tx);

        let points = 1_000_000;
        set_remaining_points(&instance, points);

        // Prepare arguments
        let memory = instance.exports.get_memory("memory").unwrap();
        let alloc = instance
            .exports
            .get_function("alloc")?
            .native::<u32, WasmPtr<u8, Array>>()?;

        let arg_bytes = arg_bytes.as_ref();
        let ptr = alloc.call(arg_bytes.len() as u32).unwrap();

        let values = ptr.deref(memory, 0, arg_bytes.len() as u32).unwrap();
        for (i, byte) in arg_bytes.iter().enumerate() {
            values[i].set(*byte);
        }

        let reduce = instance
            .exports
            .get_function(&reducer_symbol)?
            .native::<(u32, u32), ()>()?;

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);
        let result = reduce.call(ptr.offset(), arg_bytes.len() as u32);
        let duration = start.elapsed();
        let remaining_points = get_remaining_points_value(&instance);
        log::trace!(
            "Reducer \"{}\" ran: {} us, {} eV",
            reducer_symbol,
            duration.as_micros(),
            points - remaining_points
        );

        if let Some(err) = result.err() {
            let mut stdb = self.relational_db.lock().unwrap();
            let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
            let tx = instance_tx_map.remove(&instance_id).unwrap();
            stdb.rollback_tx(tx);

            let e = &err;
            let frames = e.trace();
            let frames_len = frames.len();

            log::info!("Reducer \"{}\" runtime error: {}", reducer_symbol, e.message());
            for i in 0..frames_len {
                log::info!(
                    "  Frame #{}: {:?}::{:?}",
                    frames_len - i,
                    frames[i].module_name(),
                    frames[i].function_name().or(Some("<func>")).unwrap()
                );
            }
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            let event = ModuleEvent {
                timestamp,
                caller_identity: "TODO".to_owned(),
                function_call: ModuleFunctionCall { reducer: reducer_symbol.to_string(), arg_bytes: arg_bytes.to_owned() },
                status: EventStatus::Failed,
            };
            self.subscription.publish_event(event).unwrap();
        } else {
            let mut stdb = self.relational_db.lock().unwrap();
            let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
            let tx = instance_tx_map.remove(&instance_id).unwrap();
            if let Some(tx) = stdb.commit_tx(tx) {
                stdb.txdb.message_log.sync_all().unwrap();
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64;
                let event = ModuleEvent {
                    timestamp,
                    caller_identity: "TODO".to_owned(),
                    function_call: ModuleFunctionCall { reducer: reducer_symbol.to_string(), arg_bytes: arg_bytes.to_owned() },
                    status: EventStatus::Committed(tx.writes),
                };
                self.subscription.publish_event(event).unwrap();
            } else {
                todo!("Write skew, you need to implement retries my man, T-dawg.");
            }
        }
        Ok(())
    }
}
