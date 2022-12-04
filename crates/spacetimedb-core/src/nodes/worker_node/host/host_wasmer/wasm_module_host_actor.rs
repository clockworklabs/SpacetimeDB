use super::super::wasm_common::abi;
use super::wasm_instance_env::WasmInstanceEnv;
use super::Mem;
use crate::db::messages::transaction::Transaction;
use crate::db::relational_db::WrapTxWrapper;
use crate::db::transactional_db::CommitResult;
use crate::hash::Hash;
use crate::nodes::worker_node::host::host_controller::{DescribedEntityType, ReducerBudget, ReducerCallResult};
use crate::nodes::worker_node::host::instance_env::InstanceEnv;
use crate::nodes::worker_node::host::module_host::{
    EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHost, ModuleHostActor, ModuleHostCommand,
};
use crate::nodes::worker_node::worker_metrics::{REDUCER_COMPUTE_TIME, REDUCER_COUNT, REDUCER_WRITE_SIZE};
use crate::nodes::worker_node::{
    client_api::{client_connection_index::ClientActorId, module_subscription_actor::ModuleSubscription},
    worker_database_instance::WorkerDatabaseInstance,
};
use anyhow::{anyhow, Context};
use spacetimedb_lib::args::{Arguments, ConnectDisconnectArguments, ReducerArguments, RepeatingReducerArguments};
use spacetimedb_lib::{EntityDef, ReducerDef, RepeaterDef, TableDef};
use std::cell::RefCell;
use std::cmp::max;
use std::time::Instant;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{spawn, time::sleep};
use wasmer::{imports, AsStoreMut, Function, FunctionEnv, Instance, Module, RuntimeError, Store, WasmPtr};
use wasmer_middlewares::metering::{get_remaining_points, set_remaining_points, MeteringPoints};

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

pub const DEFAULT_EXECUTION_BUDGET: i64 = 1_000_000_000_000_000;

fn get_remaining_points_value(ctx: &mut impl AsStoreMut, instance: &Instance) -> i64 {
    let remaining_points = get_remaining_points(ctx, instance);
    match remaining_points {
        MeteringPoints::Remaining(x) => x as i64,
        MeteringPoints::Exhausted => 0,
    }
}

fn entity_from_function_name(fn_name: &str) -> Option<(DescribedEntityType, &str)> {
    for (prefix, ty) in [
        (DESCRIBE_TABLE_DUNDER, DescribedEntityType::Table),
        (DESCRIBE_REDUCER_DUNDER, DescribedEntityType::Reducer),
        (DESCRIBE_REPEATING_REDUCER_DUNDER, DescribedEntityType::RepeatingReducer),
    ] {
        if let Some(name) = fn_name.strip_prefix(prefix) {
            return Some((ty, name));
        }
    }
    None
}

fn log_traceback(func_type: &str, func: &str, e: &RuntimeError) {
    let frames = e.trace();
    let frames_len = frames.len();

    log::info!("{} \"{}\" runtime error: {}", func_type, func, e.message());
    for (i, frame) in frames.iter().enumerate().take(frames_len) {
        log::info!(
            "  Frame #{}: {:?}::{:?}",
            frames_len - i,
            frame.module_name(),
            frame.function_name().unwrap_or("<func>")
        );
    }
}

pub(crate) struct WasmModuleHostActor {
    worker_database_instance: WorkerDatabaseInstance,
    module_host: ModuleHost,
    _module_hash: Hash,
    module: Module,
    store: RefCell<Store>,
    instances: Vec<(u32, Instance, FunctionEnv<WasmInstanceEnv>)>,
    instance_tx_map: Arc<Mutex<HashMap<u32, WrapTxWrapper>>>,
    subscription: ModuleSubscription,

    // Holds the list of descriptions of each entity.
    // TODO(ryan): Long run let's replace or augment this with catalog table(s) that hold the
    // schema. Then standard table query tools could be run against it.
    description_cache: HashMap<String, EntityDef>,
    abi: abi::SpacetimeAbiVersion,
}

impl WasmModuleHostActor {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        module_hash: Hash,
        module: Module,
        store: Store,
        module_host: ModuleHost,
        abi: abi::SpacetimeAbiVersion,
    ) -> Result<Self, anyhow::Error> {
        let relational_db = worker_database_instance.relational_db.clone();
        let subscription = ModuleSubscription::spawn(relational_db);
        let mut host = Self {
            worker_database_instance,
            module_host,
            module,
            instance_tx_map: Arc::new(Mutex::new(HashMap::new())),
            _module_hash: module_hash,
            store: RefCell::new(store),
            instances: Vec::new(),
            subscription,
            description_cache: HashMap::new(),
            abi,
        };
        host.create_instance().unwrap();
        match host.populate_description_caches() {
            Ok(_) => Ok(host),
            Err(e) => Err(anyhow!(
                "Failed to populate schema description cache; garbage data in schema?: {}",
                e
            )),
        }
    }

    fn populate_description_caches(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module.exports().functions() {
            let (desc_entity_type, entity_name) = match entity_from_function_name(f.name()) {
                None => continue,
                Some(x) => x,
            };
            let description = self
                .call_describer(f.name(), desc_entity_type)?
                .ok_or_else(|| anyhow!("Bad describe function returned None; {}", f.name()))?;

            self.description_cache.insert(entity_name.into(), description);
        }
        Ok(())
    }

    fn create_instance(&mut self) -> Result<u32, anyhow::Error> {
        let store = self.store.get_mut();
        let instance_id = self.instances.len() as u32;
        let env = WasmInstanceEnv {
            instance_env: InstanceEnv {
                worker_database_instance: self.worker_database_instance.clone(),
                instance_id,
                instance_tx_map: self.instance_tx_map.clone(),
            },
            mem: None,
        };
        let env = FunctionEnv::new(store, env);
        let abi::SpacetimeAbiVersion::V0 = self.abi;
        let import_object = imports! {
            "spacetime_v0" => {
                "_delete_pk" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::delete_pk,
                ),
                "_delete_value" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::delete_value,
                ),
                "_delete_eq" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::delete_eq,
                ),
                "_delete_range" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::delete_range,
                ),
                "_insert" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::insert,
                ),
                "_create_table" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::create_table,
                ),
                "_get_table_id" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::get_table_id,
                ),
                "_iter" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::iter
                ),
                "_console_log" => Function::new_typed_with_env(
                    store,
                    &env,
                    WasmInstanceEnv::console_log
                ),
            }
        };

        let instance = Instance::new(store, &self.module, &import_object)?;

        let mem = Mem::extract(store, &instance.exports).context("couldn't access memory exports")?;
        env.as_mut(store).mem = Some(mem);

        // Note: this budget is just for INIT_PANIC_DUNDER.
        let points = DEFAULT_EXECUTION_BUDGET;
        set_remaining_points(store, &instance, points as u64);

        // Init panic if available
        let init_panic = instance.exports.get_typed_function::<(), ()>(store, INIT_PANIC_DUNDER);
        if let Ok(init_panic) = init_panic {
            match init_panic.call(store) {
                Ok(_) => {}
                Err(err) => {
                    log::warn!("Error initializing panic: {}", err);
                }
            }
        }

        self.instances.push((instance_id, instance, env));
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
        let mlog = self.worker_database_instance.message_log.clone();
        stdb.reset_hard(mlog)?;
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
        let (_tx, _consumed_energy, _remaining_energy, _repeat_duration) =
            self.execute_reducer(&create_table_symbol, None, [])?;
        Ok(())
    }

    fn call_migrate(&self, migrate_name: &str) -> Result<(), anyhow::Error> {
        let migrate_symbol = format!("{}{}", MIGRATE_DATABASE_DUNDER, migrate_name);
        let (_tx, _consumed_energy, _remaining_energy, _repeat_duration) =
            self.execute_reducer(&migrate_symbol, None, [])?;
        Ok(())
    }

    fn call_reducer(
        &self,
        caller_identity: Hash,
        reducer_name: &str,
        budget: ReducerBudget,
        arg_bytes: &[u8],
    ) -> Result<ReducerCallResult, anyhow::Error> {
        let start_instant = Instant::now();

        // TODO: validate arg_bytes
        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let arguments = ReducerArguments::new(
            spacetimedb_lib::Hash::from_arr(&caller_identity.data),
            timestamp,
            Vec::from(arg_bytes),
        );

        log::trace!("Calling reducer {} with a budget of {}", reducer_name, budget.0);

        // TODO: It's possible to push this down further into execute_reducer, and write directly
        // into the WASM memory, but ModuleEvent.function_call also wants a copy, so it doesn't
        // quite work.
        let mut new_arg_bytes = Vec::with_capacity(arguments.encoded_size());
        arguments.encode(&mut new_arg_bytes);

        let (tx, energy_quanta_used, energy_remaining, _repeat_duration) =
            self.execute_reducer(&reducer_symbol, Some(budget), new_arg_bytes)?;

        let (committed, status, budget_exceeded) = if let Some(tx) = tx {
            (true, EventStatus::Committed(tx.writes), false)
        } else if energy_remaining == 0 {
            log::error!("Ran out of energy while executing reducer {}", reducer_name);
            (false, EventStatus::OutOfEnergy, true)
        } else {
            (false, EventStatus::Failed, false)
        };

        let host_execution_duration = start_instant.elapsed();

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
            energy_quanta_used,
            host_execution_duration,
        };
        self.subscription.broadcast_event(event).unwrap();

        let result = ReducerCallResult {
            committed,
            budget_exceeded,
            energy_quanta_used,
            host_execution_duration,
        };
        Ok(result)
    }

    fn call_repeating_reducer(&self, reducer_name: &str, prev_call_time: u64) -> Result<(u64, u64), anyhow::Error> {
        let start_instant = Instant::now();

        let reducer_symbol = format!("{}{}", REPEATING_REDUCER_DUNDER, reducer_name);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let delta_time = timestamp - prev_call_time;
        let arguments = RepeatingReducerArguments::new(timestamp, delta_time);

        let mut arg_bytes = Vec::with_capacity(arguments.encoded_size());
        arguments.encode(&mut arg_bytes);

        // TODO(ryan): energy consumption from repeating reducers needs to be accounted for, for now
        // we run with default giant budget. The logistical problem here is that I'd rather not do
        // budget lookup inside the ModuleHostActor; it should rightfully be up in the HostController
        // like it is for regular reducers.
        // But the HostController is currently not involved at all in repeating reducer logic. They
        // are scheduled entirely within the ModuleHostActor.
        // Alternatively each module host actor could hold a copy of the budget, replicated all the
        // way down. But I don't know if I like that approach.
        // I think the right thing to do is refactor repeaters so that the scheduling is done up
        // in the host controller.
        let (tx, _energy_used, _remaining_energy, repeat_duration) =
            self.execute_reducer(&reducer_symbol, None, &arg_bytes)?;

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
            energy_quanta_used: 0, // TODO
            host_execution_duration: start_instant.elapsed(),
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok((repeat_duration.unwrap_or(delta_time), timestamp))
    }

    fn call_identity_connected_disconnected(&self, identity: &Hash, connected: bool) -> Result<(), anyhow::Error> {
        let start_instant = Instant::now();

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let arguments = ConnectDisconnectArguments::new(spacetimedb_lib::Hash::from_arr(&identity.data), timestamp);

        let mut new_arg_bytes = Vec::with_capacity(arguments.encoded_size());
        arguments.encode(&mut new_arg_bytes);

        let reducer_symbol = if connected {
            IDENTITY_CONNECTED_DUNDER
        } else {
            IDENTITY_DISCONNECTED_DUNDER
        };

        let result = self.execute_reducer(reducer_symbol, None, new_arg_bytes);
        let tx = match result {
            Ok((tx, _energy_consumed, _energy_remaining, _repeat_duration)) => tx,
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
            energy_quanta_used: 0,
            host_execution_duration: start_instant.elapsed(),
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }

    fn catalog(&self) -> Vec<(String, EntityDef)> {
        self.description_cache
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn describe(&self, entity_name: &str) -> Option<EntityDef> {
        self.description_cache.get(entity_name).cloned()
    }

    fn call_describer(
        &self,
        describer_func_name: &str,
        descr_type: DescribedEntityType,
    ) -> Result<Option<EntityDef>, anyhow::Error> {
        // TODO: choose one at random or whatever
        let (_instance_id, instance, env) = &self.instances[0];
        let describer = match instance.exports.get_function(describer_func_name) {
            Ok(describer) => describer,
            Err(_) => {
                // Making the bold assumption here that an error here means this entity doesn't exist.
                return Ok(None);
            }
        };

        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let mut store = &mut *self.store.borrow_mut();
        let result = describer.call(&mut store, &[]);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        match result {
            Err(err) => {
                log_traceback("describer", describer_func_name, &err);
                Err(anyhow!("Could not invoke describer function {}", describer_func_name))
            }
            Ok(ret) => {
                if ret.is_empty() {
                    return Err(anyhow!("Invalid return buffer arguments from {}", describer_func_name));
                }

                // The return value of the describer is a pointer to a vector.
                // The upper 32 bits of the 64-bit result is the offset into memory.
                // The lower 32 bits is its length
                let return_value = ret.first().unwrap().i64().unwrap() as u64;
                let offset = WasmPtr::new((return_value >> 32) as u32);
                let length = (return_value & 0xffffffff) as u32;

                // We have to copy all the memory out in order to use this.
                // This would be nice to avoid... and just somehow pass the memory contents directly
                // through to the TupleDef decode, but Wasmer's use of Cell prevents us from getting
                // a nice contiguous block of bytes?
                let mem = env.as_ref(store).mem().clone();
                let bytes = mem.read_output_bytes(store, offset, length).context("invalid ptr")?;
                mem.dealloc(store, offset, length).context("failed to dealloc")?;

                // Decode the memory as EntityDef.
                let result = match descr_type {
                    DescribedEntityType::Table => {
                        let table = TableDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Table(table)
                    }
                    DescribedEntityType::Reducer => {
                        let reducer = ReducerDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Reducer(reducer)
                    }
                    DescribedEntityType::RepeatingReducer => {
                        let repeater = RepeaterDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Repeater(repeater)
                    }
                };

                Ok(Some(result))
            }
        }
    }

    fn execute_reducer(
        &self,
        reducer_symbol: &str,
        budget: Option<ReducerBudget>,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<
        (
            Option<Transaction>,
            i64, /* energy used */
            i64, /* energy remaining */
            Option<u64>,
        ),
        anyhow::Error,
    > {
        let address = &self.worker_database_instance.address.to_abbreviated_hex();
        REDUCER_COUNT.with_label_values(&[address, reducer_symbol]).inc();

        let tx = self.worker_database_instance.relational_db.begin_tx();

        // TODO: choose one at random or whatever
        let (instance_id, instance, env) = &self.instances[0];
        self.instance_tx_map.lock().unwrap().insert(*instance_id, tx);

        let store = &mut *self.store.borrow_mut();

        let points = budget.unwrap_or(ReducerBudget(DEFAULT_EXECUTION_BUDGET));
        set_remaining_points(store, instance, max(points.0, 0) as u64);

        let mem = env.as_ref(store).mem().clone();

        let (ptr, len) = match mem.alloc_slice(store, arg_bytes.as_ref()) {
            Ok(ptr) => ptr,
            Err(e) => {
                if let Some(e) = e.downcast_ref() {
                    log_traceback("allocation", "alloc", e);
                }
                let remaining_points = get_remaining_points_value(store, instance);
                let used_points = points.0 - remaining_points;
                return Ok((None, used_points, remaining_points, None));
            }
        };

        let reduce = instance.exports.get_function(reducer_symbol)?;

        // let guard = pprof::ProfilerGuardBuilder::default().frequency(2500).build().unwrap();

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);
        // pass ownership of the `ptr` allocation into the reducer
        let result = reduce.call(store, &[ptr.into(), len.into()]);
        let duration = start.elapsed();
        let remaining_points = get_remaining_points_value(store, instance);
        log::trace!(
            "Reducer \"{}\" ran: {} us, {} eV",
            reducer_symbol,
            duration.as_micros(),
            points.0 - remaining_points
        );
        let used_energy = points.0 - remaining_points;

        REDUCER_COMPUTE_TIME
            .with_label_values(&[address, reducer_symbol])
            .observe(duration.as_secs_f64());

        // If you can afford to take 500 ms for a transaction
        // you can afford to generate a flamegraph. Fix your stuff.
        // if duration.as_millis() > 500 {
        //     if let Ok(report) = guard.report().build() {
        //         let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        //         let file = std::fs::File::create(format!("flamegraphs/flamegraph-{}.svg", now.as_millis())).unwrap();
        //         report.flamegraph(file).unwrap();
        //     };
        // }

        let result = match result {
            Err(err) => {
                let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
                let tx = instance_tx_map.remove(instance_id).unwrap();
                tx.rollback();

                log_traceback("reducer", reducer_symbol, &err);
                Ok((None, used_energy, remaining_points, None))
            }
            Ok(ret) => {
                let repeat_duration = if ret.len() == 1 {
                    ret.first().unwrap().i64().map(|i| i as u64)
                } else {
                    None
                };
                let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
                let tx = instance_tx_map.remove(instance_id).unwrap();
                if let Some(CommitResult { tx, commit_bytes }) = tx.commit().unwrap() {
                    if let Some(commit_bytes) = commit_bytes {
                        let mut mlog = self.worker_database_instance.message_log.lock().unwrap();
                        REDUCER_WRITE_SIZE
                            .with_label_values(&[address, reducer_symbol])
                            .observe(commit_bytes.len() as f64);
                        mlog.append(commit_bytes).unwrap();
                        mlog.sync_all().unwrap();
                    }
                    Ok((Some(tx), used_energy, remaining_points, repeat_duration))
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        };

        result
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
                budget,
                arg_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.call_reducer(caller_identity, &reducer_name, budget, &arg_bytes))
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
            ModuleHostCommand::Describe {
                entity_name,
                respond_to,
            } => {
                respond_to.send(self.describe(&entity_name)).unwrap();
                false
            }
        }
    }
}
