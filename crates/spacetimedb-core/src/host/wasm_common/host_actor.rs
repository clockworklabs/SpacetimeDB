use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use parking_lot::Mutex;
use slab::Slab;
use spacetimedb_lib::{EntityDef, TupleValue};

use crate::db::messages::transaction::Transaction;
use crate::db::transactional_db::CommitResult;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult, Scheduler};
use crate::host::instance_env::{InstanceEnv, TxSlot};
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHostActor, ModuleInfo};
use crate::host::timestamp::Timestamp;
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

    fn create_instance(&mut self, func_names: &FuncNames, env: InstanceEnv) -> anyhow::Result<Self::Instance>;
}

pub trait WasmInstance: Send + 'static {
    fn extract_descriptions(&mut self) -> anyhow::Result<HashMap<String, EntityDef>>;

    type Trap;

    fn call_migrate(
        &mut self,
        func_names: &FuncNames,
        id: usize,
        budget: ReducerBudget,
    ) -> (EnergyStats, ExecuteResult<Self::Trap>);

    fn call_reducer(
        &mut self,
        reducer_symbol: &str,
        budget: ReducerBudget,
        sender: &[u8; 32],
        timestamp: Timestamp,
        arg_bytes: Vec<u8>,
    ) -> (EnergyStats, ExecuteResult<Self::Trap>);

    fn call_connect_disconnect(
        &mut self,
        connect: bool,
        budget: ReducerBudget,
        sender: &[u8; 32],
        timestamp: Timestamp,
    ) -> (EnergyStats, ExecuteResult<Self::Trap>);

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap);
}

pub struct EnergyStats {
    pub used: i64,
    pub remaining: i64,
}

pub struct ExecuteResult<E> {
    pub execution_time: Duration,
    pub call_result: Result<Result<(), Box<str>>, E>,
}

pub struct ReducerResult {
    pub tx: Option<Transaction>,
    pub energy: EnergyStats,
}

pub(crate) struct WasmModuleHostActor<T: WasmModule> {
    module: T,
    worker_database_instance: WorkerDatabaseInstance,
    // store: Store,
    instances: Slab<(TxSlot, RefCell<T::Instance>)>,
    subscription: ModuleSubscription,
    #[allow(dead_code)]
    // Don't warn about 'trace_log' below when tracelogging feature isn't enabled.
    trace_log: Option<Arc<Mutex<TraceLog>>>,
    scheduler: Scheduler,
    func_names: FuncNames,

    info: Arc<ModuleInfo>,
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        module_hash: Hash,
        mut module: T,
        scheduler: Scheduler,
    ) -> anyhow::Result<Box<Self>> {
        let trace_log = if worker_database_instance.trace_log {
            Some(Arc::new(Mutex::new(TraceLog::new().unwrap())))
        } else {
            None
        };

        let mut func_names = FuncNames::default();
        module.fill_general_funcnames(&mut func_names)?;
        func_names.preinits.sort_unstable();

        let relational_db = worker_database_instance.relational_db.clone();
        let subscription = ModuleSubscription::spawn(relational_db);

        let mut instances = Slab::new();
        let instance_slot = instances.vacant_entry();

        let instance_tx = TxSlot::default();
        let mut instance = module.create_instance(
            &func_names,
            InstanceEnv::new(
                worker_database_instance.clone(),
                scheduler.clone(),
                instance_tx.clone(),
                trace_log.clone(),
            ),
        )?;

        let description_cache = instance.extract_descriptions()?;
        description_cache
            .iter()
            .try_for_each(|(name, entity)| func_names.update_from_entity(|s| module.get_export(s), name, entity))?;

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
            scheduler,
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
            self.scheduler.clone(),
            tx.clone(),
            self.trace_log.clone(),
        );
        slot.insert((tx, RefCell::new(self.module.create_instance(&self.func_names, env)?)));
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

    fn init_database(
        &mut self,
        budget: ReducerBudget,
        args: TupleValue,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let mut stdb_ = self.worker_database_instance.relational_db.lock().unwrap();
        let mut tx_ = stdb_.begin_tx();
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
        drop(stdb_);

        let rcr = self.info.catalog.contains_key(INIT_DUNDER).then(|| {
            self.call_reducer(
                self.worker_database_instance.identity,
                INIT_DUNDER.to_owned(),
                budget,
                args,
            )
        });

        // TODO: call __create_index__IndexName

        Ok(rcr)
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
            });
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
    ) -> ReducerCallResult {
        let start_instant = Instant::now();

        let EntityDef::Reducer(reducer_descr) = &self.info.catalog[&reducer_name] else {
            unreachable!() // ModuleHost::call_reducer should've already ensured this is ok
        };

        let timestamp = Timestamp::now();

        log::trace!("Calling reducer {} with a budget of {}", reducer_name, budget.0);

        let mut arg_bytes = Vec::new();
        args.encode(&mut arg_bytes);

        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);
        let ReducerResult { tx, energy } = self.with_tx(&reducer_symbol, |inst| {
            inst.call_reducer(&reducer_symbol, budget, &caller_identity.data, timestamp, arg_bytes)
        });

        let (committed, status, budget_exceeded) = if let Some(tx) = tx {
            (true, EventStatus::Committed(tx.writes), false)
        } else if energy.remaining == 0 {
            log::error!("Ran out of energy while executing reducer {}", reducer_name);
            (false, EventStatus::OutOfEnergy, true)
        } else {
            (false, EventStatus::Failed, false)
        };

        let host_execution_duration = start_instant.elapsed();

        let arg_bytes = serde_json::to_vec(&args.serialize_args_with_schema(reducer_descr)).unwrap();
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

        ReducerCallResult {
            committed,
            budget_exceeded,
            energy_quanta_used: energy.used,
            host_execution_duration,
        }
    }

    fn call_connect_disconnect(&mut self, identity: Hash, connected: bool) {
        let has_function = if connected {
            self.func_names.conn
        } else {
            self.func_names.disconn
        };
        if !has_function {
            return;
        }

        let start_instant = Instant::now();

        let timestamp = Timestamp::now();

        let reducer_symbol = if connected {
            IDENTITY_CONNECTED_DUNDER
        } else {
            IDENTITY_DISCONNECTED_DUNDER
        };

        let budget = DEFAULT_EXECUTION_BUDGET;
        let result = self.with_tx(reducer_symbol, |inst| {
            inst.call_connect_disconnect(connected, budget, &identity.data, timestamp)
        });
        let tx = result.tx;

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
    }
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    fn with_tx(
        &self,
        func_ident: &str,
        f: impl FnOnce(&mut T::Instance) -> (EnergyStats, ExecuteResult<<T::Instance as WasmInstance>::Trap>),
    ) -> ReducerResult {
        let address = &self.worker_database_instance.address.to_abbreviated_hex();
        REDUCER_COUNT.with_label_values(&[address, func_ident]).inc();

        let tx = self.worker_database_instance.relational_db.begin_tx();

        let (tx_slot, mut instance) = self.select_instance();

        let (tx, (energy, result)) = tx_slot.set(tx, || f(&mut instance));

        drop(instance);

        let ExecuteResult {
            execution_time,
            call_result,
        } = result;

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
                ReducerResult { tx: None, energy }
            }
            Ok(Err(errmsg)) => {
                tx.rollback();

                log::info!("reducer returned error: {errmsg}");

                ReducerResult { tx: None, energy }
            }
            Ok(Ok(())) => {
                if let Some(CommitResult { tx, commit_bytes }) = tx.commit().unwrap() {
                    if let Some(commit_bytes) = commit_bytes {
                        let mut mlog = self.worker_database_instance.message_log.lock().unwrap();
                        REDUCER_WRITE_SIZE
                            .with_label_values(&[address, func_ident])
                            .observe(commit_bytes.len() as f64);
                        mlog.append(commit_bytes).unwrap();
                        mlog.sync_all().unwrap();
                    }
                    ReducerResult { tx: Some(tx), energy }
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        }
    }
}
