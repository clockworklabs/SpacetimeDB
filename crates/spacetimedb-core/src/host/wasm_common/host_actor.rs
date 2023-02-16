use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use parking_lot::Mutex;
use spacetimedb_lib::ser::serde::SerializeWrapper as SerdeWrapper;
use spacetimedb_lib::{EntityDef, ReducerDef, TupleValue};
use spacetimedb_sats::Typespace;
use tokio::sync::oneshot;

use crate::db::messages::transaction::Transaction;
use crate::db::transactional_db::CommitResult;
use crate::hash::Hash;
use crate::host::host_controller::{ReducerBudget, ReducerCallResult, Scheduler};
use crate::host::instance_env::InstanceEnv;
use crate::host::module_host::{EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHostActor, ModuleInfo};
use crate::host::timestamp::Timestamp;
use crate::host::tracelog::instance_trace::TraceLog;
use crate::module_subscription_actor::ModuleSubscription;
use crate::worker_database_instance::WorkerDatabaseInstance;
use crate::worker_metrics::{REDUCER_COMPUTE_TIME, REDUCER_COUNT, REDUCER_WRITE_SIZE};

use super::*;

const MSG_CHANNEL_CAP: usize = 8;
const MSG_CHANNEL_TIMEOUT: Duration = Duration::from_millis(500);

pub trait WasmModule: Send + 'static {
    type Instance: WasmInstance;
    type UninitInstance: UninitWasmInstance<Instance = Self::Instance>;

    type ExternType: for<'a> PartialEq<FuncSig<'a>> + fmt::Debug;
    fn get_export(&self, s: &str) -> Option<Self::ExternType>;

    fn fill_general_funcnames(&self, func_names: &mut FuncNames) -> anyhow::Result<()>;

    fn create_instance(&mut self, env: InstanceEnv) -> Self::UninitInstance;
}

pub trait UninitWasmInstance: Send + 'static {
    type Instance: WasmInstance;
    fn initialize(self, func_names: &FuncNames) -> anyhow::Result<Self::Instance>;
}

pub trait WasmInstance: Send + 'static {
    fn extract_descriptions(&mut self) -> anyhow::Result<(Typespace, HashMap<String, EntityDef>)>;

    fn instance_env(&self) -> &InstanceEnv;

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
    subscription: ModuleSubscription,
    #[allow(dead_code)]
    // Don't warn about 'trace_log' below when tracelogging feature isn't enabled.
    trace_log: Option<Arc<Mutex<TraceLog>>>,
    scheduler: Scheduler,
    func_names: Arc<FuncNames>,

    msg_tx: crossbeam_channel::Sender<InstanceMessage>,
    msg_rx: crossbeam_channel::Receiver<InstanceMessage>,

    info: Arc<ModuleInfo>,

    instance_count: Arc<()>,
}

enum InitOrUninit<T: UninitWasmInstance> {
    Init(T::Instance),
    Uninit(T),
}
impl<T: UninitWasmInstance> InitOrUninit<T> {
    fn initialize(self, func_names: &FuncNames) -> anyhow::Result<T::Instance> {
        match self {
            InitOrUninit::Init(inst) => Ok(inst),
            InitOrUninit::Uninit(uninit) => uninit.initialize(func_names),
        }
    }
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

        let mut instance = module
            .create_instance(InstanceEnv::new(
                worker_database_instance.clone(),
                scheduler.clone(),
                trace_log.clone(),
            ))
            .initialize(&func_names)?;

        let (typespace, catalog) = instance.extract_descriptions()?;
        catalog
            .iter()
            .try_for_each(|(name, entity)| func_names.update_from_entity(|s| module.get_export(s), name, entity))?;

        let info = Arc::new(ModuleInfo {
            identity: worker_database_instance.identity,
            module_hash,
            typespace,
            catalog,
        });

        let func_names = Arc::new(func_names);
        let (msg_tx, msg_rx) = crossbeam_channel::bounded(MSG_CHANNEL_CAP);

        let this = Box::new(Self {
            module,
            worker_database_instance,
            msg_tx,
            msg_rx,
            subscription,
            trace_log,
            scheduler,
            func_names,
            info,
            instance_count: Arc::new(()),
        });

        this._spawn_instance(InitOrUninit::Init(instance));

        Ok(this)
    }

    fn spawn_instance(&mut self) {
        let env = InstanceEnv::new(
            self.worker_database_instance.clone(),
            self.scheduler.clone(),
            self.trace_log.clone(),
        );
        let instance = self.module.create_instance(env);
        self._spawn_instance(InitOrUninit::Uninit(instance))
    }
    fn _spawn_instance(&self, instance: InitOrUninit<T::UninitInstance>) {
        let instance_count = self.instance_count.clone();
        let (func_names, info) = (self.func_names.clone(), self.info.clone());
        let (msg_rx, subscription) = (self.msg_rx.clone(), self.subscription.clone());
        tokio::task::spawn_blocking(|| {
            // this shouldn't fail, since we already called module.create_instance()
            // before and it didn't error, and ideally they should be deterministic
            let instance = instance.initialize(&func_names).expect("failed to initialize instance");
            WasmInstanceActor {
                instance,
                func_names,
                info,
                msg_rx,
                subscription,
            }
            .run();
            drop(instance_count);
        });
    }

    fn send(&mut self, mut msg: InstanceMessage) {
        if true {
            // this can never actually be a SendError, since we're holding
            // onto a msg_rx ourselves, so the channel won't close
            let _ = self.msg_tx.send(msg);
        } else {
            // TODO: implement reducer retries and have multiple instance threads
            loop {
                match self.msg_tx.send_timeout(msg, MSG_CHANNEL_TIMEOUT) {
                    Ok(()) => break,
                    // this can never actually be a SendTimeoutError::Disconnected, since we're holding
                    // onto a msg_rx ourselves, so the channel won't close
                    Err(err) => {
                        msg = err.into_inner();
                        let instance_count = Arc::strong_count(&self.instance_count) - 1;
                        // TODO: better heuristics
                        if instance_count < 8 {
                            self.spawn_instance()
                        }
                    }
                }
            }
        }
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
        respond_to: oneshot::Sender<Result<Option<ReducerCallResult>, anyhow::Error>>,
    ) {
        self.send(InstanceMessage::InitDatabase {
            budget,
            args,
            respond_to,
        })
    }

    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let mlog = self.worker_database_instance.message_log.clone();
        stdb.reset_hard(mlog)?;
        Ok(())
    }

    fn _migrate_database(&mut self, respond_to: oneshot::Sender<Result<(), anyhow::Error>>) {
        self.send(InstanceMessage::MigrateDatabase { respond_to })
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
        // TODO: figure out if we need to communicate this to all of our instances too
        self.trace_log = None;
        Ok(())
    }

    fn call_connect_disconnect(&mut self, caller_identity: Hash, connected: bool, respond_to: oneshot::Sender<()>) {
        self.send(InstanceMessage::CallConnectDisconnect {
            caller_identity,
            connected,
            respond_to,
        });
    }

    fn call_reducer(
        &mut self,
        caller_identity: Hash,
        reducer_name: String,
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<ReducerCallResult>,
    ) {
        self.send(InstanceMessage::CallReducer {
            caller_identity,
            reducer_name,
            budget,
            args,
            respond_to,
        })
    }
}

struct WasmInstanceActor<T: WasmInstance> {
    instance: T,
    func_names: Arc<FuncNames>,
    info: Arc<ModuleInfo>,
    msg_rx: crossbeam_channel::Receiver<InstanceMessage>,
    subscription: ModuleSubscription,
}

impl<T: WasmInstance> WasmInstanceActor<T> {
    fn worker_database_instance(&self) -> &WorkerDatabaseInstance {
        &self.instance.instance_env().worker_database_instance
    }

    fn run(mut self) {
        while let Ok(msg) = self.msg_rx.recv() {
            match msg {
                InstanceMessage::InitDatabase {
                    budget,
                    args,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.init_database(budget, args));
                }
                InstanceMessage::CallConnectDisconnect {
                    caller_identity,
                    connected,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.call_connect_disconnect(caller_identity, connected));
                }
                InstanceMessage::CallReducer {
                    caller_identity,
                    reducer_name,
                    budget,
                    args,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.call_reducer(caller_identity, reducer_name, budget, args));
                }
                InstanceMessage::MigrateDatabase { respond_to } => {
                    let _ = respond_to.send(self.migrate_database());
                }
                InstanceMessage::Exit => break,
            }
        }
    }

    fn init_database(&mut self, budget: ReducerBudget, args: TupleValue) -> anyhow::Result<Option<ReducerCallResult>> {
        let mut stdb_ = self.worker_database_instance().relational_db.lock().unwrap();
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
                let schema = self
                    .info
                    .typespace
                    .with_type(&table.data)
                    .resolve_refs()
                    .context("recursive types not yet supported")?;
                let schema = schema.into_product().ok().context("table not a product type?")?;
                stdb.create_table(tx, name, schema)
                    .map(drop)
                    .with_context(|| format!("failed to create table {name}"))
            })?;
        tx_.commit()?.expect("TODO: retry?");
        drop(stdb_);

        let rcr = self.info.catalog.contains_key(INIT_DUNDER).then(|| {
            self.call_reducer(
                self.worker_database_instance().identity,
                INIT_DUNDER.to_owned(),
                budget,
                args,
            )
        });

        // TODO: call __create_index__IndexName

        Ok(rcr)
    }

    fn migrate_database(&mut self) -> Result<(), anyhow::Error> {
        // TODO: figure out a better way to do this? all in one transaction? have to make sure not
        // to forget about EnergyStats if one returns an error
        for idx in 0..self.func_names.migrates.len() {
            self.execute(ReducerBudget(DEFAULT_EXECUTION_BUDGET), InstanceOp::Migrate { idx });
        }

        // TODO: call __create_index__IndexName
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

        let timestamp = Timestamp::now();

        log::trace!("Calling reducer {} with a budget of {}", reducer_name, budget.0);

        let mut arg_bytes = Vec::new();
        args.encode(&mut arg_bytes);

        let reducer_symbol = [REDUCE_DUNDER, &reducer_name].concat();
        let ReducerResult { tx, energy } = self.execute(
            budget,
            InstanceOp::Reducer {
                sym: &reducer_symbol,
                sender: &caller_identity,
                timestamp,
                arg_bytes,
            },
        );

        let (committed, status, budget_exceeded) = if let Some(tx) = tx {
            (true, EventStatus::Committed(tx.writes), false)
        } else if energy.remaining == 0 {
            log::error!("Ran out of energy while executing reducer {}", reducer_name);
            (false, EventStatus::OutOfEnergy, true)
        } else {
            (false, EventStatus::Failed, false)
        };

        let host_execution_duration = start_instant.elapsed();

        let EntityDef::Reducer(reducer_descr) = &self.info.catalog[&reducer_name] else {
            unreachable!() // ModuleHost::call_reducer should've already ensured this is ok
        };
        let reducer_descr = self.info.typespace.with_type(reducer_descr);
        let arg_bytes =
            serde_json::to_vec(&SerdeWrapper::new(ReducerDef::serialize_args(reducer_descr, &args))).unwrap();
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

        let result = self.execute(
            ReducerBudget(DEFAULT_EXECUTION_BUDGET),
            InstanceOp::ConnDisconn {
                conn: connected,
                sender: &identity,
                timestamp,
            },
        );

        let status = if let Some(tx) = result.tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        let reducer_symbol = if connected {
            IDENTITY_CONNECTED_DUNDER
        } else {
            IDENTITY_DISCONNECTED_DUNDER
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

    fn execute(&mut self, budget: ReducerBudget, op: InstanceOp<'_>) -> ReducerResult {
        let address = &self.worker_database_instance().address.to_abbreviated_hex();
        let func_ident = match op {
            InstanceOp::Reducer { sym, .. } => sym,
            InstanceOp::Migrate { idx, .. } => &self.func_names.migrates[idx],
            InstanceOp::ConnDisconn { conn, .. } => {
                if conn {
                    IDENTITY_CONNECTED_DUNDER
                } else {
                    IDENTITY_DISCONNECTED_DUNDER
                }
            }
        };
        REDUCER_COUNT.with_label_values(&[address, func_ident]).inc();

        let tx = self.worker_database_instance().relational_db.begin_tx();

        let tx_slot = self.instance.instance_env().tx.clone();
        let (tx, (energy, result)) = tx_slot.set(tx, || match op {
            InstanceOp::Reducer {
                sym,
                sender,
                timestamp,
                arg_bytes,
            } => self
                .instance
                .call_reducer(sym, budget, &sender.data, timestamp, arg_bytes),
            InstanceOp::Migrate { idx } => self.instance.call_migrate(&self.func_names, idx, budget),
            InstanceOp::ConnDisconn {
                conn,
                sender,
                timestamp,
            } => self
                .instance
                .call_connect_disconnect(conn, budget, &sender.data, timestamp),
        });

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

                T::log_traceback("reducer", func_ident, &err);
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
                        let mut mlog = self.worker_database_instance().message_log.lock().unwrap();
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

enum InstanceOp<'a> {
    Reducer {
        sym: &'a str,
        sender: &'a Hash,
        timestamp: Timestamp,
        arg_bytes: Vec<u8>,
    },
    Migrate {
        idx: usize,
    },
    ConnDisconn {
        conn: bool,
        sender: &'a Hash,
        timestamp: Timestamp,
    },
}

enum InstanceMessage {
    InitDatabase {
        budget: ReducerBudget,
        args: TupleValue,
        respond_to: oneshot::Sender<Result<Option<ReducerCallResult>, anyhow::Error>>,
    },
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
    MigrateDatabase {
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
    // TODO: some heuristic to figure out when we should cull instances due to lack of use
    #[allow(unused)]
    Exit,
}
