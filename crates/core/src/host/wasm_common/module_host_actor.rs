use anyhow::{anyhow, Context};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;

use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{bsatn, Address, ModuleDef, ModuleValidationError, TableDesc};
use spacetimedb_vm::expr::CrudExpr;

use super::instrumentation::CallTimes;
use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::{LogLevel, Record, SystemLogger};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::IsolationLevel;
use crate::energy::{EnergyMonitor, EnergyQuanta, ReducerBudget, ReducerFingerprint};
use crate::execution_context::{self, ExecutionContext, ReducerContext};
use crate::host::instance_env::InstanceEnv;
use crate::host::module_host::{
    CallReducerParams, DatabaseUpdate, EventStatus, Module, ModuleEvent, ModuleFunctionCall, ModuleInfo,
    ModuleInstance, ReducersMap, UpdateDatabaseResult, UpdateDatabaseSuccess,
};
use crate::host::{ArgsTuple, EntityDef, ReducerCallResult, ReducerId, ReducerOutcome, Scheduler, Timestamp};
use crate::identity::Identity;
use crate::messages::control_db::Database;
use crate::module_host_context::ModuleCreationContext;
use crate::sql;
use crate::subscription::module_subscription_actor::{ModuleSubscriptions, WriteSkew};
use crate::util::const_unwrap;
use crate::worker_metrics::WORKER_METRICS;
use spacetimedb_sats::db::def::TableDef;

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
    fn instantiate(&self, env: InstanceEnv, func_names: &FuncNames) -> Result<Self::Instance, InitializationError>;
}

pub trait WasmInstance: Send + Sync + 'static {
    fn extract_descriptions(&mut self) -> Result<Bytes, DescribeError>;

    fn instance_env(&self) -> &InstanceEnv;

    type Trap: Send;

    fn call_reducer(&mut self, op: ReducerOp<'_>, budget: ReducerBudget) -> ExecuteResult<Self::Trap>;

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap);
}

pub struct EnergyStats {
    pub used: EnergyQuanta,
    pub remaining: ReducerBudget,
}

pub struct ExecutionTimings {
    pub total_duration: Duration,
    pub wasm_instance_env_call_times: CallTimes,
}

pub struct ExecuteResult<E> {
    pub energy: EnergyStats,
    pub timings: ExecutionTimings,
    pub call_result: Result<Result<(), Box<str>>, E>,
}

pub(crate) struct WasmModuleHostActor<T: WasmModule> {
    module: T::InstancePre,
    initial_instance: Option<Box<WasmModuleInstance<T::Instance>>>,
    database_instance_context: Arc<DatabaseInstanceContext>,
    scheduler: Scheduler,
    func_names: Arc<FuncNames>,
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
}

#[derive(thiserror::Error, Debug)]
pub enum InitializationError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    ModuleValidation(#[from] ModuleValidationError),
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

impl From<TypeRefError> for InitializationError {
    fn from(err: TypeRefError) -> Self {
        ValidationError::from(err).into()
    }
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
    pub fn new(mcc: ModuleCreationContext, module: T) -> Result<Self, InitializationError> {
        let ModuleCreationContext {
            dbic: database_instance_context,
            scheduler,
            program_bytes: _,
            program_hash: module_hash,
            energy_monitor,
        } = mcc;
        log::trace!(
            "Making new module host actor for database {}",
            database_instance_context.address
        );
        let log_tx = database_instance_context.logger.tx.clone();

        FuncNames::check_required(|name| module.get_export(name))?;
        let mut func_names = FuncNames::default();
        module.for_each_export(|sym, ty| func_names.update_from_general(sym, ty))?;
        func_names.preinits.sort_unstable();

        let owner_identity = database_instance_context.identity;
        let relational_db = database_instance_context.relational_db.clone();
        let subscriptions = ModuleSubscriptions::new(relational_db, owner_identity);

        let uninit_instance = module.instantiate_pre()?;
        let mut instance = uninit_instance.instantiate(
            InstanceEnv::new(database_instance_context.clone(), scheduler.clone()),
            &func_names,
        )?;

        let desc = instance.extract_descriptions()?;
        let desc: ModuleDef = bsatn::from_slice(&desc).map_err(DescribeError::Decode)?;
        desc.validate_reducers()?;
        let ModuleDef {
            mut typespace,
            mut tables,
            reducers,
            misc_exports: _,
        } = desc;
        // Tables can't handle typerefs, let alone recursive types, so we need
        // to walk over the columns and inline all typerefs as the resolved
        // types to prevent runtime panics when trying to e.g. insert rows.
        // TODO: support type references properly in the future.
        for table in &mut tables {
            for col in &mut table.schema.columns {
                typespace.inline_typerefs_in_type(&mut col.col_type)?;
            }
        }
        let catalog = itertools::chain(
            tables
                .into_iter()
                .map(|x| (x.schema.table_name.clone(), EntityDef::Table(x))),
            reducers
                .iter()
                .filter(|r| !(r.name.starts_with("__") && r.name.ends_with("__")))
                .map(|x| (x.name.clone(), EntityDef::Reducer(x.clone()))),
        )
        .collect();
        let reducers = ReducersMap(reducers.into_iter().map(|x| (x.name.clone(), x)).collect());

        let info = Arc::new(ModuleInfo {
            identity: database_instance_context.identity,
            address: database_instance_context.address,
            module_hash,
            typespace,
            reducers,
            catalog,
            log_tx,
            subscriptions,
        });

        let func_names = Arc::new(func_names);
        let mut module = WasmModuleHostActor {
            module: uninit_instance,
            initial_instance: None,
            func_names,
            info,
            database_instance_context,
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
        let env = InstanceEnv::new(self.database_instance_context.clone(), self.scheduler.clone());
        // this shouldn't fail, since we already called module.create_instance()
        // before and it didn't error, and ideally they should be deterministic
        let mut instance = self
            .module
            .instantiate(env, &self.func_names)
            .expect("failed to initialize instance");
        let _ = instance.extract_descriptions();
        self.make_from_instance(instance)
    }

    fn dbic(&self) -> &DatabaseInstanceContext {
        &self.database_instance_context
    }

    fn inject_logs(&self, log_level: LogLevel, message: &str) {
        self.database_instance_context.logger.write(
            log_level,
            &Record {
                ts: chrono::Utc::now(),
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

    #[tracing::instrument(skip_all)]
    fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<spacetimedb_vm::relation::MemTable>, DBError> {
        let db = &self.database_instance_context.relational_db;
        let auth = AuthCtx::new(self.database_instance_context.identity, caller_identity);
        log::debug!("One-off query: {query}");
        // Don't need the `slow query` logger on compilation
        let ctx = &ExecutionContext::sql(db.address(), db.read_config().slow_query);
        let compiled: Vec<_> = db.with_read_only(ctx, |tx| {
            let ast = sql::compiler::compile_sql(db, tx, &query)?;
            ast.into_iter()
                .map(|expr| {
                    if matches!(expr, CrudExpr::Query { .. }) {
                        Ok(expr)
                    } else {
                        Err(anyhow!("One-off queries are not allowed to modify the database"))
                    }
                })
                .collect::<Result<_, _>>()
        })?;

        sql::execute::execute_sql(db, &query, compiled, auth)
    }

    fn clear_table(&self, table_name: &str) -> Result<(), anyhow::Error> {
        let db = &*self.database_instance_context.relational_db;
        db.with_auto_commit(&ExecutionContext::internal(db.address()), |tx| {
            let tables = db.get_all_tables_mut(tx)?;
            // We currently have unique table names,
            // so we can assume there's only one table to clear.
            if let Some(table_id) = tables
                .iter()
                .find_map(|t| (&*t.table_name == table_name).then_some(t.table_id))
            {
                db.clear_table(tx, table_id)?;
            }
            Ok(())
        })
    }
}

pub struct WasmModuleInstance<T: WasmInstance> {
    instance: T,
    info: Arc<ModuleInfo>,
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

fn get_tabledefs(info: &ModuleInfo) -> impl Iterator<Item = anyhow::Result<TableDef>> + '_ {
    info.catalog
        .values()
        .filter_map(EntityDef::as_table)
        .map(|table| TableDesc::into_table_def(info.typespace.with_type(table)))
}

impl<T: WasmInstance> ModuleInstance for WasmModuleInstance<T> {
    fn trapped(&self) -> bool {
        self.trapped
    }

    #[tracing::instrument(skip(self, args), fields(db_id = self.instance.instance_env().dbic.id))]
    fn init_database(&mut self, fence: u128, args: ArgsTuple) -> anyhow::Result<Option<ReducerCallResult>> {
        let timestamp = Timestamp::now();
        let stdb = &*self.database_instance_context().relational_db;
        let ctx = ExecutionContext::internal(stdb.address());
        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let (tx, ()) = stdb
            .with_auto_rollback(&ctx, tx, |tx| {
                for schema in get_tabledefs(&self.info) {
                    let schema = schema?;
                    let table_name = schema.table_name.clone();
                    self.system_logger().info(&format!("Creating table `{table_name}`"));
                    stdb.create_table(tx, schema)
                        .with_context(|| format!("failed to create table {table_name}"))?;
                }
                // Set the module hash. Morally, this should be done _after_ calling
                // the `init` reducer, but that consumes our transaction context.
                stdb.set_program_hash(tx, fence, self.info.module_hash)?;
                anyhow::Ok(())
            })
            .inspect_err(|e| log::error!("{e:?}"))?;

        let rcr = match self.info.reducers.lookup_id(INIT_DUNDER) {
            None => {
                stdb.commit_tx(&ctx, tx)?;
                None
            }

            Some(reducer_id) => {
                self.system_logger().info("Invoking `init` reducer");
                // If a caller address was passed to the `/database/publish` HTTP endpoint,
                // the init/update reducer will receive it as the caller address.
                // This is useful for bootstrapping the control DB in SpacetimeDB-cloud.
                let Database {
                    identity: caller_identity,
                    publisher_address: caller_address,
                    ..
                } = self.database_instance_context().database;
                let client = None;
                Some(self.call_reducer_with_tx(
                    Some(tx),
                    CallReducerParams {
                        timestamp,
                        caller_identity,
                        caller_address: caller_address.unwrap_or(Address::__DUMMY),
                        client,
                        request_id: None,
                        timer: None,
                        reducer_id,
                        args,
                    },
                ))
            }
        };

        self.system_logger().info("Database initialized");

        Ok(rcr)
    }

    #[tracing::instrument(skip_all)]
    fn update_database(&mut self, fence: u128) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let timestamp = Timestamp::now();

        let proposed_tables = get_tabledefs(&self.info).collect::<anyhow::Result<Vec<_>>>()?;

        let stdb = &*self.database_instance_context().relational_db;
        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let res = crate::db::update::update_database(
            stdb,
            tx,
            proposed_tables,
            fence,
            self.info.module_hash,
            self.system_logger(),
        )?;
        let tx = match res {
            Ok(tx) => tx,
            Err(e) => return Ok(Err(e)),
        };

        let update_result = match self.info.reducers.lookup_id(UPDATE_DUNDER) {
            None => {
                stdb.commit_tx(&ExecutionContext::internal(stdb.address()), tx)?;
                None
            }

            Some(reducer_id) => {
                self.system_logger().info("Invoking `update` reducer");
                // If a caller address was passed to the `/database/publish` HTTP endpoint,
                // the init/update reducer will receive it as the caller address.
                // This is useful for bootstrapping the control DB in SpacetimeDB-cloud.
                let Database {
                    identity: caller_identity,
                    publisher_address: caller_address,
                    ..
                } = self.database_instance_context().database;
                let res = self.call_reducer_with_tx(
                    Some(tx),
                    CallReducerParams {
                        timestamp,
                        caller_identity,
                        caller_address: caller_address.unwrap_or(Address::__DUMMY),
                        client: None,
                        request_id: None,
                        timer: None,
                        reducer_id,
                        args: ArgsTuple::nullary(),
                    },
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

    fn call_reducer(&mut self, params: CallReducerParams) -> ReducerCallResult {
        crate::callgrind_flag::invoke_allowing_callgrind(|| self.call_reducer_with_tx(None, params))
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
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
    /// The method also performs various measurements and records energy usage,
    /// as well as broadcasting a [`ModuleEvent`] containg information about
    /// the outcome of the call.
    #[tracing::instrument(skip_all)]
    fn call_reducer_with_tx(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        let CallReducerParams {
            timestamp,
            caller_identity,
            caller_address,
            client,
            request_id,
            reducer_id,
            args,
            timer,
        } = params;
        let caller_address_opt = (caller_address != Address::__DUMMY).then_some(caller_address);

        let dbic = self.database_instance_context();
        let stdb = &*dbic.relational_db.clone();
        let address = dbic.address;
        let reducer_name = &*self.info.reducers[reducer_id].name;

        let _outer_span = tracing::trace_span!("call_reducer",
            reducer_name,
            %caller_identity,
            caller_address = caller_address_opt.map(tracing::field::debug),
        )
        .entered();

        let energy_fingerprint = ReducerFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.identity,
            caller_identity,
            reducer_name,
        };
        let budget = self.energy_monitor.reducer_budget(&energy_fingerprint);

        let op = ReducerOp {
            id: reducer_id,
            name: reducer_name,
            caller_identity: &caller_identity,
            caller_address: &caller_address,
            timestamp,
            arg_bytes: args.get_bsatn().clone(),
        };

        let tx = tx.unwrap_or_else(|| stdb.begin_mut_tx(IsolationLevel::Serializable));
        let mut tx_slot = self.instance.instance_env().tx.clone();

        let reducer_span = tracing::trace_span!(
            "run_reducer",
            timings.total_duration = tracing::field::Empty,
            energy.budget = budget.get(),
            energy.used = tracing::field::Empty,
        )
        .entered();
        let ctx = ExecutionContext::reducer(address, ReducerContext::from(op.clone()));
        // run the call_reducer call in rayon. it's important that we don't acquire a lock inside a rayon task,
        // as that can lead to deadlock.
        let (ctx, tx, result) = rayon::scope(|_| tx_slot.set(ctx, tx, || self.instance.call_reducer(op, budget)));

        let ExecuteResult {
            energy,
            timings,
            call_result,
        } = result;

        self.energy_monitor
            .record_reducer(&energy_fingerprint, energy.used, timings.total_duration);

        reducer_span
            .record("timings.total_duration", tracing::field::debug(timings.total_duration))
            .record("energy.used", tracing::field::debug(energy.used));

        const FRAME_LEN_60FPS: Duration = const_unwrap(Duration::from_secs(1).checked_div(60));
        if timings.total_duration > FRAME_LEN_60FPS {
            // If we can't get your reducer done in a single frame we should debug it.
            tracing::debug!(
                message = "Long running reducer finished executing",
                reducer_name,
                ?timings.total_duration,
            );
        }
        reducer_span.exit();

        let status = match call_result {
            Err(err) => {
                T::log_traceback("reducer", reducer_name, &err);

                WORKER_METRICS
                    .wasm_instance_errors
                    .with_label_values(&caller_identity, &self.info.module_hash, &caller_address, reducer_name)
                    .inc();

                // discard this instance
                self.trapped = true;

                if energy.remaining.get() == 0 {
                    EventStatus::OutOfEnergy
                } else {
                    EventStatus::Failed("The Wasm instance encountered a fatal error.".into())
                }
            }
            Ok(Err(errmsg)) => {
                log::info!("reducer returned error: {errmsg}");

                EventStatus::Failed(errmsg.into())
            }
            Ok(Ok(())) => EventStatus::Committed(DatabaseUpdate::default()),
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            caller_address: caller_address_opt,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_owned(),
                args,
            },
            status,
            energy_quanta_used: energy.used,
            host_execution_duration: timings.total_duration,
            request_id,
            timer,
        };
        let event = match self
            .info
            .subscriptions
            .commit_and_broadcast_event(client.as_deref(), event, &ctx, tx)
            .unwrap()
        {
            Ok(ev) => ev,
            Err(WriteSkew) => todo!("Write skew, you need to implement retries my man, T-dawg."),
        };

        ReducerCallResult {
            outcome: ReducerOutcome::from(&event.status),
            energy_used: energy.used,
            execution_duration: timings.total_duration,
        }
    }

    // Helpers - NOT API
    fn system_logger(&self) -> &SystemLogger {
        self.database_instance_context().logger.system_logger()
    }
}

/// Describes a reducer call in a cheaply shareable way.
#[derive(Clone, Debug)]
pub struct ReducerOp<'a> {
    pub id: ReducerId,
    pub name: &'a str,
    pub caller_identity: &'a Identity,
    pub caller_address: &'a Address,
    pub timestamp: Timestamp,
    /// The BSATN-serialized arguments passed to the reducer.
    pub arg_bytes: Bytes,
}

impl From<ReducerOp<'_>> for execution_context::ReducerContext {
    fn from(
        ReducerOp {
            id: _,
            name,
            caller_identity,
            caller_address,
            timestamp,
            arg_bytes,
        }: ReducerOp<'_>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            caller_identity: *caller_identity,
            caller_address: *caller_address,
            timestamp,
            arg_bsatn: arg_bytes.clone(),
        }
    }
}
