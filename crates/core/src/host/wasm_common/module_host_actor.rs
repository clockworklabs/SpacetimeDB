use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::datastore::traits::{ColumnDef, IndexDef, TableDef, TableSchema};
use crate::host::scheduler::Scheduler;
use anyhow::Context;
use bytes::Bytes;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::{bsatn, IndexType, ModuleDef};

use crate::client::ClientConnectionSender;
use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::{DatabaseLogger, LogLevel, Record};
use crate::hash::Hash;
use crate::host::instance_env::InstanceEnv;
use crate::host::module_host::{
    DatabaseUpdate, EventStatus, Module, ModuleEvent, ModuleFunctionCall, ModuleInfo, ModuleInstance,
    UpdateDatabaseError, UpdateDatabaseResult, UpdateDatabaseSuccess,
};
use crate::host::{
    ArgsTuple, EnergyDiff, EnergyMonitor, EnergyMonitorFingerprint, EnergyQuanta, EntityDef, ReducerCallResult,
    ReducerOutcome, Timestamp,
};
use crate::identity::Identity;
use crate::subscription::module_subscription_actor::{ModuleSubscriptionManager, SubscriptionEventSender};
use crate::worker_metrics::{REDUCER_COMPUTE_TIME, REDUCER_COUNT, REDUCER_WRITE_SIZE};

use super::*;

pub trait WasmModule: Send + 'static {
    type Instance: WasmInstance;
    type InstancePre: WasmInstancePre<Instance = Self::Instance>;

    type ExternType: FuncSigLike;
    fn get_export(&self, s: &str) -> Option<Self::ExternType>;
    fn for_each_export<E>(&self, f: impl FnMut(&str, &Self::ExternType) -> Result<(), E>) -> Result<(), E>;

    fn instantiate_pre(&self) -> Result<Self::InstancePre, InitializationError>;
}

pub trait WasmInstancePre: Send + Sync + 'static {
    type Instance: WasmInstance;
    fn instantiate(&self, env: InstanceEnv, func_name: &FuncNames) -> Result<Self::Instance, InitializationError>;
}

pub trait WasmInstance: Send + Sync + 'static {
    fn extract_descriptions(&mut self) -> Result<Bytes, DescribeError>;

    fn instance_env(&self) -> &InstanceEnv;

    type Trap;

    fn call_reducer(
        &mut self,
        reducer_id: usize,
        budget: EnergyQuanta,
        sender: &[u8; 32],
        timestamp: Timestamp,
        arg_bytes: Bytes,
    ) -> ExecuteResult<Self::Trap>;

    fn call_connect_disconnect(
        &mut self,
        connect: bool,
        budget: EnergyQuanta,
        sender: &[u8; 32],
        timestamp: Timestamp,
    ) -> ExecuteResult<Self::Trap>;

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap);
}

pub struct EnergyStats {
    pub used: EnergyDiff,
    pub remaining: EnergyQuanta,
}

pub struct ExecuteResult<E> {
    pub energy: EnergyStats,
    pub execution_duration: Duration,
    pub call_result: Result<Result<(), Box<str>>, E>,
}

pub(crate) struct WasmModuleHostActor<T: WasmModule> {
    module: T::InstancePre,
    initial_instance: Option<Box<WasmModuleInstance<T::Instance>>>,
    worker_database_instance: Arc<DatabaseInstanceContext>,
    event_tx: SubscriptionEventSender,
    scheduler: Scheduler,
    func_names: Arc<FuncNames>,
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
}

#[derive(thiserror::Error, Debug)]
pub enum InitializationError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("setup function returned an error: {0}")]
    Setup(Box<str>),
    #[error("wasm trap while calling {func:?}")]
    RuntimeError {
        #[source]
        err: anyhow::Error,
        func: String,
    },
    #[error(transparent)]
    Instantiation(anyhow::Error),
    #[error("error getting module description: {0}")]
    Describe(#[from] DescribeError),
}

#[derive(thiserror::Error, Debug)]
pub enum DescribeError {
    #[error("bad signature for descriptor function")]
    Signature,
    #[error("error decoding module description: {0}")]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    RuntimeError(anyhow::Error),
    #[error("invalid buffer")]
    BadBuffer,
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn new(
        database_instance_context: Arc<DatabaseInstanceContext>,
        module_hash: Hash,
        module: T,
        scheduler: Scheduler,
        energy_monitor: Arc<dyn EnergyMonitor>,
    ) -> Result<Self, InitializationError> {
        log::trace!(
            "Making new module host actor for database {}",
            database_instance_context.address
        );
        let log_tx = database_instance_context.logger.lock().unwrap().tx.clone();

        FuncNames::check_required(|name| module.get_export(name))?;
        let mut func_names = FuncNames::default();
        module.for_each_export(|sym, ty| func_names.update_from_general(sym, ty))?;
        func_names.preinits.sort_unstable();

        let owner_identity = database_instance_context.identity;
        let relational_db = database_instance_context.relational_db.clone();
        let (subscription, event_tx) = ModuleSubscriptionManager::spawn(relational_db, owner_identity);

        let uninit_instance = module.instantiate_pre()?;
        let mut instance = uninit_instance.instantiate(
            InstanceEnv::new(database_instance_context.clone(), scheduler.clone()),
            &func_names,
        )?;

        let desc = instance.extract_descriptions()?;
        let desc = bsatn::from_slice(&desc).map_err(DescribeError::Decode)?;
        let ModuleDef {
            typespace,
            tables,
            reducers,
            misc_exports: _,
        } = desc;
        let catalog = itertools::chain(
            tables.into_iter().map(|x| (x.name.clone(), EntityDef::Table(x))),
            reducers.iter().map(|x| (x.name.clone(), EntityDef::Reducer(x.clone()))),
        )
        .collect();
        let reducers = reducers.into_iter().map(|x| (x.name.clone(), x)).collect();

        let info = Arc::new(ModuleInfo {
            identity: database_instance_context.identity,
            module_hash,
            typespace,
            reducers,
            catalog,
            log_tx,
            subscription,
        });

        let func_names = Arc::new(func_names);
        let mut module = WasmModuleHostActor {
            module: uninit_instance,
            initial_instance: None,
            func_names,
            info,
            event_tx,
            worker_database_instance: database_instance_context,
            scheduler,
            energy_monitor,
        };
        module.initial_instance = Some(Box::new(module.make_from_instance(instance)));

        Ok(module)
    }
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    fn make_from_instance(&self, instance: T::Instance) -> WasmModuleInstance<T::Instance> {
        WasmModuleInstance {
            instance,
            func_names: self.func_names.clone(),
            info: self.info.clone(),
            event_tx: self.event_tx.clone(),
            energy_monitor: self.energy_monitor.clone(),
            trapped: false,
        }
    }
}

impl<T: WasmModule> Module for WasmModuleHostActor<T> {
    type Instance = WasmModuleInstance<T::Instance>;

    type InitialInstances<'a> = Option<Self::Instance>;

    fn initial_instances(&mut self) -> Self::InitialInstances<'_> {
        self.initial_instance.take().map(|x| *x)
    }

    fn info(&self) -> Arc<ModuleInfo> {
        self.info.clone()
    }

    fn create_instance(&self) -> Self::Instance {
        let env = InstanceEnv::new(self.worker_database_instance.clone(), self.scheduler.clone());
        // this shouldn't fail, since we already called module.create_instance()
        // before and it didn't error, and ideally they should be deterministic
        let mut instance = self
            .module
            .instantiate(env, &self.func_names)
            .expect("failed to initialize instance");
        let _ = instance.extract_descriptions();
        self.make_from_instance(instance)
    }

    fn inject_logs(&self, log_level: LogLevel, message: &str) {
        self.worker_database_instance.logger.lock().unwrap().write(
            log_level,
            &Record {
                target: None,
                filename: Some("external"),
                line_number: None,
                message,
            },
            &(),
        )
    }

    fn close(self) {
        self.scheduler.close()
    }
}

/// Somewhat ad-hoc wrapper around [`DatabaseLogger`] which allows to inject
/// "system messages" into the user-retrievable database / module log
struct SystemLogger<'a> {
    inner: std::sync::MutexGuard<'a, DatabaseLogger>,
}

impl SystemLogger<'_> {
    fn warn(&mut self, msg: &str) {
        self.inner
            .write(crate::database_logger::LogLevel::Warn, &Self::record(msg), &())
    }

    fn error(&mut self, msg: &str) {
        self.inner
            .write(crate::database_logger::LogLevel::Error, &Self::record(msg), &())
    }

    fn record(message: &str) -> Record {
        Record {
            target: None,
            filename: Some("spacetimedb"),
            line_number: None,
            message,
        }
    }
}

pub struct WasmModuleInstance<T: WasmInstance> {
    instance: T,
    func_names: Arc<FuncNames>,
    info: Arc<ModuleInfo>,
    event_tx: SubscriptionEventSender,
    energy_monitor: Arc<dyn EnergyMonitor>,
    trapped: bool,
}

impl<T: WasmInstance> std::fmt::Debug for WasmModuleInstance<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmInstanceActor")
            .field("trapped", &self.trapped)
            .finish()
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
    fn database_instance_context(&self) -> &DatabaseInstanceContext {
        &self.instance.instance_env().dbic
    }
}

impl<T: WasmInstance> ModuleInstance for WasmModuleInstance<T> {
    fn trapped(&self) -> bool {
        self.trapped
    }

    #[tracing::instrument(skip(args))]
    fn init_database(&mut self, args: ArgsTuple) -> anyhow::Result<ReducerCallResult> {
        let stdb = &*self.database_instance_context().relational_db;
        stdb.with_auto_commit::<_, _, anyhow::Error>(|tx| {
            for table in self.info.catalog.values().filter_map(EntityDef::as_table) {
                let schema = self.schema_for(table)?;
                let result = stdb
                    .create_table(tx, schema)
                    .with_context(|| format!("failed to create table {}", table.name));
                if let Err(err) = result {
                    log::error!("{:?}", err);
                    return Err(err);
                }
            }
            Ok(())
        })?;

        let rcr = self
            .info
            .reducers
            .get_index_of(INIT_DUNDER)
            .map(|id| self.call_reducer(self.database_instance_context().identity, None, id, args))
            .unwrap_or(ReducerCallResult {
                outcome: ReducerOutcome::Committed,
                energy_used: EnergyDiff::ZERO,
                execution_duration: Duration::ZERO,
            });

        Ok(rcr)
    }

    #[tracing::instrument(skip_all)]
    fn update_database(&mut self) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let stdb = &*self.database_instance_context().relational_db;

        let mut tainted = vec![];
        stdb.with_auto_commit::<_, _, anyhow::Error>(|tx| {
            let mut known_tables: BTreeMap<String, TableSchema> = stdb
                .get_all_tables(tx)?
                .into_iter()
                .map(|schema| (schema.table_name.clone(), schema))
                .collect();

            let mut new_tables = Vec::new();
            for table in self.info.catalog.values().filter_map(EntityDef::as_table) {
                let mut proposed_schema = self.schema_for(table)?;
                if let Some(known_schema) = known_tables.remove(&table.name) {
                    // If the table is known, we also know its id. Update the
                    // index definitions so the `TableDef` of both schemas is
                    // equivalent.
                    for index in proposed_schema.indexes.iter_mut() {
                        index.table_id = known_schema.table_id;
                    }
                    let known_schema = TableDef::from(known_schema);
                    if known_schema != proposed_schema {
                        self.system_logger()
                            .warn(&format!("stored and proposed schema of `{}` differ", table.name));
                        tainted.push(table.name.to_owned());
                    } else {
                        // Table unchanged
                    }
                } else {
                    new_tables.push((table, proposed_schema));
                }
            }
            // We may at some point decide to drop orphaned tables automatically,
            // but for now it's an incompatible schema change
            for orphan in known_tables.into_keys() {
                if !orphan.starts_with("st_") {
                    self.system_logger()
                        .warn(format!("Orphaned table: {}", orphan).as_str());
                    tainted.push(orphan);
                }
            }
            if tainted.is_empty() {
                for (table, schema) in new_tables {
                    stdb.create_table(tx, schema)
                        .with_context(|| format!("failed to create table {}", table.name))?;
                }
            }

            Ok(())
        })?;
        if !tainted.is_empty() {
            self.system_logger()
                .error("module update rejected due to schema mismatch");
            return Ok(Err(UpdateDatabaseError::IncompatibleSchema { tables: tainted }));
        }

        let update_result = self.info.reducers.get_index_of(UPDATE_DUNDER).map(|id| {
            self.call_reducer(
                self.database_instance_context().identity,
                None,
                id,
                ArgsTuple::default(),
            )
        });

        Ok(Ok(UpdateDatabaseSuccess {
            update_result,
            migrate_results: vec![],
        }))
    }

    #[tracing::instrument(skip_all)]
    fn call_reducer(
        &mut self,
        caller_identity: Identity,
        client: Option<ClientConnectionSender>,
        reducer_id: usize,
        mut args: ArgsTuple,
    ) -> ReducerCallResult {
        let start_instant = Instant::now();

        let timestamp = Timestamp::now();

        let reducerdef = &self.info.reducers[reducer_id];

        log::trace!("Calling reducer {}", reducerdef.name);

        let (status, energy) = self.execute(InstanceOp::Reducer {
            id: reducer_id,
            sender: &caller_identity,
            timestamp,
            arg_bytes: args.get_bsatn().clone(),
        });

        let execution_duration = start_instant.elapsed();

        let outcome = ReducerOutcome::from(&status);

        let reducerdef = &self.info.reducers[reducer_id];
        let event = ModuleEvent {
            timestamp,
            caller_identity,
            function_call: ModuleFunctionCall {
                reducer: reducerdef.name.clone(),
                args,
            },
            status,
            energy_quanta_used: energy.used,
            host_execution_duration: execution_duration,
        };
        self.event_tx.broadcast_event_blocking(client.as_ref(), event);

        ReducerCallResult {
            outcome,
            energy_used: energy.used,
            execution_duration,
        }
    }

    #[tracing::instrument(skip_all)]
    fn call_connect_disconnect(&mut self, identity: Identity, connected: bool) {
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

        let (status, energy) = self.execute(InstanceOp::ConnDisconn {
            conn: connected,
            sender: &identity,
            timestamp,
        });

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
                args: ArgsTuple::default(),
            },
            status,
            caller_identity: identity,
            energy_quanta_used: energy.used,
            host_execution_duration: start_instant.elapsed(),
        };
        self.event_tx.broadcast_event_blocking(None, event);
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
    #[tracing::instrument(skip_all)]
    fn execute(&mut self, op: InstanceOp<'_>) -> (EventStatus, EnergyStats) {
        let address = &self.database_instance_context().address.to_abbreviated_hex();
        let func_ident = match op {
            InstanceOp::Reducer { id, .. } => &*self.info.reducers[id].name,
            InstanceOp::ConnDisconn { conn, .. } => {
                if conn {
                    IDENTITY_CONNECTED_DUNDER
                } else {
                    IDENTITY_DISCONNECTED_DUNDER
                }
            }
        };
        REDUCER_COUNT.with_label_values(&[address, func_ident]).inc();

        let energy_fingerprint = EnergyMonitorFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.identity,
            caller_identity: match op {
                InstanceOp::Reducer { sender, .. } | InstanceOp::ConnDisconn { sender, .. } => *sender,
            },
            reducer_name: func_ident,
        };

        let budget = self.energy_monitor.reducer_budget(&energy_fingerprint);

        let tx = self.database_instance_context().relational_db.begin_tx();

        let tx_slot = self.instance.instance_env().tx.clone();
        let (tx, result) = tx_slot.set(tx, || match op {
            InstanceOp::Reducer {
                id,
                sender,
                timestamp,
                arg_bytes,
            } => self
                .instance
                .call_reducer(id, budget, sender.as_bytes(), timestamp, arg_bytes),
            InstanceOp::ConnDisconn {
                conn,
                sender,
                timestamp,
            } => self
                .instance
                .call_connect_disconnect(conn, budget, sender.as_bytes(), timestamp),
        });

        let ExecuteResult {
            energy,
            execution_duration,
            call_result,
        } = result;

        self.energy_monitor
            .record(&energy_fingerprint, energy.used, execution_duration);

        const FRAME_LEN_60FPS: Duration = match Duration::from_secs(1).checked_div(60) {
            Some(d) => d,
            None => unreachable!(),
        };
        if execution_duration > FRAME_LEN_60FPS {
            // If we can't get your reducer done in a single frame
            // we should debug it.
            log::debug!("Long running reducer {func_ident:?} took {execution_duration:?} to execute");
        } else {
            log::trace!("Reducer {func_ident:?} ran: {execution_duration:?}, {:?}", energy.used);
        }

        REDUCER_COMPUTE_TIME
            .with_label_values(&[address, func_ident])
            .observe(execution_duration.as_secs_f64());

        // If you can afford to take 500 ms for a transaction
        // you can afford to generate a flamegraph. Fix your stuff.
        // if duration.as_millis() > 500 {
        //     if let Ok(report) = guard.report().build() {
        //         let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        //         let file = std::fs::File::create(format!("flamegraphs/flamegraph-{}.svg", now.as_millis())).unwrap();
        //         report.flamegraph(file).unwrap();
        //     };
        // }

        let stdb = &*self.database_instance_context().relational_db;
        let status = match call_result {
            Err(err) => {
                stdb.rollback_tx(tx);

                T::log_traceback("reducer", func_ident, &err);

                // discard this instance
                self.trapped = true;

                if energy.remaining == EnergyQuanta::ZERO {
                    EventStatus::OutOfEnergy
                } else {
                    EventStatus::Failed("The Wasm instance encountered a fatal error.".into())
                }
            }
            Ok(Err(errmsg)) => {
                stdb.rollback_tx(tx);

                log::info!("reducer returned error: {errmsg}");

                EventStatus::Failed(errmsg.into())
            }
            Ok(Ok(())) => {
                if let Some((tx_data, bytes_written)) = stdb.commit_tx(tx).unwrap() {
                    // TODO(cloutiertyler): This tracking doesn't really belong here if we want to write transactions to disk
                    // in batches. This is because it's possible for a tiny reducer call to trigger a whole commit to be written to disk.
                    // We should track the commit sizes instead internally to the CommitLog probably.
                    if let Some(bytes_written) = bytes_written {
                        REDUCER_WRITE_SIZE
                            .with_label_values(&[address, func_ident])
                            .observe(bytes_written as f64);
                    }
                    EventStatus::Committed(DatabaseUpdate::from_writes(stdb, &tx_data))
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        };
        (status, energy)
    }

    // Helpers - NOT API

    fn schema_for(&self, table: &spacetimedb_lib::TableDef) -> anyhow::Result<TableDef> {
        let schema = self
            .info
            .typespace
            .with_type(&table.data)
            .resolve_refs()
            .context("recursive types not yet supported")?;
        let schema = schema.into_product().ok().context("table not a product type?")?;
        anyhow::ensure!(
            table.column_attrs.len() == schema.elements.len(),
            "mismatched number of columns"
        );
        let columns: Vec<ColumnDef> = std::iter::zip(&schema.elements, &table.column_attrs)
            .map(|(ty, attr)| {
                Ok(ColumnDef {
                    col_name: ty.name.clone().context("column without name")?,
                    col_type: ty.algebraic_type.clone(),
                    is_autoinc: attr.is_autoinc(),
                })
            })
            .collect::<anyhow::Result<_>>()?;

        let mut indexes = Vec::new();
        for (col_id, col) in columns.iter().enumerate() {
            let mut index_for_column = None;
            for index in table.indexes.iter() {
                let [index_col_id] = *index.col_ids else {
                    anyhow::bail!("multi-column indexes not yet supported")
                };
                if index_col_id as usize != col_id {
                    continue;
                }
                index_for_column = Some(index);
                break;
            }

            let col_attr = table.column_attrs.get(col_id).context("invalid column id")?;
            // If there's an index defined for this column already, use it
            // making sure that it is unique if the column has a unique constraint
            if let Some(index) = index_for_column {
                match index.ty {
                    IndexType::BTree => {}
                    // TODO
                    IndexType::Hash => anyhow::bail!("hash indexes not yet supported"),
                }
                let index = IndexDef {
                    table_id: 0, // Will be ignored
                    col_id: col_id as u32,
                    name: index.name.clone(),
                    is_unique: col_attr.is_unique(),
                };
                indexes.push(index);
            } else if col_attr.is_unique() {
                // If you didn't find an index, but the column is unique then create a unique btree index
                // anyway.
                let index = IndexDef {
                    table_id: 0, // Will be ignored
                    col_id: col_id as u32,
                    name: format!("{}_{}_unique", table.name, col.col_name),
                    is_unique: true,
                };
                indexes.push(index);
            }
        }

        Ok(TableDef {
            table_name: table.name.clone(),
            columns,
            indexes,
            table_type: table.table_type,
            table_access: table.table_access,
        })
    }

    fn system_logger(&self) -> SystemLogger {
        let inner = self.database_instance_context().logger.lock().unwrap();
        SystemLogger { inner }
    }
}

#[derive(Debug)]
enum InstanceOp<'a> {
    Reducer {
        id: usize,
        sender: &'a Identity,
        timestamp: Timestamp,
        arg_bytes: Bytes,
    },
    ConnDisconn {
        conn: bool,
        sender: &'a Identity,
        timestamp: Timestamp,
    },
}
