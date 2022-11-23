use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use parking_lot::Mutex;
use slab::Slab;
use spacetimedb_lib::args::{Arguments, ConnectDisconnectArguments, ReducerArguments, RepeatingReducerArguments};
use spacetimedb_lib::{EntityDef, TupleValue};

use crate::db::messages::transaction::Transaction;
use crate::db::transactional_db::CommitResult;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult};
use crate::host::instance_env::{InstanceEnv, TxSlot};
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHostActor, ModuleInfo};
use crate::host::tracelog::instance_trace::TraceLog;
use crate::module_subscription_actor::ModuleSubscription;
use crate::worker_database_instance::WorkerDatabaseInstance;
use crate::worker_metrics::{REDUCER_COMPUTE_TIME, REDUCER_COUNT, REDUCER_WRITE_SIZE};

use super::*;

pub trait WasmModule: Send + 'static {
    type Instance: WasmInstance;

    type ExternType: for<'a> PartialEq<FuncSig<'a>> + fmt::Debug;
    fn get_export(&self, s: &str) -> Option<Self::ExternType>;

    fn fill_general_funcnames(&self, func_names: &mut FuncNames) -> anyhow::Result<()>;

    fn create_instance(&mut self, env: InstanceEnv) -> anyhow::Result<Self::Instance>;
}

pub trait WasmInstance: Send + 'static {
    fn extract_descriptions(&mut self) -> anyhow::Result<HashMap<String, EntityDef>>;

    type Trap;

    fn call_migrate(
        &mut self,
        func_names: &FuncNames,
        id: usize,
        budget: ReducerBudget,
    ) -> (EnergyStats, Option<ExecuteResult<Self::Trap>>);

    fn call_reducer(
        &mut self,
        reducer_symbol: &str,
        budget: ReducerBudget,
        arg_bytes: &[u8],
    ) -> (EnergyStats, Option<ExecuteResult<Self::Trap>>);

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap);
}

pub struct EnergyStats {
    pub used: i64,
    pub remaining: i64,
}

pub struct ExecuteResult<E> {
    pub execution_time: Duration,
    pub call_result: Result<Option<u64>, E>,
}

pub struct ReducerResult {
    pub tx: Option<Transaction>,
    pub energy: EnergyStats,
    pub repeat_duration: Option<u64>,
}

pub(crate) struct WasmModuleHostActor<T: WasmModule> {
    module: T,
    worker_database_instance: WorkerDatabaseInstance,
    // store: Store,
    instances: Slab<(TxSlot, RefCell<T::Instance>)>,
    subscription: ModuleSubscription,
    #[allow(dead_code)] // Don't warn about 'trace_log' below when tracelogging feature isn't enabled.
    trace_log: Option<Arc<Mutex<TraceLog>>>,
    func_names: FuncNames,

    info: Arc<ModuleInfo>,
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        module_hash: Hash,
        mut module: T,
    ) -> anyhow::Result<Box<Self>> {
        let trace_log = if worker_database_instance.trace_log {
            Some(Arc::new(Mutex::new(TraceLog::new().unwrap())))
        } else {
            None
        };

        let relational_db = worker_database_instance.relational_db.clone();
        let subscription = ModuleSubscription::spawn(relational_db);

        let mut instances = Slab::new();
        let instance_slot = instances.vacant_entry();

        let instance_tx = TxSlot::default();
        let mut instance = module.create_instance(InstanceEnv::new(
            worker_database_instance.clone(),
            instance_tx.clone(),
            trace_log.clone(),
        ))?;

        let description_cache = instance.extract_descriptions()?;
        let mut func_names = FuncNames::default();
        description_cache
            .iter()
            .try_for_each(|(name, entity)| func_names.update_from_entity(|s| module.get_export(s), name, entity))?;
        module.fill_general_funcnames(&mut func_names)?;

        let info = Arc::new(ModuleInfo {
            identity: worker_database_instance.identity,
            module_hash,
            catalog: description_cache,
        });

        instance_slot.insert((instance_tx, RefCell::new(instance)));

        let mut host_actor = Box::new(Self {
            module,
            worker_database_instance,
            instances,
            subscription,
            trace_log,
            func_names,
            info,
        });
        if false {
            // silence warning
            // TODO: actually utilize this
            let _ = host_actor.create_instance();
        }
        Ok(host_actor)
    }

    fn create_instance(&mut self) -> anyhow::Result<usize> {
        let slot = self.instances.vacant_entry();
        let key = slot.key();
        let tx = TxSlot::default();
        let env = InstanceEnv::new(
            self.worker_database_instance.clone(),
            tx.clone(),
            self.trace_log.clone(),
        );
        slot.insert((tx, RefCell::new(self.module.create_instance(env)?)));
        Ok(key)
    }

    fn select_instance(&self) -> (&TxSlot, impl DerefMut<Target = T::Instance> + '_) {
        // TODO: choose one at random or whatever
        // These should be their own worker threads?
        let (tx_slot, instance) = &self.instances[0];
        (tx_slot, instance.borrow_mut())
    }
}

impl<T: WasmModule> ModuleHostActor for WasmModuleHostActor<T> {
    fn info(&self) -> Arc<ModuleInfo> {
        self.info.clone()
    }

    fn subscription(&self) -> &ModuleSubscription {
        &self.subscription
    }

    fn init_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();
        self.info
            .catalog
            .iter()
            .filter_map(|(name, entity)| match entity {
                EntityDef::Table(t) => Some((name, t)),
                _ => None,
            })
            .try_for_each(|(name, table)| {
                stdb.create_table(tx, name, table.tuple.clone())
                    .map(drop)
                    .with_context(|| format!("failed to create table {name}"))
            })?;
        tx_.commit()?.expect("TODO: retry?");

        // TODO: call __create_index__IndexName

        Ok(())
    }

    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mlog = self.worker_database_instance.message_log.clone();
        stdb.reset_hard(mlog)?;
        Ok(())
    }

    fn _migrate_database(&mut self) -> Result<(), anyhow::Error> {
        // TODO: figure out a better way to do this? all in one transaction? have to make sure not
        // to forget about EnergyStats if one returns an error
        for (i, name) in self.func_names.migrates.iter().enumerate() {
            self.with_tx(name, |inst| {
                inst.call_migrate(&self.func_names, i, DEFAULT_EXECUTION_BUDGET)
            })?;
        }

        // TODO: call __create_index__IndexName
        Ok(())
    }

    #[cfg(feature = "tracelogging")]
    fn get_trace(&self) -> Option<bytes::Bytes> {
        match &self.trace_log {
            None => None,
            Some(tl) => {
                let results = tl.lock().retrieve();
                match results {
                    Ok(tl) => Some(tl),
                    Err(e) => {
                        log::error!("Unable to retrieve trace log: {}", e);
                        None
                    }
                }
            }
        }
    }

    #[cfg(feature = "tracelogging")]
    fn stop_trace(&mut self) -> Result<(), anyhow::Error> {
        self.trace_log = None;
        Ok(())
    }

    fn call_reducer(
        &mut self,
        caller_identity: Hash,
        reducer_name: String,
        budget: ReducerBudget,
        args: TupleValue,
    ) -> Result<ReducerCallResult, anyhow::Error> {
        let start_instant = Instant::now();

        let EntityDef::Reducer(reducer_descr) = &self.info.catalog[&reducer_name] else {
            unreachable!() // ModuleHost::call_reducer should've already ensured this is ok
        };

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let arguments = ReducerArguments::new(spacetimedb_lib::Hash::from_arr(&caller_identity.data), timestamp, args);

        log::trace!("Calling reducer {} with a budget of {}", reducer_name, budget.0);

        // TODO: It's possible to push this down further into execute_reducer, and write directly
        // into the WASM memory, but ModuleEvent.function_call also wants a copy, so it doesn't
        // quite work.
        let arg_bytes = arguments.encode_to_vec();

        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);
        let ReducerResult {
            tx,
            energy,
            repeat_duration: _,
        } = self.with_tx(&reducer_symbol, |inst| {
            inst.call_reducer(&reducer_symbol, budget, &arg_bytes)
        })?;

        let (committed, status, budget_exceeded) = if let Some(tx) = tx {
            (true, EventStatus::Committed(tx.writes), false)
        } else if energy.remaining == 0 {
            log::error!("Ran out of energy while executing reducer {}", reducer_name);
            (false, EventStatus::OutOfEnergy, true)
        } else {
            (false, EventStatus::Failed, false)
        };

        let host_execution_duration = start_instant.elapsed();

        let arg_bytes = serde_json::to_vec(&arguments.arguments.serialize_args_with_schema(reducer_descr)).unwrap();
        let event = ModuleEvent {
            timestamp,
            caller_identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name,
                arg_bytes,
            },
            status,
            energy_quanta_used: energy.used,
            host_execution_duration,
        };
        self.subscription.broadcast_event(event).unwrap();

        let result = ReducerCallResult {
            committed,
            budget_exceeded,
            energy_quanta_used: energy.used,
            host_execution_duration,
        };
        Ok(result)
    }

    fn get_repeating_reducers(&self) -> Vec<String> {
        self.func_names.get_repeaters()
    }

    fn call_repeating_reducer(&mut self, reducer_id: usize, prev_call_time: u64) -> Result<(u64, u64), anyhow::Error> {
        let start_instant = Instant::now();

        let reducer_symbol = self
            .func_names
            .repeaters
            .get(reducer_id)
            .context("invalid repeater id")?;
        let reducer_name = &reducer_symbol[REPEATING_REDUCER_DUNDER.len()..];
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
        let budget = DEFAULT_EXECUTION_BUDGET;
        let ReducerResult {
            tx,
            energy,
            repeat_duration,
        } = self.with_tx(reducer_symbol, |inst| {
            inst.call_reducer(reducer_symbol, budget, &arg_bytes)
        })?;

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
            energy_quanta_used: energy.used,
            host_execution_duration: start_instant.elapsed(),
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok((repeat_duration.unwrap_or(delta_time), timestamp))
    }

    fn call_connect_disconnect(&mut self, identity: Hash, connected: bool) -> Result<(), anyhow::Error> {
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

        let budget = DEFAULT_EXECUTION_BUDGET;
        let result = self.with_tx(reducer_symbol, |inst| {
            inst.call_reducer(reducer_symbol, budget, &new_arg_bytes)
        });
        let tx = match result {
            Ok(res) => res.tx,
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
            caller_identity: identity,
            energy_quanta_used: 0,
            host_execution_duration: start_instant.elapsed(),
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    fn with_tx(
        &self,
        func_ident: &str,
        f: impl FnOnce(&mut T::Instance) -> (EnergyStats, Option<ExecuteResult<<T::Instance as WasmInstance>::Trap>>),
    ) -> Result<ReducerResult, anyhow::Error> {
        let address = &self.worker_database_instance.address.to_abbreviated_hex();
        REDUCER_COUNT.with_label_values(&[address, func_ident]).inc();

        let tx = self.worker_database_instance.relational_db.begin_tx();

        let (tx_slot, mut instance) = self.select_instance();

        let (tx, (energy, result)) = tx_slot.set(tx, || f(&mut instance));

        drop(instance);

        let ExecuteResult {
            execution_time,
            call_result,
        } = match result {
            Some(x) => x,
            None => {
                return Ok(ReducerResult {
                    tx: None,
                    energy,
                    repeat_duration: None,
                })
            }
        };

        log::trace!(
            "Reducer \"{}\" ran: {} us, {} eV",
            func_ident,
            execution_time.as_micros(),
            energy.used
        );

        REDUCER_COMPUTE_TIME
            .with_label_values(&[address, func_ident])
            .observe(execution_time.as_secs_f64());

        // If you can afford to take 500 ms for a transaction
        // you can afford to generate a flamegraph. Fix your stuff.
        // if duration.as_millis() > 500 {
        //     if let Ok(report) = guard.report().build() {
        //         let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        //         let file = std::fs::File::create(format!("flamegraphs/flamegraph-{}.svg", now.as_millis())).unwrap();
        //         report.flamegraph(file).unwrap();
        //     };
        // }

        match call_result {
            Err(err) => {
                tx.rollback();

                T::Instance::log_traceback("reducer", func_ident, &err);
                // TODO: discard instance on trap? there are likely memory leaks
                Ok(ReducerResult {
                    tx: None,
                    energy,
                    repeat_duration: None,
                })
            }
            Ok(repeat_duration) => {
                if let Some(CommitResult { tx, commit_bytes }) = tx.commit().unwrap() {
                    if let Some(commit_bytes) = commit_bytes {
                        let mut mlog = self.worker_database_instance.message_log.lock().unwrap();
                        REDUCER_WRITE_SIZE
                            .with_label_values(&[address, func_ident])
                            .observe(commit_bytes.len() as f64);
                        mlog.append(commit_bytes).unwrap();
                        mlog.sync_all().unwrap();
                    }
                    Ok(ReducerResult {
                        tx: Some(tx),
                        energy,
                        repeat_duration,
                    })
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        }
    }
}
