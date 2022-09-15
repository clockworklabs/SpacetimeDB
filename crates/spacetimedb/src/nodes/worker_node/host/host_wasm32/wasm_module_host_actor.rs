use anyhow::anyhow;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{spawn, time::sleep};
use wasmer::{imports, Array, Function, Instance, LazyInit, Module, Store, Value, WasmPtr};
use wasmer_middlewares::metering::{get_remaining_points, set_remaining_points, MeteringPoints};

use spacetimedb_bindings::args::{Arguments, ConnectDisconnectArguments, ReducerArguments, RepeatingReducerArguments};
use spacetimedb_bindings::buffer::VectorBufWriter;
use spacetimedb_bindings::{ElementDef, TupleDef, TypeDef};

use crate::db::{messages::transaction::Transaction, transactional_db::Tx};
use crate::hash::Hash;
use crate::nodes::worker_node::host::host_controller::{DescribedEntityType, Entity, EntityDescription};
use crate::nodes::worker_node::host::host_wasm32::wasm_instance_env::WasmInstanceEnv;
use crate::nodes::worker_node::host::instance_env::InstanceEnv;
use crate::nodes::worker_node::host::module_host::{
    EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHost, ModuleHostActor, ModuleHostCommand,
};
use crate::nodes::worker_node::{
    client_api::{client_connection_index::ClientActorId, module_subscription_actor::ModuleSubscription},
    worker_database_instance::WorkerDatabaseInstance,
};

const REDUCE_DUNDER: &str = "__reducer__";
const DESCRIBE_REDUCER_DUNDER: &str = "__describe_reducer__";

const REPEATING_REDUCER_DUNDER: &str = "__repeating_reducer__";
// TODO(ryan): not actually used, since we don't really need to call a describe for repeating
// reducers as the arguments are always the same. However I'm leaving it here for consistency in
// the DescribedEntity interface below, and also in case we ever need user arguments on
// repeaters.
const DESCRIBE_REPEATING_REDUCER_DUNDER: &str = "__describe_repeating_reducer__";

const CREATE_TABLE_DUNDER: &str = "__create_table__";
const DESCRIBE_TABLE_DUNDER: &str = "__describe_table__";

const INIT_PANIC_DUNDER: &str = "__init_panic__";
const MIGRATE_DATABASE_DUNDER: &str = "__migrate_database__";
const IDENTITY_CONNECTED_DUNDER: &str = "__identity_connected__";
const IDENTITY_DISCONNECTED_DUNDER: &str = "__identity_disconnected__";

fn get_remaining_points_value(instance: &Instance) -> u64 {
    let remaining_points = get_remaining_points(instance);
    let remaining_points = match remaining_points {
        MeteringPoints::Remaining(x) => x,
        MeteringPoints::Exhausted => 0,
    };
    return remaining_points;
}

fn entity_as_prefix_str(entity: &DescribedEntityType) -> &str {
    match entity {
        DescribedEntityType::Table => DESCRIBE_TABLE_DUNDER,
        DescribedEntityType::Reducer => DESCRIBE_REDUCER_DUNDER,
        DescribedEntityType::RepeatingReducer => DESCRIBE_REPEATING_REDUCER_DUNDER,
    }
}

fn entity_from_function_name(fn_name: &str) -> Option<DescribedEntityType> {
    if fn_name.starts_with(DESCRIBE_TABLE_DUNDER) {
        Some(DescribedEntityType::Table)
    } else if fn_name.starts_with(DESCRIBE_REDUCER_DUNDER) {
        Some(DescribedEntityType::Reducer)
    } else if fn_name.starts_with(DESCRIBE_REPEATING_REDUCER_DUNDER) {
        Some(DescribedEntityType::RepeatingReducer)
    } else {
        None
    }
}

pub(crate) struct WasmModuleHostActor {
    worker_database_instance: WorkerDatabaseInstance,
    module_host: ModuleHost,
    _module_hash: Hash,
    module: Module,
    store: Store,
    instances: Vec<(u32, Instance)>,
    instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
    subscription: ModuleSubscription,

    // Holds the list of descriptions of each entity.
    // TODO(ryan): Long run let's replace or augment this with catalog table(s) that hold the
    // schema. Then standard table query tools could be run against it.
    description_cache: HashMap<Entity, TupleDef>,
}

impl WasmModuleHostActor {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        module_hash: Hash,
        module: Module,
        store: Store,
        module_host: ModuleHost,
    ) -> Self {
        let relational_db = worker_database_instance.relational_db.clone();
        let subscription = ModuleSubscription::spawn(relational_db);
        let mut host = Self {
            worker_database_instance,
            module_host,
            module,
            instance_tx_map: Arc::new(Mutex::new(HashMap::new())),
            _module_hash: module_hash,
            store,
            instances: Vec::new(),
            subscription,
            description_cache: HashMap::new(),
        };
        host.create_instance().unwrap();
        host.populate_description_caches()
            .expect("Unable to populate description cache");
        host
    }

    fn populate_description_caches(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            let desc_entity_type = match entity_from_function_name(f.name()) {
                None => continue,
                Some(desc_entity_type) => desc_entity_type,
            };
            // Special case for repeaters.
            let (entity_name, description) = if desc_entity_type == DescribedEntityType::RepeatingReducer {
                let entity_name = f.name().strip_prefix(REPEATING_REDUCER_DUNDER).unwrap();
                let description = TupleDef {
                    elements: vec![
                        ElementDef {
                            tag: 0,
                            name: Some(String::from("timestamp")),
                            element_type: TypeDef::U64,
                        },
                        ElementDef {
                            tag: 1,
                            name: Some(String::from("delta_time")),
                            element_type: TypeDef::U64,
                        },
                    ],
                };
                (entity_name, description)
            } else {
                let prefix = entity_as_prefix_str(&desc_entity_type);
                let entity_name = f.name().strip_prefix(prefix).unwrap();
                let description = self.call_describer(String::from(f.name()))?;
                let description = match description {
                    None => return Err(anyhow!("Bad describe function returned None; {}", f.name())),
                    Some(description) => description,
                };
                (entity_name, description)
            };

            let entity = Entity {
                entity_name: String::from(entity_name),
                entity_type: desc_entity_type,
            };
            self.description_cache.insert(entity, description);
        }
        Ok(())
    }

    fn create_instance(&mut self) -> Result<u32, anyhow::Error> {
        let instance_id = self.instances.len() as u32;
        let env = WasmInstanceEnv {
            instance_env: InstanceEnv {
                worker_database_instance: self.worker_database_instance.clone(),
                instance_id,
                instance_tx_map: self.instance_tx_map.clone(),
            },
            memory: LazyInit::new(),
            alloc: LazyInit::new(),
        };
        let import_object = imports! {
            "env" => {
                "_delete_pk" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::delete_pk,
                ),
                "_delete_value" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::delete_value,
                ),
                "_delete_eq" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::delete_eq,
                ),
                "_delete_range" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::delete_range,
                ),
                "_insert" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::insert,
                ),
                "_create_table" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::create_table,
                ),
                "_get_table_id" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::get_table_id,
                ),
                "_iter" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::iter
                ),
                "_console_log" => Function::new_native_with_env(
                    &self.store,
                    env.clone(),
                    WasmInstanceEnv::console_log
                ),
            }
        };

        let instance = Instance::new(&self.module, &import_object)?;
        let points = 1_000_000_000_000;
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
                    }
                };
                let module_host = self.module_host.clone();
                let mut prev_call_time = call_timestamp;
                let mut cur_repeat_duration = repeat_duration;
                spawn(async move {
                    loop {
                        sleep(Duration::from_millis(cur_repeat_duration)).await;
                        let res = module_host.call_repeating_reducer(name.clone(), prev_call_time).await;
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
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
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

    fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.subscription.remove_subscriber(client_id)
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
        let arguments = ReducerArguments::new(
            spacetimedb_bindings::Hash::from_arr(&caller_identity.data),
            timestamp,
            Vec::from(arg_bytes),
        );

        // TODO: It's possible to push this down further into execute_reducer, and write directly
        // into the WASM memory, but ModuleEvent.function_call also wants a copy, so it doesn't
        // quite work.
        let mut new_arg_bytes = Vec::with_capacity(arguments.encoded_size());
        let mut writer = VectorBufWriter::new(&mut new_arg_bytes);
        arguments.encode(&mut writer);

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
        let reducer_symbol = format!("{}{}", REPEATING_REDUCER_DUNDER, reducer_name);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let delta_time = timestamp - prev_call_time;
        let arguments = RepeatingReducerArguments::new(timestamp, delta_time);

        let mut arg_bytes = Vec::with_capacity(arguments.encoded_size());
        let mut writer = VectorBufWriter::new(&mut arg_bytes);
        arguments.encode(&mut writer);

        let (tx, repeat_duration) = self.execute_reducer(&reducer_symbol, &arg_bytes)?;

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity: self.worker_database_instance.identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok((repeat_duration.unwrap_or(delta_time), timestamp))
    }

    fn call_identity_connected_disconnected(&self, identity: &Hash, connected: bool) -> Result<(), anyhow::Error> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let arguments =
            ConnectDisconnectArguments::new(spacetimedb_bindings::Hash::from_arr(&identity.data), timestamp);

        let mut new_arg_bytes = Vec::with_capacity(arguments.encoded_size());
        let mut writer = VectorBufWriter::new(&mut new_arg_bytes);
        arguments.encode(&mut writer);

        let reducer_symbol = if connected {
            IDENTITY_CONNECTED_DUNDER
        } else {
            IDENTITY_DISCONNECTED_DUNDER
        };

        let result = self.execute_reducer(reducer_symbol, new_arg_bytes);
        let tx = match result {
            Ok((tx, _repeat_duration)) => tx,
            Err(err) => {
                log::debug!("Error with connect/disconnect: {}", err);
                return Ok(());
            }
        };

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        // TODO(cloutiertyler): We need to think about how to handle this special
        // function. Is this just an autogenerated reducer? In the future if I call
        // a reducer from within a reducer should that generate a module event?
        // Good question, Tyler, good question.
        let event = ModuleEvent {
            timestamp,
            function_call: ModuleFunctionCall {
                reducer: reducer_symbol.to_string(),
                arg_bytes: Vec::new(),
            },
            status,
            caller_identity: *identity,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }

    fn catalog(&self) -> Vec<EntityDescription> {
        self.description_cache
            .iter()
            .map(|k| EntityDescription {
                entity: k.0.clone(),
                schema: k.1.clone(),
            })
            .collect()
    }

    fn describe(&self, entity: &Entity) -> Option<TupleDef> {
        self.description_cache.get(entity).map(|t| t.clone())
    }

    fn call_describer(&self, describer_func_name: String) -> Result<Option<TupleDef>, anyhow::Error> {
        // TODO: choose one at random or whatever
        let (_instance_id, instance) = &self.instances[0];
        let describer = match instance.exports.get_function(&describer_func_name) {
            Ok(describer) => describer,
            Err(_) => {
                // Making the bold assumption here that an error here means this entity doesn't exist.
                return Ok(None);
            }
        };

        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let result = describer.call(&[]);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        match result {
            Err(err) => {
                let e = &err;
                let frames = e.trace();
                let frames_len = frames.len();

                log::info!("Describer\"{}\" runtime error: {}", describer_func_name, e.message());
                for i in 0..frames_len {
                    log::info!(
                        "  Frame #{}: {:?}::{:?}",
                        frames_len - i,
                        frames[i].module_name(),
                        frames[i].function_name().or(Some("<func>")).unwrap()
                    );
                }
                Err(anyhow!("Could not invoke describer function {}", describer_func_name))
            }
            Ok(ret) => {
                if ret.is_empty() {
                    return Err(anyhow!("Invalid return buffer arguments from {}", describer_func_name));
                }

                // The return value of the describer is a pointer to a vector.
                // The upper 32 bits of the 64-bit result is the offset into memory.
                // The lower 32 bits is its length
                let return_value = ret.first().unwrap().i64().unwrap() as usize;
                let offset = return_value >> 32;
                let length = return_value & 0xffffffff;

                // We have to copy all the memory out in order to use this.
                // This would be nice to avoid... and just somehow pass the memory contents directly
                // through to the TupleDef decode, but Wasmer's use of Cell prevents us from getting
                // a nice contiguous block of bytes?
                let memory = instance.exports.get_memory("memory").unwrap();
                let view = memory.view::<u8>();
                let bytes: Vec<u8> = view[offset..offset + length].iter().map(|c| c.get()).collect();

                // Decode the memory as TupleDef. Do not exit yet, as we have to dealloc the buffer.
                let (args, _) = TupleDef::decode(bytes);
                let result = match args {
                    Ok(args) => args,
                    Err(e) => {
                        return Err(anyhow!(
                            "argument tuples has invalid schema: {} Err: {}",
                            describer_func_name,
                            e
                        ))
                    }
                };

                // Clean out the vector buffer memory that the wasm-side "forgot" in order to pass
                // it to us.
                // TODO(ryan): way to generalize this to some RAII thing?
                let dealloc = instance
                    .exports
                    .get_function("dealloc")?
                    .native::<(WasmPtr<u8, Array>, u32), ()>()?;
                let dealloc_result = dealloc.call(WasmPtr::new(offset as u32), length as u32);
                dealloc_result.expect("Could not dealloc describer buffer memory");

                Ok(Some(result))
            }
        }
    }

    fn execute_reducer(
        &self,
        reducer_symbol: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<(Option<Transaction>, Option<u64>), anyhow::Error> {
        let tx = {
            let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
            stdb.begin_tx()
        };

        // TODO: choose one at random or whatever
        let (instance_id, instance) = &self.instances[0];
        self.instance_tx_map.lock().unwrap().insert(*instance_id, tx);

        let points = 1_000_000_000_000;
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
                let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
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
                let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
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

impl ModuleHostActor for WasmModuleHostActor {
    fn handle_message(&mut self, message: ModuleHostCommand) -> bool {
        match message {
            ModuleHostCommand::CallConnectDisconnect {
                caller_identity,
                connected,
                respond_to,
            } => {
                respond_to
                    .send(self.call_identity_connected_disconnected(&caller_identity, connected))
                    .unwrap();
                false
            }
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
            ModuleHostCommand::_MigrateDatabase { respond_to } => {
                respond_to.send(self.migrate_database()).unwrap();
                false
            }
            ModuleHostCommand::Exit {} => true,
            ModuleHostCommand::AddSubscriber { client_id, respond_to } => {
                respond_to.send(self.add_subscriber(client_id)).unwrap();
                false
            }
            ModuleHostCommand::RemoveSubscriber { client_id, respond_to } => {
                respond_to.send(self.remove_subscriber(client_id)).unwrap();
                false
            }
            ModuleHostCommand::StartRepeatingReducers => {
                self.start_repeating_reducers();
                false
            }
            ModuleHostCommand::Catalog { respond_to } => {
                respond_to.send(self.catalog()).unwrap();
                false
            }
            ModuleHostCommand::Describe { entity, respond_to } => {
                respond_to.send(self.describe(&entity)).unwrap();
                false
            }
        }
    }
}
