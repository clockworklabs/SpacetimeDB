use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::{ColumnDef, IndexDef, IndexId, TableDef, TableSchema};
use crate::host::scheduler::Scheduler;
use crate::sql;
use anyhow::Context;
use bytes::Bytes;
use nonempty::NonEmpty;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{bsatn, Address, IndexType, ModuleDef};
use spacetimedb_vm::expr::CrudExpr;

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
        sender_identity: &Identity,
        sender_address: &Address,
        timestamp: Timestamp,
        arg_bytes: Bytes,
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
            address: database_instance_context.address,
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

    fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<spacetimedb_lib::relation::MemTable>, DBError> {
        let db = &self.worker_database_instance.relational_db;
        let auth = AuthCtx::new(self.worker_database_instance.identity, caller_identity);
        // TODO(jgilles): make this a read-only TX when those get added

        db.with_read_only(|tx| {
            log::debug!("One-off query: {query}");
            // NOTE(jgilles): this returns errors about mutating queries as SubscriptionErrors, which is perhaps
            // mildly confusing, since the user did not subscribe to anything. Should we rename SubscriptionError to ReadOnlyQueryError?
            let compiled = crate::subscription::query::compile_read_only_query(db, tx, &auth, &query)?;

            sql::execute::execute_sql(
                db,
                tx,
                compiled.queries.into_iter().map(CrudExpr::Query).collect(),
                auth,
            )
        })
    }

    fn clear_table(&self, table_name: String) -> Result<(), anyhow::Error> {
        let db = &*self.worker_database_instance.relational_db;
        db.with_auto_commit(|tx| {
            let tables = db.get_all_tables(tx)?;
            for table in tables {
                if table.table_name != table_name {
                    continue;
                }

                db.clear_table(tx, table.table_id)?;
            }
            Ok(())
        })
    }
}

/// Somewhat ad-hoc wrapper around [`DatabaseLogger`] which allows to inject
/// "system messages" into the user-retrievable database / module log
struct SystemLogger<'a> {
    inner: std::sync::MutexGuard<'a, DatabaseLogger>,
}

impl SystemLogger<'_> {
    fn info(&mut self, msg: &str) {
        self.inner
            .write(crate::database_logger::LogLevel::Info, &Self::record(msg), &())
    }

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

    #[tracing::instrument(skip(self, args), fields(db_id=self.instance.instance_env().dbic.database_id))]
    fn init_database(&mut self, fence: u128, args: ArgsTuple) -> anyhow::Result<ReducerCallResult> {
        let stdb = &*self.database_instance_context().relational_db;
        let mut tx = stdb.begin_tx();
        for table in self.info.catalog.values().filter_map(EntityDef::as_table) {
            self.system_logger().info(&format!("Creating table `{}`", table.name));
            tx = stdb
                .with_auto_rollback(tx, |tx| {
                    let schema = self.schema_for(table)?;
                    stdb.create_table(tx, schema)
                        .with_context(|| format!("failed to create table {}", table.name))
                })
                .map(|(tx, _)| tx)
                .map_err(|e| {
                    log::error!("{e:?}");
                    e
                })?;
        }

        // Set the module hash. Morally, this should be done _after_ calling
        // the `init` reducer, but that consumes our transaction context.
        tx = stdb
            .with_auto_rollback(tx, |tx| stdb.set_program_hash(tx, fence, self.info.module_hash))
            .map(|(tx, ())| tx)?;

        let rcr = match self.info.reducers.get_index_of(INIT_DUNDER) {
            None => {
                stdb.commit_tx(tx)?;
                ReducerCallResult {
                    outcome: ReducerOutcome::Committed,
                    energy_used: EnergyDiff::ZERO,
                    execution_duration: Duration::ZERO,
                }
            }

            Some(reducer_id) => {
                self.system_logger().info("Invoking `init` reducer");
                let caller_identity = self.database_instance_context().identity;
                // If a caller address was passed to the `/database/publish` HTTP endpoint,
                // the init/update reducer will receive it as the caller address.
                // This is useful for bootstrapping the control DB in SpacetimeDB-cloud.
                let caller_address = self.database_instance_context().publisher_address;
                let client = None;
                self.call_reducer_internal(Some(tx), caller_identity, caller_address, client, reducer_id, args)
            }
        };

        self.system_logger().info("Database initialized");

        Ok(rcr)
    }

    #[tracing::instrument(skip_all)]
    fn update_database(&mut self, fence: u128) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let stdb = &*self.database_instance_context().relational_db;
        let mut tx = stdb.begin_tx();

        let (tx0, updates) = stdb.with_auto_rollback::<_, _, anyhow::Error>(tx, |tx| self.schema_updates(tx))?;
        tx = tx0;
        if updates.tainted_tables.is_empty() {
            tx = stdb
                .with_auto_rollback::<_, _, DBError>(tx, |tx| {
                    for (name, schema) in updates.new_tables {
                        self.system_logger().info(&format!("Creating table `{}`", name));
                        stdb.create_table(tx, schema)
                            .with_context(|| format!("failed to create table {}", name))?;
                    }

                    for index_id in updates.indexes_to_drop {
                        self.system_logger()
                            .info(&format!("Dropping index with id {}", index_id.0));
                        stdb.drop_index(tx, index_id)?;
                    }

                    for index_def in updates.indexes_to_create {
                        self.system_logger()
                            .info(&format!("Creating index `{}`", index_def.name));
                        stdb.create_index(tx, index_def)?;
                    }

                    Ok(())
                })
                .map(|(tx, ())| tx)?;
        } else {
            stdb.rollback_tx(tx);
            self.system_logger()
                .error("Module update rejected due to schema mismatch");
            return Ok(Err(UpdateDatabaseError::IncompatibleSchema {
                tables: updates.tainted_tables,
            }));
        }

        // Update the module hash. Morally, this should be done _after_ calling
        // the `update` reducer, but that consumes our transaction context.
        tx = stdb
            .with_auto_rollback(tx, |tx| stdb.set_program_hash(tx, fence, self.info.module_hash))
            .map(|(tx, ())| tx)?;

        let update_result = match self.info.reducers.get_index_of(UPDATE_DUNDER) {
            None => {
                stdb.commit_tx(tx)?;
                None
            }

            Some(reducer_id) => {
                self.system_logger().info("Invoking `update` reducer");
                let caller_identity = self.database_instance_context().identity;
                // If a caller address was passed to the `/database/publish` HTTP endpoint,
                // the init/update reducer will receive it as the caller address.
                // This is useful for bootstrapping the control DB in SpacetimeDB-cloud.
                let caller_address = self.database_instance_context().publisher_address;
                let client = None;
                let res = self.call_reducer_internal(
                    Some(tx),
                    caller_identity,
                    caller_address,
                    client,
                    reducer_id,
                    ArgsTuple::default(),
                );
                Some(res)
            }
        };

        self.system_logger().info("Database updated");

        Ok(Ok(UpdateDatabaseSuccess {
            update_result,
            migrate_results: vec![],
        }))
    }

    #[tracing::instrument(skip_all)]
    fn call_reducer(
        &mut self,
        caller_identity: Identity,
        caller_address: Option<Address>,
        client: Option<ClientConnectionSender>,
        reducer_id: usize,
        args: ArgsTuple,
    ) -> ReducerCallResult {
        self.call_reducer_internal(None, caller_identity, caller_address, client, reducer_id, args)
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
    /// Call a reducer.
    ///
    /// This is semantically the same as the trait method
    /// [`ModuleInstance::call_reducer`], but allows to supply an optional
    /// transaction context `tx`. If this context is `None`, a fresh transaction
    /// is started.
    ///
    /// **Note** that the transaction context is consumed, i.e. committed or
    /// rolled back as appropriate.
    ///
    /// Apart from executing the reducer via [`Self::execute`], this method will
    /// broadcast a [`ModuleEvent`] containg information about the outcome of
    /// the call.
    ///
    /// See also: [`Self::execute`]
    fn call_reducer_internal(
        &mut self,
        tx: Option<MutTxId>,
        caller_identity: Identity,
        caller_address: Option<Address>,
        client: Option<ClientConnectionSender>,
        reducer_id: usize,
        mut args: ArgsTuple,
    ) -> ReducerCallResult {
        let start_instant = Instant::now();

        let timestamp = Timestamp::now();

        let reducerdef = &self.info.reducers[reducer_id];

        log::trace!("Calling reducer {}", reducerdef.name);

        let (status, energy) = self.execute(
            tx,
            ReducerOp {
                id: reducer_id,
                sender_identity: &caller_identity,
                sender_address: &caller_address.unwrap_or(Address::__dummy()),
                timestamp,
                arg_bytes: args.get_bsatn().clone(),
            },
        );

        let execution_duration = start_instant.elapsed();

        let outcome = ReducerOutcome::from(&status);

        let reducerdef = &self.info.reducers[reducer_id];
        let event = ModuleEvent {
            timestamp,
            caller_identity,
            caller_address,
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

    /// Execute a reducer.
    ///
    /// If `Some` [`MutTxId`] is supplied, the reducer is called within the
    /// context of this transaction. Otherwise, a fresh transaction is started.
    ///
    /// **Note** that the transaction is committed or rolled back by this method
    /// depending on the outcome of the reducer call.
    //
    // TODO(kim): This should probably change in the future. The reason it is
    // not straightforward is that the returned [`EventStatus`] is constructed
    // from transaction data in the [`EventStatus::Committed`] (i.e. success)
    // case.
    //
    /// The method also performs various measurements and records energy usage.
    #[tracing::instrument(skip_all)]
    fn execute(&mut self, tx: Option<MutTxId>, op: ReducerOp<'_>) -> (EventStatus, EnergyStats) {
        let address = &self.database_instance_context().address.to_abbreviated_hex();
        let func_ident = &*self.info.reducers[op.id].name;
        REDUCER_COUNT.with_label_values(&[address, func_ident]).inc();

        let energy_fingerprint = EnergyMonitorFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.identity,
            caller_identity: *op.sender_identity,
            reducer_name: func_ident,
        };

        let budget = self.energy_monitor.reducer_budget(&energy_fingerprint);

        let tx = tx.unwrap_or_else(|| self.database_instance_context().relational_db.begin_tx());

        let tx_slot = self.instance.instance_env().tx.clone();
        let (tx, result) = tx_slot.set(tx, || {
            self.instance.call_reducer(
                op.id,
                budget,
                op.sender_identity,
                op.sender_address,
                op.timestamp,
                op.arg_bytes,
            )
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
                    //Ignore multi-column indexes
                    continue;
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
                    cols: NonEmpty::new(col_id as u32),
                    name: index.name.clone(),
                    is_unique: col_attr.is_unique(),
                };
                indexes.push(index);
            } else if col_attr.is_unique() {
                // If you didn't find an index, but the column is unique then create a unique btree index
                // anyway.
                let index = IndexDef {
                    table_id: 0, // Will be ignored
                    cols: NonEmpty::new(col_id as u32),
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

    /// Compute the diff between the current and proposed schema.
    fn schema_updates(&self, tx: &MutTxId) -> anyhow::Result<SchemaUpdates> {
        let stdb = &*self.database_instance_context().relational_db;

        // Until we know how to migrate schemas, we only accept `TableDef`s for
        // existing tables which are equal sans their indexes.
        struct Equiv<'a>(&'a TableDef);
        impl PartialEq for Equiv<'_> {
            fn eq(&self, other: &Self) -> bool {
                let TableDef {
                    table_name,
                    columns,
                    indexes: _,
                    table_type,
                    table_access,
                } = &self.0;
                table_name == &other.0.table_name
                    && table_type == &other.0.table_type
                    && table_access == &other.0.table_access
                    && columns == &other.0.columns
            }
        }

        let mut new_tables = HashMap::new();
        let mut tainted_tables = Vec::new();
        let mut indexes_to_create = Vec::new();
        let mut indexes_to_drop = Vec::new();

        let mut known_tables: BTreeMap<String, TableSchema> = stdb
            .get_all_tables(tx)?
            .into_iter()
            .map(|schema| (schema.table_name.clone(), schema))
            .collect();

        for table in self.info.catalog.values().filter_map(EntityDef::as_table) {
            let proposed_schema_def = self.schema_for(table)?;
            if let Some(known_schema) = known_tables.remove(&table.name) {
                let table_id = known_schema.table_id;
                let known_schema_def = TableDef::from(known_schema.clone());
                // If the schemas differ acc. to [Equiv], the update should be
                // rejected.
                if Equiv(&known_schema_def) != Equiv(&proposed_schema_def) {
                    self.system_logger()
                        .warn(&format!("stored and proposed schema of `{}` differ", table.name));
                    tainted_tables.push(table.name.to_owned());
                } else {
                    // The schema is unchanged, but maybe the indexes are.
                    let mut known_indexes = known_schema
                        .indexes
                        .into_iter()
                        .map(|idx| (idx.index_name.clone(), idx))
                        .collect::<BTreeMap<_, _>>();

                    for mut index_def in proposed_schema_def.indexes {
                        // This is zero in the proposed schema, as the table id
                        // is not known at proposal time.
                        index_def.table_id = table_id;

                        match known_indexes.remove(&index_def.name) {
                            None => indexes_to_create.push(index_def),
                            Some(known_index) => {
                                let known_id = IndexId(known_index.index_id);
                                let known_index_def = IndexDef::from(known_index);
                                if known_index_def != index_def {
                                    indexes_to_drop.push(known_id);
                                    indexes_to_create.push(index_def);
                                }
                            }
                        }
                    }

                    // Indexes not in the proposed schema shall be dropped.
                    for index in known_indexes.into_values() {
                        indexes_to_drop.push(IndexId(index.index_id));
                    }
                }
            } else {
                new_tables.insert(table.name.to_owned(), proposed_schema_def);
            }
        }
        // We may at some point decide to drop orphaned tables automatically,
        // but for now it's an incompatible schema change
        for orphan in known_tables.into_keys() {
            if !orphan.starts_with("st_") {
                self.system_logger()
                    .warn(format!("Orphaned table: {}", orphan).as_str());
                tainted_tables.push(orphan);
            }
        }

        Ok(SchemaUpdates {
            new_tables,
            tainted_tables,
            indexes_to_drop,
            indexes_to_create,
        })
    }
}

struct SchemaUpdates {
    /// Tables to create.
    new_tables: HashMap<String, TableDef>,
    /// Names of tables with incompatible schema updates.
    tainted_tables: Vec<String>,
    /// Indexes to drop.
    ///
    /// Should be processed _before_ `indexes_to_create`, as we might be
    /// updating (i.e. drop then create with different parameters).
    indexes_to_drop: Vec<IndexId>,
    /// Indexes to create.
    ///
    /// Should be processed _after_ `indexes_to_drop`.
    indexes_to_create: Vec<IndexDef>,
}

#[derive(Debug)]
struct ReducerOp<'a> {
    id: usize,
    sender_identity: &'a Identity,
    sender_address: &'a Address,
    timestamp: Timestamp,
    arg_bytes: Bytes,
}
