use super::instrumentation::CallTimes;
use super::*;
use crate::client::ClientActorId;
use crate::database_logger;
use crate::energy::{EnergyMonitor, FunctionBudget, FunctionFingerprint};
use crate::error::DBError;
use crate::host::host_controller::CallProcedureReturn;
use crate::host::instance_env::{InstanceEnv, TxSlot};
use crate::host::module_common::{build_common_module_from_raw, ModuleCommon};
use crate::host::module_host::{
    call_identity_connected, init_database, CallProcedureParams, CallReducerParams, CallViewParams,
    ClientConnectedError, DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall, ModuleInfo, RefInstance,
    ViewCallResult, ViewCommand, ViewCommandResult, ViewOutcome,
};
use crate::host::scheduler::{CallScheduledFunctionResult, ScheduledFunctionParams};
use crate::host::{
    ArgsTuple, ModuleHost, ProcedureCallError, ProcedureCallResult, ReducerCallError, ReducerCallResult, ReducerId,
    ReducerOutcome, Scheduler, UpdateDatabaseResult,
};
use crate::identity::Identity;
use crate::messages::control_db::HostType;
use crate::module_host_context::ModuleCreationContextLimited;
use crate::replica_context::ReplicaContext;
use crate::sql::ast::SchemaViewer;
use crate::sql::execute::run_with_instance;
use crate::subscription::module_subscription_actor::commit_and_broadcast_event;
use crate::subscription::module_subscription_manager::TransactionOffset;
use crate::util::prometheus_handle::{HistogramExt, TimerGuard};
use crate::worker_metrics::WORKER_METRICS;
use anyhow::{anyhow, bail, ensure, Context};
use bytes::{Buf, Bytes};
use core::future::Future;
use core::time::Duration;
use prometheus::{Histogram, IntCounter, IntGauge};
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_datastore::db_metrics::DB_METRICS;
use spacetimedb_datastore::error::{DatastoreError, ViewError};
use spacetimedb_datastore::execution_context::{self, ReducerContext, Workload};
use spacetimedb_datastore::locking_tx_datastore::{FuncCallType, MutTxId, ViewCallInfo};
use spacetimedb_datastore::traits::{IsolationLevel, Program};
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, ViewResultHeader};
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::{bsatn, ConnectionId, Hash, ProductType, RawModuleDef, Timestamp};
use spacetimedb_primitives::{ProcedureId, TableId, ViewFnPtr, ViewId};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, Deserialize, ProductValue, Typespace, WithTypespace};
use spacetimedb_schema::auto_migrate::{MigratePlan, MigrationPolicy, MigrationPolicyError};
use spacetimedb_schema::def::deserialize::FunctionDef;
use spacetimedb_schema::def::{ModuleDef, ViewDef};
use spacetimedb_subscription::SubscriptionPlan;
use std::sync::Arc;
use tracing::span::EnteredSpan;

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

// TODO: Technically this trait is also used for V8.
// We should rename and move to some place more appropriate.
pub trait WasmInstance {
    fn extract_descriptions(&mut self) -> Result<RawModuleDef, DescribeError>;

    fn replica_ctx(&self) -> &Arc<ReplicaContext>;

    fn tx_slot(&self) -> TxSlot;

    fn call_reducer(&mut self, op: ReducerOp<'_>, budget: FunctionBudget) -> ReducerExecuteResult;

    fn call_view(&mut self, op: ViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult;

    fn call_view_anon(&mut self, op: AnonymousViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult;

    fn log_traceback(&self, func_type: &str, func: &str, trap: &anyhow::Error);

    fn call_procedure(
        &mut self,
        op: ProcedureOp,
        budget: FunctionBudget,
    ) -> impl Future<Output = (ProcedureExecuteResult, Option<TransactionOffset>)>;
}

pub struct EnergyStats {
    pub budget: FunctionBudget,
    pub remaining: FunctionBudget,
}

impl EnergyStats {
    pub const ZERO: Self = Self {
        budget: FunctionBudget::ZERO,
        remaining: FunctionBudget::ZERO,
    };

    /// Returns the used energy amount.
    fn used(&self) -> FunctionBudget {
        (self.budget.get() - self.remaining.get()).into()
    }
}

fn deserialize_view_rows(
    row_type: AlgebraicTypeRef,
    bytes: Bytes,
    typespace: &Typespace,
) -> Result<Vec<ProductValue>, DBError> {
    // The return type is expected to be an array of products.
    let row_type = typespace.resolve(row_type);
    let ret_type = AlgebraicType::array(row_type.ty().clone());
    let seed = WithTypespace::new(typespace, &ret_type);
    let rows = seed
        .deserialize(bsatn::Deserializer::new(&mut &bytes[..]))
        .map_err(|e| DatastoreError::from(ViewError::DeserializeReturn(e.to_string())))
        .map_err(DBError::from)?;

    rows.into_array()
        .map_err(|_| ViewError::SerializeRow)
        .map_err(DatastoreError::from)
        .map_err(DBError::from)?
        .into_iter()
        .map(|product| {
            product
                .into_product()
                .map_err(|_| ViewError::SerializeRow)
                .map_err(DatastoreError::from)
                .map_err(DBError::from)
        })
        .collect()
}

pub struct ExecutionTimings {
    pub total_duration: Duration,
    pub wasm_instance_env_call_times: CallTimes,
}

impl ExecutionTimings {
    /// Not a `const` because there doesn't seem to be any way to `const` construct an `enum_map::EnumMap`,
    /// which `CallTimes` uses.
    pub fn zero() -> Self {
        Self {
            total_duration: Duration::ZERO,
            wasm_instance_env_call_times: CallTimes::new(),
        }
    }
}

/// The result that `__call_reducer__` produces during normal non-trap execution.
pub type ReducerResult = Result<(), Box<str>>;

pub struct ExecutionStats {
    pub energy: EnergyStats,
    pub timings: ExecutionTimings,
    pub memory_allocation: usize,
}

impl ExecutionStats {
    fn energy_used(&self) -> FunctionBudget {
        self.energy.used()
    }

    fn abi_duration(&self) -> Duration {
        self.timings.wasm_instance_env_call_times.sum()
    }

    fn total_duration(&self) -> Duration {
        self.timings.total_duration
    }
}

pub enum ExecutionError {
    User(Box<str>),
    Recoverable(anyhow::Error),
    Trap(anyhow::Error),
}

#[derive(derive_more::AsRef)]
pub struct ExecutionResult<T, E> {
    #[as_ref]
    pub stats: ExecutionStats,
    pub call_result: Result<T, E>,
}

pub type ReducerExecuteResult = ExecutionResult<(), ExecutionError>;

impl<T, E> ExecutionResult<T, E> {
    pub fn map_result<X, Y>(self, f: impl FnOnce(Result<T, E>) -> Result<X, Y>) -> ExecutionResult<X, Y> {
        let Self { stats, call_result } = self;
        let call_result = f(call_result);
        ExecutionResult { stats, call_result }
    }
}

// The original version of views used a different return format (it returned the rows directly).
// The newer version uses ViewReturnData to represent the different formats.
pub enum ViewReturnData {
    // This view returns a Vec of rows (bsatn encoded).
    Rows(Bytes),
    // This view returns a ViewResultHeader, potentially followed by more data.
    HeaderFirst(Bytes),
}

// A view result after processing the return header.
pub enum ViewResult {
    // The rows are encoded as a bsatn array of products.
    Rows(Bytes),
    RawSql(String),
}

impl ViewResult {
    pub fn from_return_data(data: ViewReturnData) -> Result<Self, anyhow::Error> {
        match data {
            ViewReturnData::Rows(bytes) => Ok(ViewResult::Rows(bytes)),
            ViewReturnData::HeaderFirst(bytes) => {
                let mut reader = &bytes[..];
                let header = {
                    let deserializer = bsatn::Deserializer::new(&mut reader);
                    ViewResultHeader::deserialize(deserializer)
                        .context("failed to deserialize ViewResultHeader from view return data")?
                };
                match header {
                    ViewResultHeader::RawSql(query) => Ok(ViewResult::RawSql(query)),
                    ViewResultHeader::RowData => {
                        let at = bytes.len() - reader.remaining();
                        let remaining_bytes = bytes.slice(at..);
                        Ok(ViewResult::Rows(remaining_bytes))
                    }
                }
            }
        }
    }
}

pub type ViewExecuteResult = ExecutionResult<ViewReturnData, ExecutionError>;

pub type ProcedureExecuteResult = ExecutionResult<Bytes, anyhow::Error>;

pub struct WasmModuleHostActor<T: WasmModule> {
    module: T::InstancePre,
    common: ModuleCommon,
    func_names: Arc<FuncNames>,
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
    #[error("bad signature for descriptor function: {0}")]
    Signature(anyhow::Error),
    #[error("error when preparing descriptor function: {0}")]
    Setup(anyhow::Error),
    #[error("error decoding module description: {0}")]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    RuntimeError(anyhow::Error),
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn new(
        mcc: ModuleCreationContextLimited,
        module: T,
    ) -> Result<(Self, WasmModuleInstance<T::Instance>), InitializationError> {
        log::trace!(
            "Making new WASM module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program_hash,
        );

        let func_names = {
            FuncNames::check_required(|name| module.get_export(name))?;
            let mut func_names = FuncNames::default();
            module.for_each_export(|sym, ty| func_names.update_from_general(sym, ty))?;
            func_names.preinits.sort_unstable();
            func_names
        };
        let uninit_instance = module.instantiate_pre()?;
        let instance_env = InstanceEnv::new(mcc.replica_ctx.clone(), mcc.scheduler.clone());
        let mut instance = uninit_instance.instantiate(instance_env, &func_names)?;

        let desc = instance.extract_descriptions()?;

        // Validate and create a common module rom the raw definition.
        let common = build_common_module_from_raw(mcc, desc)?;

        let func_names = Arc::new(func_names);
        let module = WasmModuleHostActor {
            module: uninit_instance,
            func_names,
            common,
        };
        let initial_instance = module.make_from_instance(instance);

        Ok((module, initial_instance))
    }
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    fn make_from_instance(&self, instance: T::Instance) -> WasmModuleInstance<T::Instance> {
        let common = InstanceCommon::new(&self.common);
        WasmModuleInstance {
            instance,
            common,
            trapped: false,
        }
    }
}

impl<T: WasmModule> WasmModuleHostActor<T> {
    pub fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        self.common.replica_ctx()
    }

    pub fn scheduler(&self) -> &Scheduler {
        self.common.scheduler()
    }

    pub fn info(&self) -> Arc<ModuleInfo> {
        self.common.info()
    }

    pub fn create_instance(&self) -> WasmModuleInstance<T::Instance> {
        let common = &self.common;
        let env = InstanceEnv::new(common.replica_ctx().clone(), common.scheduler().clone());
        // this shouldn't fail, since we already called module.create_instance()
        // before and it didn't error, and ideally they should be deterministic
        let mut instance = self
            .module
            .instantiate(env, &self.func_names)
            .expect("failed to initialize instance");
        let _ = instance.extract_descriptions();
        self.make_from_instance(instance)
    }
}

pub struct WasmModuleInstance<T: WasmInstance> {
    instance: T,
    common: InstanceCommon,
    trapped: bool,
}

impl<T: WasmInstance> std::fmt::Debug for WasmModuleInstance<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmInstanceActor").finish()
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
    pub fn trapped(&self) -> bool {
        self.trapped
    }

    pub fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.common
            .update_database(program, old_module_info, policy, &mut self.instance)
    }

    pub fn call_reducer(&mut self, params: CallReducerParams) -> ReducerCallResult {
        let (res, trapped) = self.call_reducer_with_tx(None, params);
        self.trapped = trapped;
        res
    }

    pub fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.common.clear_all_clients()
    }

    pub fn call_identity_connected(
        &mut self,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        let module = &self.common.info.clone();
        let call_reducer = |tx, params| self.call_reducer_with_tx(tx, params);
        let mut trapped = false;
        let res = call_identity_connected(caller_auth, caller_connection_id, module, call_reducer, &mut trapped);
        self.trapped = trapped;
        res
    }

    pub fn call_identity_disconnected(
        &mut self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        drop_view_subscribers: bool,
    ) -> Result<(), ReducerCallError> {
        let module = &self.common.info.clone();
        let call_reducer = |tx, params| self.call_reducer_with_tx(tx, params);
        let mut trapped = false;
        let res = ModuleHost::call_identity_disconnected_inner(
            caller_identity,
            caller_connection_id,
            module,
            drop_view_subscribers,
            call_reducer,
            &mut trapped,
        );
        self.trapped = trapped;
        res
    }

    pub fn disconnect_client(&mut self, client_id: ClientActorId) -> Result<(), ReducerCallError> {
        let module = &self.common.info.clone();
        let call_reducer = |tx, params| self.call_reducer_with_tx(tx, params);
        let mut trapped = false;
        let res = ModuleHost::disconnect_client_inner(client_id, module, call_reducer, &mut trapped);
        self.trapped = trapped;
        res
    }

    pub fn init_database(&mut self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        let module_def = &self.common.info.clone().module_def;
        let replica_ctx = &self.instance.replica_ctx().clone();
        let call_reducer = |tx, params| self.call_reducer_with_tx(tx, params);
        let (res, trapped) = init_database(replica_ctx, module_def, program, call_reducer);
        self.trapped = trapped;
        res
    }

    pub async fn call_procedure(&mut self, params: CallProcedureParams) -> CallProcedureReturn {
        let (res, trapped) = self.common.call_procedure(params, &mut self.instance).await;
        self.trapped = trapped;
        res
    }

    pub(in crate::host) async fn call_scheduled_function(
        &mut self,
        params: ScheduledFunctionParams,
    ) -> CallScheduledFunctionResult {
        let (res, trapped) = self.common.call_scheduled_function(params, &mut self.instance).await;
        self.trapped = trapped;
        res
    }
}

impl<T: WasmInstance> WasmModuleInstance<T> {
    #[tracing::instrument(level = "trace", skip_all)]
    fn call_reducer_with_tx(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> (ReducerCallResult, bool) {
        crate::callgrind_flag::invoke_allowing_callgrind(|| {
            self.common.call_reducer_with_tx(tx, params, &mut self.instance)
        })
    }

    pub fn call_view(&mut self, cmd: ViewCommand) -> ViewCommandResult {
        let (res, trapped) = self.common.handle_cmd(cmd, &mut self.instance);
        self.trapped = trapped;
        res
    }
}

pub struct InstanceCommon {
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
    allocated_memory: usize,
    metric_wasm_memory_bytes: IntGauge,
    vm_metrics: AllVmMetrics,
}

impl InstanceCommon {
    pub(crate) fn new(module: &ModuleCommon) -> Self {
        let info = module.info();
        let vm_metrics = AllVmMetrics::new(&info);

        Self {
            info: module.info(),
            vm_metrics,
            energy_monitor: module.energy_monitor(),
            // Will be updated on the first reducer call.
            allocated_memory: 0,
            metric_wasm_memory_bytes: WORKER_METRICS
                .wasm_memory_bytes
                .with_label_values(module.database_identity()),
        }
    }

    pub(crate) fn info(&self) -> Arc<ModuleInfo> {
        self.info.clone()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn update_database<I: WasmInstance>(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
        inst: &mut I,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        let replica_ctx = inst.replica_ctx().clone();
        let system_logger = replica_ctx.logger.system_logger();
        let stdb = &replica_ctx.relational_db;

        let plan: MigratePlan = match policy.try_migrate(
            self.info.database_identity,
            old_module_info.module_hash,
            &old_module_info.module_def,
            self.info.module_hash,
            &self.info.module_def,
        ) {
            Ok(plan) => plan,
            Err(e) => {
                return match e {
                    MigrationPolicyError::AutoMigrateFailure(e) => Ok(UpdateDatabaseResult::AutoMigrateError(e.into())),
                    _ => Ok(UpdateDatabaseResult::ErrorExecutingMigration(e.into())),
                }
            }
        };

        let program_hash = program.hash;
        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (mut tx, _) = stdb.with_auto_rollback(tx, |tx| stdb.update_program(tx, HostType::Wasm, program))?;
        system_logger.info(&format!("Updated program to {program_hash}"));

        let auth_ctx = AuthCtx::for_current(replica_ctx.database.owner_identity);
        let res = crate::db::update::update_database(stdb, &mut tx, auth_ctx, plan, system_logger);
        let mut energy_quanta_used = FunctionBudget::ZERO;
        let mut host_execution_duration = Duration::ZERO;

        match res {
            Err(e) => {
                log::warn!("Database update failed: {} @ {}", e, stdb.database_identity());
                system_logger.warn(&format!("Database update failed: {e}"));
                let (_, tx_metrics, reducer) = stdb.rollback_mut_tx(tx);
                stdb.report_mut_tx_metrics(reducer, tx_metrics, None);
                Ok(UpdateDatabaseResult::ErrorExecutingMigration(e))
            }
            Ok(res) => {
                system_logger.info("Database updated");
                log::info!("Database updated, {}", stdb.database_identity());
                let res: UpdateDatabaseResult = match res {
                    crate::db::update::UpdateResult::Success => UpdateDatabaseResult::UpdatePerformed,
                    crate::db::update::UpdateResult::EvaluateSubscribedViews => {
                        let (out, trapped) = self.evaluate_subscribed_views(tx, inst)?;
                        tx = out.tx;
                        energy_quanta_used = out.energy_used;
                        host_execution_duration = out.total_duration;

                        if trapped || out.outcome != ViewOutcome::Success {
                            let msg = match trapped {
                                true => "Trapped while evaluating views during database update".to_string(),
                                false => format!(
                                    "Views evaluation did not complete successfully during database update: {:?}",
                                    out.outcome
                                ),
                            };

                            UpdateDatabaseResult::ErrorExecutingMigration(anyhow::anyhow!(msg))
                        } else {
                            UpdateDatabaseResult::UpdatePerformed
                        }
                    }
                    crate::db::update::UpdateResult::RequiresClientDisconnect => {
                        UpdateDatabaseResult::UpdatePerformedWithClientDisconnect
                    }
                };

                if res.was_successful() {
                    let event = ModuleEvent {
                        timestamp: Timestamp::now(),
                        caller_identity: self.info.owner_identity,
                        caller_connection_id: None,
                        function_call: ModuleFunctionCall::update(),
                        status: EventStatus::Committed(DatabaseUpdate::default()),
                        energy_quanta_used: energy_quanta_used.into(),
                        host_execution_duration,
                        request_id: None,
                        timer: None,
                    };
                    //TODO: Return back event in `UpdateDatabaseResult`?
                    let _ = commit_and_broadcast_event(&self.info.subscriptions, None, event, tx);
                } else {
                    let (_, tx_metrics, reducer) = stdb.rollback_mut_tx(tx);
                    stdb.report_mut_tx_metrics(reducer, tx_metrics, None);
                }
                Ok(res)
            }
        }
    }

    /// Re-evaluates all views which have entries in `st_view_subs`.
    fn evaluate_subscribed_views<I: WasmInstance>(
        &mut self,
        tx: MutTxId,
        inst: &mut I,
    ) -> Result<(ViewCallResult, bool), anyhow::Error> {
        let views = self.info.module_def.views().collect::<Vec<_>>();
        let owner_identity = self.info.owner_identity;

        let mut view_calls = Vec::new();

        for view in views {
            let ViewDef {
                name: view_name,
                is_anonymous,
                fn_ptr,
                product_type_ref,
                ..
            } = view;

            let st_view = tx
                .view_from_name(view_name)?
                .ok_or_else(|| anyhow::anyhow!("view {} not found in database", &view_name))?;

            let view_id = st_view.view_id;
            let table_id = st_view
                .table_id
                .ok_or_else(|| anyhow::anyhow!("view {} does not have a backing table in database", &view_name))?;

            for sub in tx.lookup_st_view_subs(view_id)? {
                view_calls.push(CallViewParams {
                    view_name: view_name.to_owned().into(),
                    view_id,
                    table_id,
                    fn_ptr: *fn_ptr,
                    caller: owner_identity,
                    sender: if *is_anonymous { None } else { Some(sub.identity.into()) },
                    args: ArgsTuple::nullary(),
                    row_type: *product_type_ref,
                    timestamp: Timestamp::now(),
                });
            }
        }

        Ok(self.execute_view_calls(tx, view_calls, inst))
    }

    pub(crate) async fn call_procedure<I: WasmInstance>(
        &mut self,
        params: CallProcedureParams,
        inst: &mut I,
    ) -> (CallProcedureReturn, bool) {
        let CallProcedureParams {
            timestamp,
            caller_identity,
            caller_connection_id,
            timer,
            procedure_id,
            args,
        } = params;

        // We've already validated by this point that the procedure exists,
        // so it's fine to use the panicking `procedure_by_id`.
        let procedure_def = self.info.module_def.procedure_by_id(procedure_id);
        let procedure_name: &str = &procedure_def.name;

        // TODO(observability): Add tracing spans, energy, metrics?
        // These will require further thinking once we implement procedure suspend/resume,
        // and so are not worth doing yet.

        let op = ProcedureOp {
            id: procedure_id,
            name: procedure_name.into(),
            caller_identity,
            caller_connection_id,
            timestamp,
            arg_bytes: args.get_bsatn().clone(),
        };
        let energy_fingerprint = FunctionFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.owner_identity,
            caller_identity,
            function_name: &procedure_def.name,
        };

        // TODO(procedure-energy): replace with call to separate function `procedure_budget`.
        let budget = self.energy_monitor.reducer_budget(&energy_fingerprint);

        let (result, tx_offset) = inst.call_procedure(op, budget).await;

        let ProcedureExecuteResult {
            stats:
                ExecutionStats {
                    memory_allocation,
                    // TODO(procedure-energy): Do something with timing and energy.
                    ..
                },
            call_result,
        } = result;

        // TODO(shub): deduplicate with reducer and view logic.
        if self.allocated_memory != memory_allocation {
            self.metric_wasm_memory_bytes.set(memory_allocation as i64);
            self.allocated_memory = memory_allocation;
        }

        let trapped = call_result.is_err();

        let result = match call_result {
            Err(err) => {
                inst.log_traceback("procedure", &procedure_def.name, &err);

                WORKER_METRICS
                    .wasm_instance_errors
                    .with_label_values(&self.info.database_identity, &self.info.module_hash, procedure_name)
                    .inc();

                // TODO(procedure-energy):
                // if energy.remaining.get() == 0 {
                //     return Err(ProcedureCallError::OutOfEnergy);
                // } else
                {
                    Err(ProcedureCallError::InternalError(format!("{err}")))
                }
            }
            Ok(return_val) => {
                let return_type = &procedure_def.return_type;
                let seed = spacetimedb_sats::WithTypespace::new(self.info.module_def.typespace(), return_type);
                seed.deserialize(bsatn::Deserializer::new(&mut &return_val[..]))
                    .map_err(|err| ProcedureCallError::InternalError(format!("{err}")))
                    .map(|return_val| ProcedureCallResult {
                        return_val,
                        execution_duration: timer.map(|timer| timer.elapsed()).unwrap_or_default(),
                        start_timestamp: timestamp,
                    })
            }
        };

        (CallProcedureReturn { result, tx_offset }, trapped)
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
    // not straightforward is that the returned [`UpdateStatus`] is constructed
    // from transaction data in the [`UpdateStatus::Committed`] (i.e. success)
    // case.
    //
    /// The method also performs various measurements and records energy usage,
    /// as well as broadcasting a [`ModuleEvent`] containing information about
    /// the outcome of the call.
    ///
    /// The `bool` in the return type signifies whether there was an "outer error".
    /// For WASM, this should be interpreted as a trap occurring.
    pub(crate) fn call_reducer_with_tx<I: WasmInstance>(
        &mut self,
        tx: Option<MutTxId>,
        params: CallReducerParams,
        inst: &mut I,
    ) -> (ReducerCallResult, bool) {
        let CallReducerParams {
            timestamp,
            caller_identity,
            caller_connection_id,
            client,
            request_id,
            reducer_id,
            args,
            timer,
        } = params;
        let caller_connection_id_opt = (caller_connection_id != ConnectionId::ZERO).then_some(caller_connection_id);

        let replica_ctx = inst.replica_ctx();
        let stdb = &*replica_ctx.relational_db.clone();
        let info = self.info.clone();
        let reducer_def = info.module_def.reducer_by_id(reducer_id);
        let reducer_name = &*reducer_def.name;

        // Do some `with_label_values`.
        // TODO(perf, centril): consider caching this.
        let _outer_span = start_call_function_span(reducer_name, &caller_identity, caller_connection_id_opt);

        let op = ReducerOp {
            id: reducer_id,
            name: reducer_name,
            caller_identity: &caller_identity,
            caller_connection_id: &caller_connection_id,
            timestamp,
            args: &args,
        };

        let workload = Workload::Reducer(ReducerContext::from(op.clone()));
        let tx = tx.unwrap_or_else(|| stdb.begin_mut_tx(IsolationLevel::Serializable, workload));
        let mut tx_slot = inst.tx_slot();

        let vm_metrics = self.vm_metrics.get_for_reducer_id(reducer_id);
        let _guard = vm_metrics.timer_guard_for_reducer_plus_query(tx.timer);

        let (mut tx, result) = tx_slot.set(tx, || {
            self.call_function(caller_identity, reducer_name, |budget| inst.call_reducer(op, budget))
        });

        // Report execution metrics on each reducer call.
        vm_metrics.report(&result.stats);

        // An outer error occurred.
        // This signifies a logic error in the module rather than a properly
        // handled bad argument from the caller of a reducer.
        // For WASM, this will be interpreted as a trap
        // and that the instance must be discarded.
        // However, that does not necessarily apply to e.g., V8.
        let trapped = matches!(result.call_result, Err(ExecutionError::Trap(_)));

        let status = match result.call_result {
            Err(ExecutionError::Recoverable(err) | ExecutionError::Trap(err)) => {
                inst.log_traceback("reducer", reducer_name, &err);

                self.handle_outer_error(&result.stats.energy, reducer_name)
            }
            Err(ExecutionError::User(err)) => {
                log_reducer_error(
                    inst.replica_ctx(),
                    timestamp,
                    reducer_name,
                    &err,
                    &self.info.module_hash,
                );
                EventStatus::Failed(err.into())
            }
            // We haven't actually committed yet - `commit_and_broadcast_event` will commit
            // for us and replace this with the actual database update.
            Ok(()) => {
                // If this is an OnDisconnect lifecycle event, remove the client from st_clients.
                // We handle OnConnect events before running the reducer.
                let res = match reducer_def.lifecycle {
                    Some(Lifecycle::OnDisconnect) => {
                        tx.delete_st_client(caller_identity, caller_connection_id, info.database_identity)
                    }
                    _ => Ok(()),
                };
                match res {
                    Ok(()) => EventStatus::Committed(DatabaseUpdate::default()),
                    Err(err) => {
                        let err = err.to_string();
                        log_reducer_error(
                            inst.replica_ctx(),
                            timestamp,
                            reducer_name,
                            &err,
                            &self.info.module_hash,
                        );
                        EventStatus::Failed(err)
                    }
                }
            }
        };

        // Only re-evaluate and update views if the reducer's execution was successful
        let (out, trapped) = if !trapped && matches!(status, EventStatus::Committed(_)) {
            self.call_views_with_tx(tx, caller_identity, &info.module_def, inst, timestamp)
        } else {
            (ViewCallResult::default(tx), trapped)
        };

        // Account for view execution in reducer reporting metrics
        vm_metrics.report_energy_used(out.energy_used);
        vm_metrics.report_total_duration(out.total_duration);
        vm_metrics.report_abi_duration(out.abi_duration);

        let status = match out.outcome {
            ViewOutcome::BudgetExceeded => EventStatus::OutOfEnergy,
            ViewOutcome::Failed(err) => EventStatus::Failed(err),
            ViewOutcome::Success => status,
        };

        let energy_quanta_used = result.stats.energy_used().into();
        let total_duration = result.stats.total_duration();

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            caller_connection_id: caller_connection_id_opt,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                reducer_id,
                args,
            },
            status,
            energy_quanta_used,
            host_execution_duration: total_duration,
            request_id,
            timer,
        };
        let event = commit_and_broadcast_event(&info.subscriptions, client, event, out.tx).event;

        let res = ReducerCallResult {
            outcome: ReducerOutcome::from(&event.status),
            energy_used: energy_quanta_used,
            execution_duration: total_duration,
        };

        (res, trapped)
    }

    fn handle_outer_error(&mut self, energy: &EnergyStats, reducer_name: &str) -> EventStatus {
        WORKER_METRICS
            .wasm_instance_errors
            .with_label_values(&self.info.database_identity, &self.info.module_hash, reducer_name)
            .inc();

        if energy.remaining.get() == 0 {
            EventStatus::OutOfEnergy
        } else {
            EventStatus::Failed("The instance encountered a fatal error.".into())
        }
    }

    /// Calls a function (reducer, view) and performs energy monitoring.
    fn call_function<F, R: AsRef<ExecutionStats>>(
        &mut self,
        caller_identity: Identity,
        function_name: &str,
        vm_call_function: F,
    ) -> R
    where
        F: FnOnce(FunctionBudget) -> R,
    {
        let energy_fingerprint = FunctionFingerprint {
            module_hash: self.info.module_hash,
            module_identity: self.info.owner_identity,
            caller_identity,
            function_name,
        };
        let budget = self.energy_monitor.reducer_budget(&energy_fingerprint);

        let function_span = start_run_function_span(budget);

        let result = vm_call_function(budget);

        let stats: &ExecutionStats = result.as_ref();
        let energy_used = stats.energy.used();
        let energy_quanta_used = energy_used.into();
        let timings = &stats.timings;
        let memory_allocation = stats.memory_allocation;

        self.energy_monitor
            .record_reducer(&energy_fingerprint, energy_quanta_used, timings.total_duration);
        if self.allocated_memory != memory_allocation {
            self.metric_wasm_memory_bytes.set(memory_allocation as i64);
            self.allocated_memory = memory_allocation;
        }

        maybe_log_long_running_function(function_name, timings.total_duration);

        function_span
            .record("timings.total_duration", tracing::field::debug(timings.total_duration))
            .record("energy.used", tracing::field::debug(energy_used));

        result
    }

    pub(crate) fn handle_cmd<I: WasmInstance>(&mut self, cmds: ViewCommand, inst: &mut I) -> (ViewCommandResult, bool) {
        let info = self.info.clone();
        let mut inst = RefInstance {
            instance: inst,
            common: self,
        };
        match cmds {
            ViewCommand::AddSingleSubscription {
                sender,
                auth,
                request,
                timer,
            } => {
                let res = info
                    .subscriptions
                    .add_single_subscription_with_instance(&mut inst, sender, auth, request, timer, None);

                match res {
                    Ok((metrics, trapped)) => (ViewCommandResult::Subscription { result: Ok(metrics) }, trapped),
                    Err(err) => (ViewCommandResult::Subscription { result: Err(err) }, false),
                }
            }
            ViewCommand::AddLegacySubscription {
                sender,
                auth,
                subscribe,
                timer,
            } => {
                let res = info
                    .subscriptions
                    .add_legacy_subscriber_with_instance(&mut inst, sender, auth, subscribe, timer, None);

                match res {
                    Ok((metrics, trapped)) => (
                        ViewCommandResult::Subscription {
                            result: Ok(Some(metrics)),
                        },
                        trapped,
                    ),
                    Err(err) => (ViewCommandResult::Subscription { result: Err(err) }, false),
                }
            }
            ViewCommand::AddMultiSubscription {
                sender,
                auth,
                request,
                timer,
            } => {
                let res = info
                    .subscriptions
                    .add_multi_subscription_with_instance(&mut inst, sender, auth, request, timer, None);

                match res {
                    Ok((metrics, trapped)) => (ViewCommandResult::Subscription { result: Ok(metrics) }, trapped),
                    Err(err) => (ViewCommandResult::Subscription { result: Err(err) }, false),
                }
            }

            ViewCommand::Sql {
                db,
                sql_text,
                auth,
                subs,
            } => {
                let mut head = vec![];
                let res = run_with_instance(&mut inst, db, sql_text, auth, subs, &mut head);

                match res {
                    Ok((result, trapped)) => (
                        ViewCommandResult::Sql {
                            result: Ok(result),
                            head,
                        },
                        trapped,
                    ),
                    Err(err) => (ViewCommandResult::Sql { result: Err(err), head }, false),
                }
            }
        }
    }

    /// Executes a view and materializes its result,
    /// deleting any previously materialized rows.
    ///
    /// Similar to [`Self::call_reducer_with_tx`], but for views.
    /// However, unlike [`Self::call_reducer_with_tx`],
    /// it mutates a previously allocated [`MutTxId`] and returns it.
    /// It does not commit the transaction.
    pub(crate) fn call_view_with_tx<I: WasmInstance>(
        &mut self,
        tx: MutTxId,
        params: CallViewParams,
        inst: &mut I,
    ) -> (ViewCallResult, bool) {
        let CallViewParams {
            view_name,
            view_id,
            table_id,
            fn_ptr,
            caller,
            sender,
            args,
            row_type,
            timestamp,
        } = params;

        let _outer_span = start_call_function_span(&view_name, &caller, None);

        let mut tx_slot = inst.tx_slot();
        let (mut tx, result) = tx_slot.set(tx, || {
            self.call_function(caller, &view_name, |budget| match sender {
                Some(sender) => inst.call_view(
                    ViewOp {
                        name: &view_name,
                        view_id,
                        table_id,
                        fn_ptr,
                        sender: &sender,
                        args: &args,
                        timestamp,
                    },
                    budget,
                ),
                None => inst.call_view_anon(
                    AnonymousViewOp {
                        name: &view_name,
                        view_id,
                        table_id,
                        fn_ptr,
                        args: &args,
                        timestamp,
                    },
                    budget,
                ),
            })
        });

        // Report execution metrics on each view call.
        self.vm_metrics.get_for_view_id(view_id).report(&result.stats);

        let trapped = matches!(result.call_result, Err(ExecutionError::Trap(_)));

        let outcome: ViewOutcome = match (result.call_result, sender) {
            (Err(ExecutionError::Recoverable(err) | ExecutionError::Trap(err)), _) => {
                inst.log_traceback("view", &view_name, &err);
                self.handle_outer_error(&result.stats.energy, &view_name).into()
            }
            // TODO: maybe do something else with user errors?
            (Err(ExecutionError::User(err)), _) => {
                inst.log_traceback("view", &view_name, &anyhow::anyhow!(err));
                self.handle_outer_error(&result.stats.energy, &view_name).into()
            }
            (Ok(raw), sender) => {
                // This is wrapped in a closure to simplify error handling.
                let outcome: Result<ViewOutcome, anyhow::Error> = (|| {
                    let result = ViewResult::from_return_data(raw).context("Error parsing view result")?;
                    let typespace = self.info.module_def.typespace();
                    let row_product_type = typespace
                        .resolve(row_type)
                        .resolve_refs()?
                        .into_product()
                        .map_err(|_| anyhow!("Error resolving row type for view"))?;

                    let rows = match result {
                        ViewResult::Rows(bytes) => deserialize_view_rows(row_type, bytes, typespace)
                            .context("Error deserializing rows returned by view".to_string())?,
                        ViewResult::RawSql(query) => self
                            .run_query_for_view(
                                &mut tx,
                                &query,
                                &row_product_type,
                                &ViewCallInfo {
                                    view_id,
                                    table_id,
                                    fn_ptr,
                                    sender,
                                },
                            )
                            .context("Error executing raw SQL returned by view".to_string())?,
                    };

                    let replica_ctx = inst.replica_ctx();
                    let stdb = &*replica_ctx.relational_db.clone();
                    let res = match sender {
                        Some(sender) => stdb.materialize_view(&mut tx, table_id, sender, rows),
                        None => stdb.materialize_anonymous_view(&mut tx, table_id, rows),
                    };

                    res.context("Error materializing view")?;

                    Ok(ViewOutcome::Success)
                })();
                match outcome {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        log::error!("Error materializing view `{view_name}`: {err:?}");
                        ViewOutcome::Failed(format!("Error materializing view `{view_name}`: {err}"))
                    }
                }
            }
        };

        let res = ViewCallResult {
            outcome,
            tx,
            energy_used: result.stats.energy_used(),
            total_duration: result.stats.total_duration(),
            abi_duration: result.stats.abi_duration(),
        };

        (res, trapped)
    }

    /// Compiles and runs a query that was returned from a view.
    /// This tracks read dependencies for the view.
    /// Note that this doesn't modify the resulting rows in any way.
    fn run_query_for_view(
        &self,
        tx: &mut MutTxId,
        the_query: &str,
        expected_row_type: &ProductType,
        call_info: &ViewCallInfo,
    ) -> anyhow::Result<Vec<ProductValue>> {
        if the_query.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Views bypass RLS, since views should enforce their own access control procedurally.
        let auth = AuthCtx::for_current(self.info.database_identity);
        let schema_view = SchemaViewer::new(&*tx, &auth);

        // Compile to subscription plans.
        let (plans, has_params) = SubscriptionPlan::compile(the_query, &schema_view, &auth)?;
        ensure!(
            !has_params,
            "parameterized SQL is not supported for view materialization yet"
        );

        // Validate shape and disallow views-on-views.
        for plan in &plans {
            let phys = plan.optimized_physical_plan();
            let Some(source_schema) = phys.return_table() else {
                bail!("query does not return plain table rows");
            };
            if phys.reads_from_view(true) || phys.reads_from_view(false) {
                bail!("view definition cannot read from other views");
            }
            if source_schema.row_type != *expected_row_type {
                bail!(
                    "query returns `{}` but view expects `{}`",
                    fmt_algebraic_type(&AlgebraicType::Product(source_schema.row_type.clone())),
                    fmt_algebraic_type(&AlgebraicType::Product(expected_row_type.clone())),
                );
            }
        }

        let op = FuncCallType::View(call_info.clone());
        let mut metrics = ExecutionMetrics::default();
        let mut rows = Vec::new();

        for plan in plans {
            // Track read sets for all tables involved in this plan.
            // TODO(jsdt): This means we will rerun the view and query for any change to these tables, so we should optimize this asap.
            for table_id in plan.table_ids() {
                tx.record_table_scan(&op, table_id);
            }

            let pipelined = PipelinedProject::from(plan.optimized_physical_plan().clone());
            pipelined.execute(&*tx, &mut metrics, &mut |row| {
                rows.push(row.to_product_value());
                Ok(())
            })?;
        }

        Ok(rows)
    }
    /// A [`MutTxId`] knows which views must be updated (re-evaluated).
    /// This method re-evaluates them and updates their backing tables.
    pub(crate) fn call_views_with_tx<I: WasmInstance>(
        &mut self,
        tx: MutTxId,
        caller: Identity,
        module_def: &ModuleDef,
        inst: &mut I,
        timestamp: Timestamp,
    ) -> (ViewCallResult, bool) {
        let view_calls = tx
            .view_for_update()
            .cloned()
            .map(|info| {
                let view_def = module_def
                    .get_view_by_id(info.fn_ptr, info.sender.is_none())
                    .unwrap_or_else(|| panic!("view with fn_ptr `{}` not found", info.fn_ptr));

                CallViewParams {
                    view_name: view_def.name.clone().into(),
                    view_id: info.view_id,
                    table_id: info.table_id,
                    fn_ptr: view_def.fn_ptr,
                    caller,
                    sender: info.sender,
                    args: ArgsTuple::nullary(),
                    row_type: view_def.product_type_ref,
                    timestamp,
                }
            })
            .collect::<Vec<_>>();

        self.execute_view_calls(tx, view_calls, inst)
    }

    /// Executes view calls and accumulate results.
    /// Returns early if any call traps or fails.
    fn execute_view_calls<I: WasmInstance>(
        &mut self,
        tx: MutTxId,
        view_calls: Vec<CallViewParams>,
        inst: &mut I,
    ) -> (ViewCallResult, bool) {
        let mut out = ViewCallResult::default(tx);
        let mut trapped = false;

        for params in view_calls {
            let (result, call_trapped) = self.call_view_with_tx(out.tx, params, inst);

            out.tx = result.tx;
            out.outcome = result.outcome;
            out.energy_used += result.energy_used;
            out.total_duration += result.total_duration;
            out.abi_duration += result.abi_duration;

            trapped = trapped || call_trapped;

            // Terminate early if execution failed
            if trapped || !matches!(out.outcome, ViewOutcome::Success) {
                break;
            }
        }

        (out, trapped)
    }

    /// Empty the system tables tracking clients without running any lifecycle reducers.
    pub(crate) fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.info.relational_db().clear_all_clients().map_err(Into::into)
    }

    pub(crate) async fn call_scheduled_function<I: WasmInstance>(
        &mut self,
        params: ScheduledFunctionParams,
        inst: &mut I,
    ) -> (CallScheduledFunctionResult, bool) {
        crate::host::scheduler::call_scheduled_function(&self.info.clone(), params, self, inst).await
    }
}

/// Pre-fetched VM metrics counters for all reducers and views in a module.
struct AllVmMetrics {
    // We use a `Vec` here as the number of reducers + views
    // will likely be lower than e.g., 128, which would take up a page (4096 / 32).
    // TODO(perf, centril): Define a `VecMapWithFallback<N>`
    // that falls back to `HashMap` when exceeding `N` entries.
    // This could be useful elsewhere for e.g., TableId => X maps and similar.
    counters: Vec<VmMetrics>,
    num_reducers: u32,
}

impl AllVmMetrics {
    /// Pre-fetch all vm metrics counters for the module in `info`.
    fn new(info: &ModuleInfo) -> Self {
        // These are the reducers:
        let def = &info.module_def;
        let reducers = def.reducer_ids_and_defs();
        let num_reducers = reducers.len() as u32;
        let reducers = reducers.map(|(_, def)| def.name());

        // These are the views:
        let views = def.views().map(|def| def.name());

        // Pre-fetch the metrics for both:
        let counters = reducers
            .chain(views)
            .map(|name| VmMetrics::new(&info.database_identity, name))
            .collect();

        Self { counters, num_reducers }
    }

    fn get_for_index(&self, index: u32) -> VmMetrics {
        self.counters[index as usize].clone()
    }

    /// Returns the vm metrics counters for `id`,
    /// or panics if `id` was not pre-fetched in [`AllVmMetrics::new`].
    fn get_for_reducer_id(&self, id: ReducerId) -> VmMetrics {
        self.get_for_index(id.0)
    }

    /// Returns the vm metrics counters for `id`,
    /// or panics if `id` was not pre-fetched in [`AllVmMetrics::new`].
    fn get_for_view_id(&self, id: ViewId) -> VmMetrics {
        // Counters for the first view starts after counters for the last reducer.
        self.get_for_index(self.num_reducers + id.0)
    }
}

/// VM-related metrics for reducer execution.
#[derive(Clone)]
struct VmMetrics {
    /// The time spent executing a reducer + plus evaluating its subscription queries.
    reducer_plus_query_duration: Histogram,
    /// The total VM fuel used.
    reducer_fuel_used: IntCounter,
    /// The total runtime of reducer calls.
    reducer_duration_usec: IntCounter,
    /// The total time spent in reducer ABI calls.
    reducer_abi_time_usec: IntCounter,
}

impl VmMetrics {
    /// Returns new metrics counters for `database_identity` and `reducer_name`.
    fn new(database_identity: &Identity, reducer_name: &str) -> Self {
        let reducer_plus_query_duration = WORKER_METRICS
            .reducer_plus_query_duration
            .with_label_values(database_identity, reducer_name);
        let reducer_fuel_used = DB_METRICS
            .reducer_wasmtime_fuel_used
            .with_label_values(database_identity, reducer_name);
        let reducer_duration_usec = DB_METRICS
            .reducer_duration_usec
            .with_label_values(database_identity, reducer_name);
        let reducer_abi_time_usec = DB_METRICS
            .reducer_abi_time_usec
            .with_label_values(database_identity, reducer_name);

        Self {
            reducer_plus_query_duration,
            reducer_fuel_used,
            reducer_duration_usec,
            reducer_abi_time_usec,
        }
    }

    /// Returns a timer guard for `reducer_plus_query_duration`.
    fn timer_guard_for_reducer_plus_query(&self, start: Instant) -> TimerGuard {
        self.reducer_plus_query_duration.clone().with_timer(start)
    }

    fn report_energy_used(&self, energy_used: FunctionBudget) {
        self.reducer_fuel_used.inc_by(energy_used.get());
    }

    fn report_total_duration(&self, duration: Duration) {
        self.reducer_duration_usec.inc_by(duration.as_micros() as u64);
    }

    fn report_abi_duration(&self, duration: Duration) {
        self.reducer_abi_time_usec.inc_by(duration.as_micros() as u64);
    }

    /// Reports some VM metrics.
    fn report(&self, stats: &ExecutionStats) {
        let energy_used = stats.energy.used();
        let reducer_duration = stats.timings.total_duration;
        let abi_time = stats.timings.wasm_instance_env_call_times.sum();
        self.report_energy_used(energy_used);
        self.report_total_duration(reducer_duration);
        self.report_abi_duration(abi_time);
    }
}

/// Starts the `call_function` span.
fn start_call_function_span(
    function_name: &str,
    caller_identity: &Identity,
    caller_connection_id_opt: Option<ConnectionId>,
) -> EnteredSpan {
    tracing::trace_span!("call_function",
        function_name,
        %caller_identity,
        caller_connection_id = caller_connection_id_opt.map(tracing::field::debug),
    )
    .entered()
}

/// Starts the `run_function` span.
fn start_run_function_span(budget: FunctionBudget) -> EnteredSpan {
    tracing::trace_span!(
        "run_function",
        timings.total_duration = tracing::field::Empty,
        energy.budget = budget.get(),
        energy.used = tracing::field::Empty,
    )
    .entered()
}

/// Logs a tracing message if a reducer doesn't finish in a single frame at 60 FPS.
fn maybe_log_long_running_function(reducer_name: &str, total_duration: Duration) {
    const FRAME_LEN_60FPS: Duration = Duration::from_secs(1).checked_div(60).unwrap();
    if total_duration > FRAME_LEN_60FPS {
        tracing::debug!(
            message = "Long running reducer finished executing",
            reducer_name,
            ?total_duration,
        );
    }
}

/// Logs an error `message` for `reducer` at `timestamp` into `replica_ctx`.
fn log_reducer_error(
    replica_ctx: &ReplicaContext,
    timestamp: Timestamp,
    reducer: &str,
    message: &str,
    module_hash: &Hash,
) {
    use database_logger::Record;

    WORKER_METRICS
        .sender_errors
        .with_label_values(&replica_ctx.database_identity, module_hash, reducer)
        .inc();

    log::info!("reducer returned error: {message}");

    let record = Record {
        ts: chrono::DateTime::from_timestamp_micros(timestamp.to_micros_since_unix_epoch()).unwrap(),
        function: Some(reducer),
        ..Record::injected(message)
    };
    replica_ctx.logger.write(database_logger::LogLevel::Error, &record, &());
}

/*
/// Detects lifecycle events for connecting/disconnecting a new client
/// and inserts/removes into `st_clients` depending on which.
fn lifecyle_modifications_to_tx(
    lifecycle: Option<Lifecycle>,
    caller_id: Identity,
    caller_conn_id: ConnectionId,
    db_id: Identity,
    tx: &mut MutTxId,
) -> Result<(), Box<str>> {
    match lifecycle {
        Some(Lifecycle::OnConnect) => tx.insert_st_client(caller_id, caller_conn_id),
        Some(Lifecycle::OnDisconnect) => tx.delete_st_client(caller_id, caller_conn_id, db_id),
        _ => Ok(()),
    }
    .map_err(|e| e.to_string().into())
}
*/

pub trait InstanceOp {
    fn name(&self) -> &str;
    fn timestamp(&self) -> Timestamp;
    fn call_type(&self) -> FuncCallType;
}

/// Describes a view call in a cheaply shareable way.
#[derive(Clone, Debug)]
pub struct ViewOp<'a> {
    pub name: &'a str,
    pub view_id: ViewId,
    pub table_id: TableId,
    pub fn_ptr: ViewFnPtr,
    pub args: &'a ArgsTuple,
    pub sender: &'a Identity,
    pub timestamp: Timestamp,
}

impl InstanceOp for ViewOp<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    fn call_type(&self) -> FuncCallType {
        FuncCallType::View(ViewCallInfo {
            view_id: self.view_id,
            table_id: self.table_id,
            fn_ptr: self.fn_ptr,
            sender: Some(*self.sender),
        })
    }
}

/// Describes an anonymous view call in a cheaply shareable way.
#[derive(Clone, Debug)]
pub struct AnonymousViewOp<'a> {
    pub name: &'a str,
    pub view_id: ViewId,
    pub table_id: TableId,
    pub fn_ptr: ViewFnPtr,
    pub args: &'a ArgsTuple,
    pub timestamp: Timestamp,
}

impl InstanceOp for AnonymousViewOp<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    fn call_type(&self) -> FuncCallType {
        FuncCallType::View(ViewCallInfo {
            view_id: self.view_id,
            table_id: self.table_id,
            fn_ptr: self.fn_ptr,
            sender: None,
        })
    }
}

/// Describes a reducer call in a cheaply shareable way.
#[derive(Clone, Debug)]
pub struct ReducerOp<'a> {
    pub id: ReducerId,
    pub name: &'a str,
    pub caller_identity: &'a Identity,
    pub caller_connection_id: &'a ConnectionId,
    pub timestamp: Timestamp,
    /// The arguments passed to the reducer.
    pub args: &'a ArgsTuple,
}

impl InstanceOp for ReducerOp<'_> {
    fn name(&self) -> &str {
        self.name
    }
    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
    fn call_type(&self) -> FuncCallType {
        FuncCallType::Reducer
    }
}

impl From<ReducerOp<'_>> for execution_context::ReducerContext {
    fn from(
        ReducerOp {
            id: _,
            name,
            caller_identity,
            caller_connection_id,
            timestamp,
            args,
        }: ReducerOp<'_>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            caller_identity: *caller_identity,
            caller_connection_id: *caller_connection_id,
            timestamp,
            arg_bsatn: args.get_bsatn().clone(),
        }
    }
}

/// Describes a procedure call in a cheaply shareable way.
#[derive(Clone, Debug)]
pub struct ProcedureOp {
    pub id: ProcedureId,
    pub name: Box<str>,
    pub caller_identity: Identity,
    pub caller_connection_id: ConnectionId,
    pub timestamp: Timestamp,
    pub arg_bytes: Bytes,
}

impl InstanceOp for ProcedureOp {
    fn name(&self) -> &str {
        &self.name
    }
    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
    fn call_type(&self) -> FuncCallType {
        FuncCallType::Procedure
    }
}
