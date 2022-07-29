use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    clients::{client_connection_index::ClientActorId, module_subscription_actor::ModuleSubscription},
    db::{
        messages::{transaction::Transaction, write::Write},
        relational_db::RelationalDB,
        transactional_db::Tx,
    },
    hash::Hash,
};
use tokio::{
    spawn,
    sync::{mpsc, oneshot},
    time::sleep,
};
use wasmer::{imports, Array, Function, Instance, LazyInit, Module, Store, Value, WasmPtr};
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
    pub arg_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ModuleEvent {
    pub timestamp: u64,
    pub caller_identity: Hash,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
}

#[derive(Debug)]
enum ModuleHostCommand {
    CallReducer {
        caller_identity: Hash,
        reducer_name: String,
        arg_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
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
        let inner_tx = tx.clone();
        tokio::spawn(async move {
            let module_host = ModuleHost { tx: inner_tx };
            let mut actor = ModuleHostActor::new(identity, name, module_hash, module, store, module_host);
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
                    break;
                }
            }
        });
        ModuleHost { tx }
    }

    pub async fn call_reducer(
        &self,
        caller_identity: Hash,
        reducer_name: String,
        arg_bytes: Vec<u8>,
    ) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(ModuleHostCommand::CallReducer {
                caller_identity,
                reducer_name,
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
const REPEATING_REDUCER_DUNDER: &str = "__repeating_reducer__";
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
    module_host: ModuleHost,
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
    pub fn new(
        identity: Hash,
        name: String,
        module_hash: Hash,
        module: Module,
        store: Store,
        module_host: ModuleHost,
    ) -> Self {
        let hex_identity = hex::encode(identity);
        let relational_db = Arc::new(Mutex::new(RelationalDB::open(format!(
            "/stdb/dbs/{hex_identity}/{name}"
        ))));
        let subscription = ModuleSubscription::spawn(relational_db.clone());
        let mut host = Self {
            module_host,
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
                caller_identity,
                reducer_name,
                arg_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.call_reducer(caller_identity, &reducer_name, &arg_bytes))
                    .unwrap();
                false
            }
            ModuleHostCommand::CallRepeatingReducer {
                reducer_name,
                prev_call_time,
                respond_to,
            } => {
                respond_to
                    .send(self.call_repeating_reducer(&reducer_name, prev_call_time))
                    .unwrap();
                false
            }
            ModuleHostCommand::InitDatabase { respond_to } => {
                respond_to.send(self.init_database()).unwrap();
                false
            }
            ModuleHostCommand::DeleteDatabase { respond_to } => {
                respond_to.send(self.delete_database()).unwrap();
                true
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
            ModuleHostCommand::StartRepeatingReducers => {
                self.start_repeating_reducers();
                false
            },
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
                "_delete_pk" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::delete_pk,
                ),
                "_delete_value" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::delete_value,
                ),
                "_delete_eq" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::delete_eq,
                ),
                "_delete_range" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    InstanceEnv::delete_range,
                ),
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

    fn start_repeating_reducers(&mut self) {
        for f in self.module.exports().functions() {
            if f.name().starts_with(REPEATING_REDUCER_DUNDER) {
                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
                let prev_call_time = timestamp - 20;

                // TODO: We should really have another function inside of the module that we can use to get the initial repeat
                // duration. It doesn't make sense to just make up a random value here.
                let name = f.name()[REPEATING_REDUCER_DUNDER.len()..].to_string();
                let result = self.call_repeating_reducer(&name, prev_call_time);
                let (repeat_duration, call_timestamp) = match result {
                    Ok((repeat_duration, call_timestamp)) => (repeat_duration, call_timestamp),
                    Err(err) => {
                        log::warn!("Error in repeating reducer: {}", err);
                        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
                        (20, timestamp)
                    },
                };
                let module_host = self.module_host.clone();
                let mut prev_call_time = call_timestamp;
                let mut cur_repeat_duration = repeat_duration;
                spawn(async move {
                    loop {
                        sleep(Duration::from_millis(cur_repeat_duration)).await;
                        let res = module_host
                            .call_repeating_reducer(name.clone(), prev_call_time)
                            .await;
                        if let Err(err) = res {
                            // If we get an error trying to call this, then the module host has probably restarted
                            // just break out of the loop and end this task
                            log::debug!("Error calling repeating reducer: {}", err);
                            break;
                        }
                        if let Ok((repeat_duration, call_timestamp)) = res {
                            prev_call_time = call_timestamp;
                            cur_repeat_duration = repeat_duration;
                        }
                    }
                });
            }
        }
    }

    fn init_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            if f.name().starts_with(CREATE_TABLE_DUNDER) {
                self.call_create_table(&f.name()[CREATE_TABLE_DUNDER.len()..])?;
            }
        }

        // TODO: call __create_index__IndexName

        Ok(())
    }

    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.relational_db.lock().unwrap();
        stdb.reset_hard()?;
        Ok(())
    }

    fn migrate_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            if !f.name().starts_with(MIGRATE_DATABASE_DUNDER) {
                continue;
            }
            self.call_migrate(&f.name()[MIGRATE_DATABASE_DUNDER.len()..])?;
        }

        // TODO: call __create_index__IndexName
        Ok(())
    }

    fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.subscription.add_subscriber(client_id)
    }

    fn call_create_table(&self, create_table_name: &str) -> Result<(), anyhow::Error> {
        let create_table_symbol = format!("{}{}", CREATE_TABLE_DUNDER, create_table_name);
        let (_tx, _repeat_duration) = self.execute_reducer(&create_table_symbol, &[])?;
        Ok(())
    }

    fn call_migrate(&self, migrate_name: &str) -> Result<(), anyhow::Error> {
        let migrate_symbol = format!("{}{}", MIGRATE_DATABASE_DUNDER, migrate_name);
        let (_tx, _repeat_duration) = self.execute_reducer(&migrate_symbol, &[])?;
        Ok(())
    }

    fn call_reducer(&self, caller_identity: Hash, reducer_name: &str, arg_bytes: &[u8]) -> Result<(), anyhow::Error> {
        // TODO: validate arg_bytes
        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;

        let mut new_arg_bytes = Vec::with_capacity(40 + arg_bytes.len());
        for b in caller_identity {
            new_arg_bytes.push(b);
        }

        let timestamp_buf = timestamp.to_le_bytes();
        for b in timestamp_buf {
            new_arg_bytes.push(b)
        }

        for b in arg_bytes {
            new_arg_bytes.push(*b);
        }

        let (tx, _repeat_duration) = self.execute_reducer(&reducer_symbol, new_arg_bytes)?;

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }

    fn call_repeating_reducer(&self, reducer_name: &str, prev_call_time: u64) -> Result<(u64, u64), anyhow::Error> {
        // TODO: validate arg_bytes
        let reducer_symbol = format!("{}{}", REPEATING_REDUCER_DUNDER, reducer_name);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;

        let mut arg_bytes = Vec::with_capacity(16);

        let timestamp_buf = timestamp.to_le_bytes();
        for b in timestamp_buf {
            arg_bytes.push(b)
        }

        let delta_time = timestamp - prev_call_time;
        let delta_time_buf = delta_time.to_le_bytes();
        for b in delta_time_buf {
            arg_bytes.push(b);
        }

        let (tx, repeat_duration) = self.execute_reducer(&reducer_symbol, &arg_bytes)?;

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity: self.identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok((repeat_duration.unwrap(), timestamp))
    }

    fn execute_reducer(
        &self,
        reducer_symbol: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<(Option<Transaction>, Option<u64>), anyhow::Error> {
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
        let buf_len = arg_bytes.len() as u32;
        let ptr = alloc.call(buf_len).unwrap();
        let values = ptr.deref(memory, 0, buf_len).unwrap();
        for (i, b) in arg_bytes.iter().enumerate() {
            values[i].set(*b);
        }

        let reduce = instance.exports.get_function(&reducer_symbol)?;

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);
        let result = reduce.call(&[Value::I32(ptr.offset() as i32), Value::I32(buf_len as i32)]);
        let duration = start.elapsed();
        let remaining_points = get_remaining_points_value(&instance);
        log::trace!(
            "Reducer \"{}\" ran: {} us, {} eV",
            reducer_symbol,
            duration.as_micros(),
            points - remaining_points
        );

        match result {
            Err(err) => {
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
                Ok((None, None))
            }
            Ok(ret) => {
                let repeat_duration = if ret.len() == 1 {
                    ret.first().unwrap().i64().map(|i| i as u64)
                } else {
                    None
                };
                let mut stdb = self.relational_db.lock().unwrap();
                let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
                let tx = instance_tx_map.remove(&instance_id).unwrap();
                if let Some(tx) = stdb.commit_tx(tx) {
                    stdb.txdb.message_log.sync_all().unwrap();
                    Ok((Some(tx), repeat_duration))
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        }
    }
}
