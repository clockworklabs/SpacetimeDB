use anyhow::Context;
use bytes::Bytes;
use spacetimedb_client_api_messages::timestamp::Timestamp;
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_primitives::TableId;
use spacetimedb_schema::auto_migrate::ponder_migrate;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::schema::{Schema, TableSchema};
use std::sync::Arc;
use std::time::Duration;

use super::instrumentation::CallTimes;
use crate::database_logger::{self, SystemLogger};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::system_tables::{StClientRow, ST_CLIENT_ID};
use crate::db::datastore::traits::{IsolationLevel, Program};
use crate::energy::{EnergyMonitor, EnergyQuanta, ReducerBudget, ReducerFingerprint};
use crate::execution_context::{self, ReducerContext, Workload};
use crate::host::instance_env::InstanceEnv;
use crate::host::module_host::{
    CallReducerParams, DatabaseUpdate, EventStatus, Module, ModuleEvent, ModuleFunctionCall, ModuleInfo, ModuleInstance,
};
use crate::host::{ArgsTuple, ReducerCallResult, ReducerId, ReducerOutcome, Scheduler, UpdateDatabaseResult};
use crate::identity::Identity;
use crate::messages::control_db::HostType;
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::sql::parser::RowLevelExpr;
use crate::subscription::module_subscription_actor::WriteConflict;
use crate::util::prometheus_handle::HistogramExt;
use crate::worker_metrics::WORKER_METRICS;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{bsatn, Address, RawModuleDef};

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
    fn extract_descriptions(&mut self) -> Result<Vec<u8>, DescribeError>;

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
    #[expect(unused)] // TODO: do we want to do something with this?
    pub wasm_instance_env_call_times: CallTimes,
}

pub struct ExecuteResult<E> {
    pub energy: EnergyStats,
    pub timings: ExecutionTimings,
    pub memory_allocation: usize,
    pub call_result: Result<Result<(), Box<str>>, E>,
}

pub(crate) struct WasmModuleHostActor<T: WasmModule> {
    module: T::InstancePre,
    initial_instance: Option<Box<WasmModuleInstance<T::Instance>>>,
    replica_context: Arc<ReplicaContext>,
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
    ModuleValidation(#[from] spacetimedb_schema::error::ValidationErrors),
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
    #[error("unimplemented RawModuleDef version")]
    UnimplementedRawModuleDefVersion,
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn new(mcc: ModuleCreationContext, module: T) -> Result<Self, InitializationError> {
        let ModuleCreationContext {
            replica_ctx: replica_context,
            scheduler,
            program,
            energy_monitor,
        } = mcc;
        let module_hash = program.hash;
        log::trace!(
            "Making new module host actor for database {} with module {}",
            replica_context.database_identity,
            module_hash,
        );
        let log_tx = replica_context.logger.tx.clone();

        FuncNames::check_required(|name| module.get_export(name))?;
        let mut func_names = FuncNames::default();
        module.for_each_export(|sym, ty| func_names.update_from_general(sym, ty))?;
        func_names.preinits.sort_unstable();

        let uninit_instance = module.instantiate_pre()?;
        let mut instance = uninit_instance.instantiate(
            InstanceEnv::new(replica_context.clone(), scheduler.clone()),
            &func_names,
        )?;

        let desc = instance.extract_descriptions()?;
        let desc: RawModuleDef = bsatn::from_slice(&desc).map_err(DescribeError::Decode)?;

        // Perform a bunch of validation on the raw definition.
        let def: ModuleDef = desc.try_into()?;

        // Note: assigns Reducer IDs based on the alphabetical order of reducer names.
        let info = ModuleInfo::new(
            def,
            replica_context.owner_identity,
            replica_context.database_identity,
            module_hash,
            log_tx,
            replica_context.subscriptions.clone(),
        );

        let func_names = Arc::new(func_names);
        let mut module = WasmModuleHostActor {
            module: uninit_instance,
            initial_instance: None,
            func_names,
            info,
            replica_context,
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
            // will be updated on the first reducer call
            allocated_memory: 0,
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
        let env = InstanceEnv::new(self.replica_context.clone(), self.scheduler.clone());
        // this shouldn't fail, since we already called module.create_instance()
        // before and it didn't error, and ideally they should be deterministic
        let mut instance = self
            .module
            .instantiate(env, &self.func_names)
            .expect("failed to initialize instance");
        let _ = instance.extract_descriptions();
        self.make_from_instance(instance)
    }

    fn replica_ctx(&self) -> &ReplicaContext {
        &self.replica_context
    }

    fn close(self) {
        self.scheduler.close()
    }
}

pub struct WasmModuleInstance<T: WasmInstance> {
    instance: T,
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
    allocated_memory: usize,
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
    fn replica_context(&self) -> &ReplicaContext {
        &self.instance.instance_env().replica_ctx
    }
}

impl<T: WasmInstance> ModuleInstance for WasmModuleInstance<T> {
    fn trapped(&self) -> bool {
        self.trapped
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        err
        fields(db_id = self.instance.instance_env().replica_ctx.id),
    )]
    fn init_database(&mut self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        log::debug!("init database");
        let timestamp = Timestamp::now();
        let stdb = &*self.replica_context().relational_db;

        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let auth_ctx = AuthCtx::for_current(self.replica_context().database.owner_identity);
        let (tx, ()) = stdb
            .with_auto_rollback(tx, |tx| {
                let mut table_defs: Vec<_> = self.info.module_def.tables().collect();
                table_defs.sort_by(|a, b| a.name.cmp(&b.name));

                for def in table_defs {
                    let table_name = &def.name;
                    self.system_logger().info(&format!("Creating table `{table_name}`"));
                    let schema = TableSchema::from_module_def(&self.info.module_def, def, (), TableId::SENTINEL);
                    stdb.create_table(tx, schema)
                        .with_context(|| format!("failed to create table {table_name}"))?;
                }
                // Insert the late-bound row-level security expressions.
                for rls in self.info.module_def.row_level_security() {
                    self.system_logger()
                        .info(&format!("Creating row level security `{}`", rls.sql));

                    let rls = RowLevelExpr::build_row_level_expr(tx, &auth_ctx, rls)
                        .with_context(|| format!("failed to create row-level security: `{}`", rls.sql))?;
                    let table_id = rls.def.table_id;
                    let sql = rls.def.sql.clone();
                    stdb.create_row_level_security(tx, rls.def).with_context(|| {
                        format!("failed to create row-level security for table `{table_id}`: `{sql}`",)
                    })?;
                }

                stdb.set_initialized(tx, HostType::Wasm, program)?;

                anyhow::Ok(())
            })
            .inspect_err(|e| log::error!("{e:?}"))?;

        let rcr = match self.info.module_def.lifecycle_reducer(Lifecycle::Init) {
            None => {
                stdb.commit_tx(tx)?;
                None
            }

            Some((reducer_id, _)) => {
                self.system_logger().info("Invoking `init` reducer");
                let caller_identity = self.replica_context().database.owner_identity;
                Some(self.call_reducer_with_tx(
                    Some(tx),
                    CallReducerParams {
                        timestamp,
                        caller_identity,
                        caller_address: Address::__DUMMY,
                        client: None,
                        request_id: None,
                        timer: None,
                        reducer_id,
                        args: ArgsTuple::nullary(),
                    },
                ))
            }
        };

        self.system_logger().info("Database initialized");

        Ok(rcr)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let plan = ponder_migrate(&old_module_info.module_def, &self.info.module_def);
        let plan = match plan {
            Ok(plan) => plan,
            Err(errs) => {
                return Ok(UpdateDatabaseResult::AutoMigrateError(errs));
            }
        };
        let stdb = &*self.replica_context().relational_db;

        let program_hash = program.hash;
        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (mut tx, _) = stdb.with_auto_rollback(tx, |tx| stdb.update_program(tx, HostType::Wasm, program))?;
        self.system_logger().info(&format!("Updated program to {program_hash}"));

        let auth_ctx = AuthCtx::for_current(self.replica_context().database.owner_identity);
        let res = crate::db::update::update_database(stdb, &mut tx, auth_ctx, plan, self.system_logger());

        match res {
            Err(e) => {
                log::warn!("Database update failed: {} @ {}", e, stdb.database_identity());
                self.system_logger().warn(&format!("Database update failed: {e}"));
                stdb.rollback_mut_tx(tx);
                Ok(UpdateDatabaseResult::ErrorExecutingMigration(e))
            }
            Ok(()) => {
                stdb.commit_tx(tx)?;
                self.system_logger().info("Database updated");
                log::info!("Database updated, {}", stdb.database_identity());
                Ok(UpdateDatabaseResult::UpdatePerformed)
            }
        }
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        crate::callgrind_flag::invoke_allowing_callgrind(|| self.call_reducer_with_tx(tx, params))
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
    // not straightforward is that the returned [`UpdateStatus`] is constructed
    // from transaction data in the [`UpdateStatus::Committed`] (i.e. success)
    // case.
    //
    /// The method also performs various measurements and records energy usage,
    /// as well as broadcasting a [`ModuleEvent`] containg information about
    /// the outcome of the call.
    #[tracing::instrument(level = "trace", skip_all)]
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

        let replica_ctx = self.replica_context();
        let stdb = &*replica_ctx.relational_db.clone();
        let database_identity = replica_ctx.database_identity;
        let reducer_def = self.info.module_def.reducer_by_id(reducer_id);
        let reducer_name = &*reducer_def.name;

        let _outer_span = tracing::trace_span!("call_reducer",
            reducer_name,
            %caller_identity,
            caller_address = caller_address_opt.map(tracing::field::debug),
        )
        .entered();

        let energy_fingerprint = ReducerFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.owner_identity,
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

        let tx = tx.unwrap_or_else(|| {
            stdb.begin_mut_tx(
                IsolationLevel::Serializable,
                Workload::Reducer(ReducerContext::from(op.clone())),
            )
        });
        let _guard = WORKER_METRICS
            .reducer_plus_query_duration
            .with_label_values(&database_identity, op.name)
            .with_timer(tx.timer);

        let mut tx_slot = self.instance.instance_env().tx.clone();

        let reducer_span = tracing::trace_span!(
            "run_reducer",
            timings.total_duration = tracing::field::Empty,
            energy.budget = budget.get(),
            energy.used = tracing::field::Empty,
        )
        .entered();

        // run the call_reducer call in rayon. it's important that we don't acquire a lock inside a rayon task,
        // as that can lead to deadlock.
        let (mut tx, result) = rayon::scope(|_| tx_slot.set(tx, || self.instance.call_reducer(op, budget)));

        let ExecuteResult {
            energy,
            timings,
            memory_allocation,
            call_result,
        } = result;

        self.energy_monitor
            .record_reducer(&energy_fingerprint, energy.used, timings.total_duration);
        if self.allocated_memory != memory_allocation {
            WORKER_METRICS
                .wasm_memory_bytes
                .with_label_values(&database_identity)
                .set(memory_allocation as i64);
            self.allocated_memory = memory_allocation;
        }

        reducer_span
            .record("timings.total_duration", tracing::field::debug(timings.total_duration))
            .record("energy.used", tracing::field::debug(energy.used));

        const FRAME_LEN_60FPS: Duration = Duration::from_secs(1).checked_div(60).unwrap();
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

                self.replica_context().logger.write(
                    database_logger::LogLevel::Error,
                    &database_logger::Record {
                        ts: chrono::DateTime::from_timestamp_micros(timestamp.microseconds as i64).unwrap(),
                        target: Some(reducer_name),
                        filename: None,
                        line_number: None,
                        message: &errmsg,
                    },
                    &(),
                );
                EventStatus::Failed(errmsg.into())
            }
            // we haven't actually comitted yet - `commit_and_broadcast_event` will commit
            // for us and replace this with the actual database update.
            Ok(Ok(())) => {
                // Detecing a new client, and inserting it in `st_clients`
                // Disconnect logic is written in module_host.rs, due to different transacationality requirements.
                if reducer_def.lifecycle == Some(Lifecycle::OnConnect) {
                    match self.insert_st_client(&mut tx, caller_identity, caller_address) {
                        Ok(_) => EventStatus::Committed(DatabaseUpdate::default()),
                        Err(err) => EventStatus::Failed(err.to_string()),
                    }
                } else {
                    EventStatus::Committed(DatabaseUpdate::default())
                }
            }
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            caller_address: caller_address_opt,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_owned(),
                reducer_id,
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
            .commit_and_broadcast_event(client.as_deref(), event, tx)
            .unwrap()
        {
            Ok(ev) => ev,
            Err(WriteConflict) => todo!("Write skew, you need to implement retries my man, T-dawg."),
        };

        ReducerCallResult {
            outcome: ReducerOutcome::from(&event.status),
            energy_used: energy.used,
            execution_duration: timings.total_duration,
        }
    }

    // Helpers - NOT API
    fn system_logger(&self) -> &SystemLogger {
        self.replica_context().logger.system_logger()
    }

    fn insert_st_client(&self, tx: &mut MutTxId, identity: Identity, address: Address) -> Result<(), DBError> {
        let row = &StClientRow {
            identity: identity.into(),
            address: address.into(),
        };
        tx.insert_via_serialize_bsatn(ST_CLIENT_ID, row).map(|_| ())
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
