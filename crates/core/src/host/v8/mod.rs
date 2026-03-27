use self::budget::energy_from_elapsed;
use self::error::{
    catch_exception, exception_already_thrown, log_traceback, ErrorOrException, ExcResult, ExceptionThrown,
    PinTryCatch, Throwable,
};
use self::ser::serialize_to_js;
use self::string::{str_from_ident, IntoJsString};
use self::syscall::{
    call_call_procedure, call_call_reducer, call_call_view, call_call_view_anon, call_describe_module, get_hooks,
    process_thrown_exception, resolve_sys_module, FnRet, HookFunctions,
};
use super::module_common::{build_common_module_from_raw, run_describer, ModuleCommon};
use super::module_host::{CallProcedureParams, CallReducerParams, ModuleInfo, ModuleWithInstance};
use super::UpdateDatabaseResult;
use crate::client::ClientActorId;
use crate::config::V8HeapPolicyConfig;
use crate::host::host_controller::CallProcedureReturn;
use crate::host::instance_env::{ChunkPool, InstanceEnv, TxSlot};
use crate::host::module_host::{
    call_identity_connected, init_database, ClientConnectedError, ViewCallError, ViewCommand, ViewCommandResult,
};
use crate::host::scheduler::{CallScheduledFunctionResult, ScheduledFunctionParams};
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    AnonymousViewOp, DescribeError, ExecutionError, ExecutionResult, ExecutionStats, ExecutionTimings, InstanceCommon,
    InstanceOp, ProcedureExecuteResult, ProcedureOp, ReducerExecuteResult, ReducerOp, ViewExecuteResult, ViewOp,
    WasmInstance,
};
use crate::host::wasm_common::{RowIters, TimingSpanSet};
use crate::host::{ModuleHost, ReducerCallError, ReducerCallResult, Scheduler};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_manager::TransactionOffset;
use crate::util::jobs::{AllocatedJobCore, CorePinner, LoadBalanceOnDropGuard};
use crate::worker_metrics::WORKER_METRICS;
use core::any::type_name;
use core::str;
use futures::future::LocalBoxFuture;
use futures::FutureExt;
use itertools::Either;
use parking_lot::RwLock;
use prometheus::IntGauge;
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_client_api_messages::energy::FunctionBudget;
use spacetimedb_datastore::locking_tx_datastore::FuncCallType;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{ConnectionId, Identity, RawModuleDef, Timestamp};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_table::static_assert_size;
use std::cell::Cell;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tracing::Instrument;
use v8::script_compiler::{compile_module, Source};
use v8::{
    scope_with_context, ArrayBuffer, Context, Function, Isolate, Local, MapFnTo, OwnedIsolate, PinScope,
    ResolveModuleCallback, ScriptOrigin, Value,
};

mod budget;
mod builtins;
mod de;
mod error;
mod from_value;
mod ser;
mod string;
mod syscall;
mod to_value;
mod util;

/// The V8 runtime, for modules written in e.g., JS or TypeScript.
pub struct V8Runtime {
    heap_policy: V8HeapPolicyConfig,
}

impl Default for V8Runtime {
    fn default() -> Self {
        Self::new(V8HeapPolicyConfig::default())
    }
}

impl V8Runtime {
    pub fn new(heap_policy: V8HeapPolicyConfig) -> Self {
        Self {
            heap_policy: heap_policy.normalized(),
        }
    }

    pub async fn make_actor(
        &self,
        mcc: ModuleCreationContext,
        program_bytes: &[u8],
        core: AllocatedJobCore,
    ) -> anyhow::Result<ModuleWithInstance> {
        V8_RUNTIME_GLOBAL
            .make_actor(mcc, program_bytes, core, self.heap_policy)
            .await
    }
}

#[cfg(test)]
impl V8Runtime {
    fn init_for_test() {
        LazyLock::force(&V8_RUNTIME_GLOBAL);
    }
}

static V8_RUNTIME_GLOBAL: LazyLock<V8RuntimeInner> = LazyLock::new(V8RuntimeInner::init);
static NEXT_JS_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // Note, `on_module_thread` runs host closures on a single JS module thread.
    // Enqueuing more JS module-thread work from one of those closures waits on the
    // same worker thread that is already busy running the current closure.
    // And this deadlocks.
    static ON_JS_MODULE_THREAD: Cell<bool> = const { Cell::new(false) };
}

struct EnteredJsModuleThread;

impl EnteredJsModuleThread {
    fn new() -> Self {
        ON_JS_MODULE_THREAD.with(|entered| {
            assert!(
                !entered.get(),
                "reentrancy into the JS module thread; this would deadlock. \
                 Do not enqueue onto this worker from inside `on_module_thread` work."
            );
            entered.set(true);
        });
        Self
    }
}

impl Drop for EnteredJsModuleThread {
    fn drop(&mut self) {
        ON_JS_MODULE_THREAD.with(|entered| {
            debug_assert!(
                entered.get(),
                "JS module thread marker should only be cleared after entry"
            );
            entered.set(false);
        });
    }
}

pub(crate) fn assert_not_on_js_module_thread(label: &str) {
    ON_JS_MODULE_THREAD.with(|entered| {
        assert!(
            !entered.get(),
            "{label} attempted to re-enter the JS module thread from code already \
             running on that thread; this would deadlock"
        );
    });
}

/// The actual V8 runtime, with initialization of V8.
struct V8RuntimeInner {
    _priv: (),
}

impl V8RuntimeInner {
    /// Initializes the V8 platform and engine.
    ///
    /// Should only be called once but it isn't unsound to call it more times.
    fn init() -> Self {
        // If the number in the name of this function is changed, update the version
        // of the `deno_core_icudata` dep to match the number in the function name.
        v8::icu::set_common_data_77(deno_core_icudata::ICU_DATA).ok();
        // Set a default locale for functions like `toLocaleString()`.
        // en-001 is "International English". <https://www.ctrl.blog/entry/en-001.html>
        v8::icu::set_default_locale("en-001");

        // We don't want idle tasks nor background worker tasks,
        // as we intend to run on a single core.
        // Per the docs, `new_single_threaded_default_platform` requires
        // that we pass `--single-threaded`.
        let mut flags = "--single-threaded".to_owned();
        if let Ok(env_flags) = std::env::var("STDB_V8_FLAGS") {
            flags.extend([" ", &env_flags]);
        }
        v8::V8::set_flags_from_string(&flags);
        let platform = v8::new_single_threaded_default_platform(false).make_shared();
        // Initialize V8. Internally, this uses a global lock so it's safe that we don't.
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();

        Self { _priv: () }
    }

    async fn make_actor(
        &self,
        mcc: ModuleCreationContext,
        program_bytes: &[u8],
        core: AllocatedJobCore,
        heap_policy: V8HeapPolicyConfig,
    ) -> anyhow::Result<ModuleWithInstance> {
        #![allow(unreachable_code, unused_variables)]

        log::trace!(
            "Making new V8 module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program_hash,
        );

        // Convert program to a string.
        let program: Arc<str> = str::from_utf8(program_bytes)?.into();

        // Validate/create the module and spawn the first instance.
        let mcc = Either::Right(mcc);
        let load_balance_guard = Arc::new(core.guard);
        let core_pinner = core.pinner;
        let (common, init_inst) = spawn_instance_worker(
            program.clone(),
            mcc,
            load_balance_guard.clone(),
            core_pinner.clone(),
            heap_policy,
        )
        .await?;
        let module = JsModule {
            common,
            program,
            load_balance_guard,
            core_pinner,
            heap_policy,
        };

        Ok(ModuleWithInstance::Js { module, init_inst })
    }
}

#[derive(Clone)]
pub struct JsModule {
    common: ModuleCommon,
    program: Arc<str>,
    load_balance_guard: Arc<LoadBalanceOnDropGuard>,
    core_pinner: CorePinner,
    heap_policy: V8HeapPolicyConfig,
}

impl JsModule {
    pub fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        self.common.replica_ctx()
    }

    pub fn scheduler(&self) -> &Scheduler {
        self.common.scheduler()
    }

    pub fn info(&self) -> Arc<ModuleInfo> {
        self.common.info().clone()
    }

    pub async fn create_instance(&self) -> JsInstance {
        let program = self.program.clone();
        let common = self.common.clone();
        let load_balance_guard = self.load_balance_guard.clone();
        let core_pinner = self.core_pinner.clone();
        let heap_policy = self.heap_policy;

        // This has to be done in a blocking context because of `blocking_recv`.
        let (_, instance) = spawn_instance_worker(
            program,
            Either::Left(common),
            load_balance_guard,
            core_pinner,
            heap_policy,
        )
        .await
        .expect("`spawn_instance_worker` should succeed when passed `ModuleCommon`");
        instance
    }
}

/// Returns the `JsInstanceEnv` bound to an [`Isolate`], fallibly.
fn env_on_isolate(isolate: &mut Isolate) -> Option<&mut JsInstanceEnv> {
    isolate.get_slot_mut()
}

/// Returns the `JsInstanceEnv` bound to an [`Isolate`], or panic if not set.
fn env_on_isolate_unwrap(isolate: &mut Isolate) -> &mut JsInstanceEnv {
    env_on_isolate(isolate).expect("there should be a `JsInstanceEnv`")
}

/// The environment of a [`JsInstance`].
struct JsInstanceEnv {
    instance_env: InstanceEnv,
    module_def: Option<Arc<ModuleDef>>,

    /// The slab of `BufferIters` created for this instance.
    iters: RowIters,

    /// Track time spent in module-defined spans.
    timing_spans: TimingSpanSet,

    /// Track time spent in all wasm instance env calls (aka syscall time).
    ///
    /// Each function, like `insert`, will add the `Duration` spent in it
    /// to this tracker.
    call_times: CallTimes,

    /// A pool of unused allocated chunks that can be reused.
    // TODO(Centril): consider using this pool for `console_timer_start` and `bytes_sink_write`.
    chunk_pool: ChunkPool,
}

impl JsInstanceEnv {
    /// Returns a new [`JsInstanceEnv`] wrapping `instance_env` with some defaults.
    fn new(instance_env: InstanceEnv) -> Self {
        Self {
            instance_env,
            module_def: None,
            call_times: CallTimes::new(),
            iters: <_>::default(),
            chunk_pool: <_>::default(),
            timing_spans: <_>::default(),
        }
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is beginning.
    ///
    /// Returns the handle used by reducers to read from `args`
    /// as well as the handle used to write the error message, if any.
    fn start_funcall(&mut self, name: Identifier, ts: Timestamp, func_type: FuncCallType) {
        self.instance_env.start_funcall(name, ts, func_type);
    }

    /// Returns the name of the most recent reducer to be run in this environment,
    /// or `None` if no reducer is actively being invoked.
    fn log_record_function(&self) -> Option<&str> {
        self.instance_env.log_record_function()
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    fn reducer_start(&self) -> Instant {
        self.instance_env.start_instant
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is over.
    /// This resets all of the state associated to a single reducer call,
    /// and returns instrumentation records.
    fn finish_reducer(&mut self) -> ExecutionTimings {
        let total_duration = self.reducer_start().elapsed();

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        }
    }

    fn set_module_def(&mut self, module_def: Arc<ModuleDef>) {
        self.module_def = Some(module_def);
    }

    fn module_def(&self) -> Option<Arc<ModuleDef>> {
        self.module_def.clone()
    }
}

/// An instance for a [`JsModule`].
///
/// The actual work happens in a worker thread,
/// which the instance communicates with through channels.
///
/// This handle is cloneable and shared by callers. Requests are queued FIFO
/// on the worker thread so the next reducer can start immediately after the
/// previous one finishes, without waiting for an outer task to hand the
/// instance back.
///
/// When the last handle is dropped, the channels will hang up,
/// which will cause the worker's loop to terminate and cleanup the isolate
/// and friends.
#[derive(Clone)]
pub struct JsInstance {
    /// Stable identifier for the underlying worker generation.
    ///
    /// All clones of the same handle share the same `id`. The instance lane uses
    /// it to tell whether the currently active worker has already been replaced
    /// after a trap or disconnect.
    id: u64,
    request_tx: flume::Sender<JsWorkerRequest>,
    trapped: Arc<AtomicBool>,
}

impl JsInstance {
    fn id(&self) -> u64 {
        self.id
    }

    pub fn trapped(&self) -> bool {
        self.trapped.load(Ordering::Relaxed)
    }

    async fn send_request<T>(
        &self,
        request: impl FnOnce(JsReplyTx<T>) -> JsWorkerRequest,
    ) -> Result<T, WorkerDisconnected> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send_async(request(reply_tx))
            .await
            .map_err(|_| WorkerDisconnected)?;
        let JsWorkerReply { value, trapped } = reply_rx.await.map_err(|_| WorkerDisconnected)?;
        if trapped {
            self.trapped.store(true, Ordering::Relaxed);
        }
        Ok(value)
    }

    pub async fn run_on_thread<F, R>(&self, f: F) -> R
    where
        F: AsyncFnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = tracing::Span::current();
        let (tx, rx) = oneshot::channel();

        self.request_tx
            .send_async(JsWorkerRequest::RunFunction(Box::new(move || {
                async move {
                    let result = AssertUnwindSafe(f().instrument(span)).catch_unwind().await;
                    if let Err(Err(_panic)) = tx.send(result) {
                        tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                    }
                }
                .boxed_local()
            })))
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while handling {}", type_name::<R>()));

        match rx
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while handling {}", type_name::<R>()))
        {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    pub async fn update_database(
        &self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.send_request(|reply_tx| JsWorkerRequest::UpdateDatabase {
            reply_tx,
            program,
            old_module_info,
            policy,
        })
        .await
        .unwrap_or_else(|_| panic!("worker should stay live while updating the database"))
    }

    pub async fn call_reducer(&self, params: CallReducerParams) -> ReducerCallResult {
        self.send_request(|reply_tx| JsWorkerRequest::CallReducer { reply_tx, params })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while calling a reducer"))
    }

    pub async fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.send_request(JsWorkerRequest::ClearAllClients)
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while clearing clients"))
    }

    pub async fn call_identity_connected(
        &self,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        self.send_request(|reply_tx| JsWorkerRequest::CallIdentityConnected {
            reply_tx,
            caller_auth,
            caller_connection_id,
        })
        .await
        .unwrap_or_else(|_| panic!("worker should stay live while running client_connected"))
    }

    pub async fn call_identity_disconnected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ReducerCallError> {
        self.send_request(|reply_tx| JsWorkerRequest::CallIdentityDisconnected {
            reply_tx,
            caller_identity,
            caller_connection_id,
        })
        .await
        .unwrap_or_else(|_| panic!("worker should stay live while running client_disconnected"))
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) -> Result<(), ReducerCallError> {
        self.send_request(|reply_tx| JsWorkerRequest::DisconnectClient { reply_tx, client_id })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while disconnecting a client"))
    }

    pub async fn init_database(&self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        self.send_request(|reply_tx| JsWorkerRequest::InitDatabase { reply_tx, program })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while initializing the database"))
    }

    pub async fn call_procedure(&self, params: CallProcedureParams) -> CallProcedureReturn {
        self.send_request(|reply_tx| JsWorkerRequest::CallProcedure { reply_tx, params })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while calling a procedure"))
    }

    pub async fn call_view(&self, cmd: ViewCommand) -> ViewCommandResult {
        self.send_request(|reply_tx| JsWorkerRequest::CallView { reply_tx, cmd })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while calling a view"))
    }

    pub(in crate::host) async fn call_scheduled_function(
        &self,
        params: ScheduledFunctionParams,
    ) -> CallScheduledFunctionResult {
        self.send_request(|reply_tx| JsWorkerRequest::CallScheduledFunction { reply_tx, params })
            .await
            .unwrap_or_else(|_| panic!("worker should stay live while calling a scheduled function"))
    }
}

#[derive(Clone, Copy, Debug)]
struct WorkerDisconnected;

fn instance_lane_worker_error(label: &'static str) -> String {
    format!("instance lane worker exited while handling {label}")
}

struct JsWorkerReply<T> {
    value: T,
    trapped: bool,
}

type JsReplyTx<T> = oneshot::Sender<JsWorkerReply<T>>;

/// Requests sent to the dedicated JS worker thread.
///
/// Most variants carry a `reply_tx` because the worker thread owns the isolate,
/// executes the request there, and then has to send both the typed result and
/// the worker's trapped-bit back to the async caller.
enum JsWorkerRequest {
    /// See [`JsInstance::run_on_thread`].
    ///
    /// This variant does not expect a [`JsWorkerReply`].
    RunFunction(Box<dyn FnOnce() -> LocalBoxFuture<'static, ()> + Send>),
    /// See [`JsInstance::update_database`].
    UpdateDatabase {
        reply_tx: JsReplyTx<anyhow::Result<UpdateDatabaseResult>>,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    },
    /// See [`JsInstance::call_reducer`].
    CallReducer {
        reply_tx: JsReplyTx<ReducerCallResult>,
        params: CallReducerParams,
    },
    /// See [`JsInstance::call_view`].
    CallView {
        reply_tx: JsReplyTx<ViewCommandResult>,
        cmd: ViewCommand,
    },
    /// See [`JsInstance::call_procedure`].
    CallProcedure {
        reply_tx: JsReplyTx<CallProcedureReturn>,
        params: CallProcedureParams,
    },
    /// See [`JsInstance::clear_all_clients`].
    ClearAllClients(JsReplyTx<anyhow::Result<()>>),
    /// See [`JsInstance::call_identity_connected`].
    CallIdentityConnected {
        reply_tx: JsReplyTx<Result<(), ClientConnectedError>>,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    },
    /// See [`JsInstance::call_identity_disconnected`].
    CallIdentityDisconnected {
        reply_tx: JsReplyTx<Result<(), ReducerCallError>>,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    },
    /// See [`JsInstance::disconnect_client`].
    DisconnectClient {
        reply_tx: JsReplyTx<Result<(), ReducerCallError>>,
        client_id: ClientActorId,
    },
    /// See [`JsInstance::init_database`].
    InitDatabase {
        reply_tx: JsReplyTx<anyhow::Result<Option<ReducerCallResult>>>,
        program: Program,
    },
    /// See [`JsInstance::call_scheduled_function`].
    CallScheduledFunction {
        reply_tx: JsReplyTx<CallScheduledFunctionResult>,
        params: ScheduledFunctionParams,
    },
}

static_assert_size!(CallReducerParams, 256);

fn send_worker_reply<T>(ctx: &str, reply_tx: JsReplyTx<T>, value: T, trapped: bool) {
    if reply_tx.send(JsWorkerReply { value, trapped }).is_err() {
        log::error!("should have receiver for `{ctx}` response");
    }
}

struct V8HeapMetrics {
    total_heap_size_bytes: IntGauge,
    total_physical_size_bytes: IntGauge,
    used_global_handles_size_bytes: IntGauge,
    used_heap_size_bytes: IntGauge,
    heap_size_limit_bytes: IntGauge,
    external_memory_bytes: IntGauge,
    native_contexts: IntGauge,
    detached_contexts: IntGauge,
}

impl V8HeapMetrics {
    fn new(database_identity: &Identity) -> Self {
        Self {
            total_heap_size_bytes: WORKER_METRICS
                .v8_total_heap_size_bytes
                .with_label_values(database_identity),
            total_physical_size_bytes: WORKER_METRICS
                .v8_total_physical_size_bytes
                .with_label_values(database_identity),
            used_global_handles_size_bytes: WORKER_METRICS
                .v8_used_global_handles_size_bytes
                .with_label_values(database_identity),
            used_heap_size_bytes: WORKER_METRICS
                .v8_used_heap_size_bytes
                .with_label_values(database_identity),
            heap_size_limit_bytes: WORKER_METRICS
                .v8_heap_size_limit_bytes
                .with_label_values(database_identity),
            external_memory_bytes: WORKER_METRICS
                .v8_external_memory_bytes
                .with_label_values(database_identity),
            native_contexts: WORKER_METRICS.v8_native_contexts.with_label_values(database_identity),
            detached_contexts: WORKER_METRICS.v8_detached_contexts.with_label_values(database_identity),
        }
    }

    fn observe(&self, stats: &v8::HeapStatistics) {
        self.total_heap_size_bytes.set(stats.total_heap_size() as i64);
        self.total_physical_size_bytes.set(stats.total_physical_size() as i64);
        self.used_global_handles_size_bytes
            .set(stats.used_global_handles_size() as i64);
        self.used_heap_size_bytes.set(stats.used_heap_size() as i64);
        self.heap_size_limit_bytes.set(stats.heap_size_limit() as i64);
        self.external_memory_bytes.set(stats.external_memory() as i64);
        self.native_contexts.set(stats.number_of_native_contexts() as i64);
        self.detached_contexts.set(stats.number_of_detached_contexts() as i64);
    }
}

fn sample_heap_stats(scope: &mut PinScope<'_, '_>, metrics: &V8HeapMetrics) -> v8::HeapStatistics {
    let stats = scope.get_heap_statistics();
    metrics.observe(&stats);
    stats
}

fn heap_usage(stats: &v8::HeapStatistics) -> (usize, usize) {
    (stats.used_heap_size(), stats.heap_size_limit())
}

fn heap_fraction_at_or_above(used: usize, limit: usize, fraction: f64) -> bool {
    limit > 0 && ((used as f64) / (limit as f64)) >= fraction
}

/// The single JS worker can process an unbounded number of reducer calls over its lifetime.
/// That is great for locality, but it also means any JS heap retention that would previously
/// have been spread across several pooled isolates now accumulates in one isolate.
///
/// If heap usage is close to the configured limit even after manually invoking GC,
/// we'll instantiate a new isolate to reclaim memory and avoid OOMing the current one.
fn should_retire_worker_for_heap(
    scope: &mut PinScope<'_, '_>,
    metrics: &V8HeapMetrics,
    config: V8HeapPolicyConfig,
) -> Option<(usize, usize)> {
    let stats = sample_heap_stats(scope, metrics);
    let (used, limit) = heap_usage(&stats);
    if !heap_fraction_at_or_above(used, limit, config.heap_gc_trigger_fraction) {
        return None;
    }

    scope.low_memory_notification();
    let stats = sample_heap_stats(scope, metrics);
    let (used, limit) = heap_usage(&stats);
    if heap_fraction_at_or_above(used, limit, config.heap_retire_fraction) {
        Some((used, limit))
    } else {
        None
    }
}

struct JsInstanceLaneState {
    // Instance-lane calls stay on one active worker for locality. The hot path clones
    // this handle and feeds work straight into the worker-owned FIFO; trap
    // recovery very rarely swaps it out for a fresh instance.
    active: RwLock<JsInstance>,

    // Replacement must be serialized because multiple callers can all observe
    // the same worker becoming unusable and attempt recovery at once.
    //
    // This must be an async mutex rather than a blocking mutex because recovery
    // may need to call `create_instance().await`.
    //
    // This stays safe as long as these invariants hold:
    // - `replace_lock` is only for trap/disconnect recovery, never the hot path.
    // - no `parking_lot` guard is held across the `.await` in replacement.
    // - `create_instance()` must not call back into `JsInstanceLane` or try to
    //   take `replace_lock`.
    replace_lock: AsyncMutex<()>,
}

/// A single serialized execution lane for JS module work.
///
/// Callers share one active [`JsInstance`] so hot requests stay on the same
/// worker thread for locality. The lane only steps in on the rare path, where
/// a trap or disconnect forces that active worker to be replaced.
#[derive(Clone)]
pub struct JsInstanceLane {
    module: JsModule,
    state: Arc<JsInstanceLaneState>,
}

impl JsInstanceLane {
    pub fn new(module: JsModule, init_inst: JsInstance) -> Self {
        Self {
            module,
            state: Arc::new(JsInstanceLaneState {
                active: RwLock::new(init_inst),
                replace_lock: AsyncMutex::new(()),
            }),
        }
    }

    fn active_instance(&self) -> JsInstance {
        self.state.active.read().clone()
    }

    async fn after_successful_call(&self, active: &JsInstance) {
        if active.trapped() {
            self.replace_active_if_current(active).await;
        }
    }

    async fn replace_active_if_current(&self, trapped: &JsInstance) {
        // `replace_lock` intentionally serializes the rare recovery path. This
        // prevents a trap observed by many callers from spawning many replacement
        // workers and racing to install them.
        let _replace_guard = self.state.replace_lock.lock().await;

        // The same trapped instance can be observed by multiple callers at once.
        // We only want the first one to do the swap; everybody else should notice
        // that the active handle already changed and get out of the way.
        if self.state.active.read().id() != trapped.id() {
            return;
        }

        log::warn!("instance lane worker needs replacement; creating a fresh instance-lane worker");

        // Keep the awaited instance creation outside of any `parking_lot` guard.
        // The only lock held across this await is `replace_lock`, which is why it
        // has to be async.
        let next = self.module.create_instance().await;
        *self.state.active.write() = next;
    }

    /// Run an instance-lane operation exactly once.
    ///
    /// If the worker disappears before replying, we replace it for future
    /// requests but surface the disconnect to the caller instead of retrying.
    /// This keeps instance-lane semantics closer to the old pooled-instance
    /// model now that the worker queue is a rendezvous channel.
    async fn run_once<R>(
        &self,
        label: &'static str,
        work: impl AsyncFnOnce(JsInstance) -> Result<R, WorkerDisconnected>,
    ) -> Result<R, WorkerDisconnected> {
        assert_not_on_js_module_thread(label);

        let active = self.active_instance();
        let result = work(active.clone()).await;
        match result {
            Ok(value) => {
                self.after_successful_call(&active).await;
                Ok(value)
            }
            Err(err) => {
                self.replace_active_if_current(&active).await;
                log::error!("instance-lane operation {label} lost its worker before replying");
                Err(err)
            }
        }
    }

    /// Run an arbitrary closure on the instance-lane worker thread without replay.
    ///
    /// This is non-replayable because the closure is opaque host code, not a
    /// cloneable request payload, and it may have already produced host-side
    /// effects before a worker disconnect is observed.
    pub async fn run_on_thread<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: AsyncFnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let span = tracing::Span::current();
        self.run_once("run_on_thread", async move |inst| {
            let (tx, rx) = oneshot::channel();

            inst.request_tx
                .send_async(JsWorkerRequest::RunFunction(Box::new(move || {
                    async move {
                        let _on_js_module_thread = EnteredJsModuleThread::new();
                        let result = AssertUnwindSafe(f().instrument(span)).catch_unwind().await;
                        if let Err(Err(_panic)) = tx.send(result) {
                            tracing::warn!("uncaught panic on `SingleCoreExecutor`")
                        }
                    }
                    .boxed_local()
                })))
                .await
                .map_err(|_| WorkerDisconnected)?;

            Ok(match rx.await.map_err(|_| WorkerDisconnected)? {
                Ok(r) => r,
                Err(e) => std::panic::resume_unwind(e),
            })
        })
        .await
        .map_err(|_| anyhow::anyhow!("instance lane worker exited while running a non-replayable module-thread task"))
    }

    /// Run a database update on the instance lane exactly once.
    ///
    /// If the worker disappears before replying, we replace it for future
    /// requests and surface an internal host error to the caller.
    pub async fn update_database(
        &self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.run_once("update_database", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::UpdateDatabase {
                reply_tx,
                program,
                old_module_info,
                policy,
            })
            .await
        })
        .await
        .map_err(|_| anyhow::anyhow!(instance_lane_worker_error("update_database")))?
    }

    /// Run a reducer on the instance lane exactly once.
    ///
    /// A real reducer trap still returns the reducer's own outcome before the
    /// worker is replaced. If the worker disappears before any reply, we surface
    /// that as `ReducerCallError::WorkerError`.
    pub async fn call_reducer(&self, params: CallReducerParams) -> Result<ReducerCallResult, ReducerCallError> {
        self.run_once("call_reducer", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::CallReducer { reply_tx, params })
                .await
        })
        .await
        .map_err(|_| ReducerCallError::WorkerError(instance_lane_worker_error("call_reducer")))
    }

    /// Clear all instance-lane client state exactly once.
    pub async fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.run_once("clear_all_clients", |inst: JsInstance| async move {
            inst.send_request(JsWorkerRequest::ClearAllClients).await
        })
        .await
        .map_err(|_| anyhow::anyhow!(instance_lane_worker_error("clear_all_clients")))?
    }

    /// Run the `client_connected` lifecycle reducer exactly once.
    ///
    /// If the worker disappears before replying, we replace it for future
    /// requests and reject the connection with `ReducerCallError::WorkerError`.
    pub async fn call_identity_connected(
        &self,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        self.run_once("call_identity_connected", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::CallIdentityConnected {
                reply_tx,
                caller_auth,
                caller_connection_id,
            })
            .await
        })
        .await
        .map_err(|_| {
            ClientConnectedError::from(ReducerCallError::WorkerError(instance_lane_worker_error(
                "call_identity_connected",
            )))
        })?
    }

    /// Run the `client_disconnected` lifecycle reducer exactly once.
    pub async fn call_identity_disconnected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ReducerCallError> {
        self.run_once("call_identity_disconnected", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::CallIdentityDisconnected {
                reply_tx,
                caller_identity,
                caller_connection_id,
            })
            .await
        })
        .await
        .map_err(|_| ReducerCallError::WorkerError(instance_lane_worker_error("call_identity_disconnected")))?
    }

    /// Run disconnect cleanup on the instance lane exactly once.
    pub async fn disconnect_client(&self, client_id: ClientActorId) -> Result<(), ReducerCallError> {
        self.run_once("disconnect_client", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::DisconnectClient { reply_tx, client_id })
                .await
        })
        .await
        .map_err(|_| ReducerCallError::WorkerError(instance_lane_worker_error("disconnect_client")))?
    }

    /// Run reducer-style database initialization exactly once.
    pub async fn init_database(&self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        self.run_once("init_database", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::InitDatabase { reply_tx, program })
                .await
        })
        .await
        .map_err(|_| anyhow::anyhow!(instance_lane_worker_error("init_database")))?
    }

    /// Run a view/subscription command on the instance lane exactly once.
    ///
    /// If the worker disappears before replying, we replace it for future
    /// requests and surface a `ViewCallError::InternalError`.
    pub async fn call_view(&self, cmd: ViewCommand) -> Result<ViewCommandResult, ViewCallError> {
        self.run_once("call_view", |inst: JsInstance| async move {
            inst.send_request(|reply_tx| JsWorkerRequest::CallView { reply_tx, cmd })
                .await
        })
        .await
        .map_err(|_| ViewCallError::InternalError(instance_lane_worker_error("call_view")))
    }
}

/// Performs some of the startup work of [`spawn_instance_worker`].
///
/// NOTE(centril): in its own function due to lack of `try` blocks.
fn startup_instance_worker<'scope>(
    scope: &mut PinScope<'scope, '_>,
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContext>,
) -> anyhow::Result<(HookFunctions<'scope>, ModuleCommon)> {
    let hook_functions = catch_exception(scope, |scope| {
        // Start-up the user's module.
        let exports_obj = eval_user_module(scope, &program)?;

        // Find the hook functions.
        let hooks =
            get_hooks(scope, exports_obj)?.ok_or_else(|| anyhow::anyhow!("must export schema as default export"))?;
        Ok(hooks)
    })?;

    // If we don't have a module, make one.
    let module_common = match module_or_mcc {
        Either::Left(module_common) => module_common,
        Either::Right(mcc) => {
            let def = extract_description(scope, &hook_functions, &mcc.replica_ctx)?;

            // Validate and create a common module from the raw definition.
            build_common_module_from_raw(mcc, def)?
        }
    };

    Ok((hook_functions, module_common))
}

/// Returns a new isolate.
fn new_isolate(heap_policy: V8HeapPolicyConfig) -> OwnedIsolate {
    let params = if let Some(heap_limit_bytes) = heap_policy.heap_limit_bytes {
        v8::CreateParams::default().heap_limits(0, heap_limit_bytes)
    } else {
        v8::CreateParams::default()
    };
    let mut isolate = Isolate::new(params);
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 1024);
    isolate
}

/// Spawns an instance worker for `program`
/// and returns on success the corresponding [`JsInstance`]
/// that talks to the worker.
///
/// When [`ModuleCommon`] is passed, it's assumed that `spawn_instance_worker`
/// has already happened once for this `program` and that it has been
/// validated. In that case, `Ok(_)` should be returned.
///
/// Otherwise, when [`ModuleCreationContext`] is passed,
/// this is the first time both the module and instance are created.
///
/// `load_balance_guard` and `core_pinner` should both be from the same
/// [`AllocatedJobCore`], and are used to manage the core pinning of this thread.
async fn spawn_instance_worker(
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContext>,
    load_balance_guard: Arc<LoadBalanceOnDropGuard>,
    mut core_pinner: CorePinner,
    heap_policy: V8HeapPolicyConfig,
) -> anyhow::Result<(ModuleCommon, JsInstance)> {
    // Spawn a rendezvous queue for requests to the worker.
    // Multiple callers can wait to hand work to the worker, but with
    // `bounded(0)` there is no buffered backlog inside the channel itself.
    // The worker still processes requests strictly one at a time.
    let (request_tx, request_rx) = flume::bounded(0);

    // This one-shot channel is used for initial startup error handling within the thread.
    let (result_tx, result_rx) = oneshot::channel();
    let trapped = Arc::new(AtomicBool::new(false));
    let worker_trapped = trapped.clone();

    let rt = tokio::runtime::Handle::current();

    std::thread::spawn(move || {
        let _guard = load_balance_guard;
        core_pinner.pin_now();

        let _entered = rt.enter();

        // Create the isolate and scope.
        let mut isolate = new_isolate(heap_policy);
        scope_with_context!(let scope, &mut isolate, Context::new(scope, Default::default()));

        // Setup the instance environment.
        let (replica_ctx, scheduler, idc_sender) = match &module_or_mcc {
            Either::Left(module) => (module.replica_ctx(), module.scheduler(), module.idc_sender()),
            Either::Right(mcc) => (&mcc.replica_ctx, &mcc.scheduler, mcc.idc_sender.clone()),
        };
        let instance_env = InstanceEnv::new(replica_ctx.clone(), scheduler.clone(), idc_sender);
        scope.set_slot(JsInstanceEnv::new(instance_env));

        catch_exception(scope, |scope| Ok(builtins::evaluate_builtins(scope)?))
            .expect("our builtin code shouldn't error");

        // Setup the JS module, find call_reducer, and maybe build the module.
        let send_result = |res| {
            result_tx.send(res).inspect_err(|_| {
                // This should never happen as we immediately `.recv` on the
                // other end of the channel, but sometimes it gets cancelled.
                log::error!("startup result receiver disconnected");
            })
        };
        let (hooks, module_common) = match startup_instance_worker(scope, program, module_or_mcc) {
            Err(err) => {
                // There was some error in module setup.
                // Return the error and terminate the worker.
                let _ = send_result(Err(err));
                return;
            }
            Ok((crf, module_common)) => {
                env_on_isolate_unwrap(scope).set_module_def(module_common.info().module_def.clone());
                // Success! Send `module_common` to the spawner.
                if send_result(Ok(module_common.clone())).is_err() {
                    return;
                }
                (crf, module_common)
            }
        };

        if let Some(get_error_constructor) = hooks.get_error_constructor {
            scope
                .get_current_context()
                .set_embedder_data(GET_ERROR_CONSTRUCTOR_SLOT, get_error_constructor.into());
        }

        // Setup the instance common.
        let info = &module_common.info();
        let mut instance_common = InstanceCommon::new(&module_common);
        let replica_ctx: &Arc<ReplicaContext> = module_common.replica_ctx();
        let heap_metrics = V8HeapMetrics::new(&info.database_identity);

        // Create a zero-initialized buffer for holding reducer args.
        // Arguments needing more space will not use this.
        const REDUCER_ARGS_BUFFER_SIZE: usize = 4_096; // 1 page.
        let reducer_args_buf = ArrayBuffer::new(scope, REDUCER_ARGS_BUFFER_SIZE);

        let mut inst = V8Instance {
            scope,
            replica_ctx,
            hooks: &hooks,
            reducer_args_buf,
        };
        let _initial_heap_stats = sample_heap_stats(inst.scope, &heap_metrics);

        // Process requests to the worker.
        //
        // The loop is terminated when the last `JsInstance` handle is dropped.
        // This will cause channels, scopes, and the isolate to be cleaned up.
        let mut requests_since_heap_check = 0u64;
        let mut last_heap_check_at = Instant::now();
        for request in request_rx.iter() {
            let mut call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, &mut inst);
            let mut should_exit = false;

            core_pinner.pin_if_changed();

            match request {
                JsWorkerRequest::RunFunction(f) => rt.block_on(f()),
                JsWorkerRequest::UpdateDatabase {
                    reply_tx,
                    program,
                    old_module_info,
                    policy,
                } => {
                    let res = instance_common.update_database(program, old_module_info, policy, &mut inst);
                    send_worker_reply("update_database", reply_tx, res, false);
                }
                JsWorkerRequest::CallReducer { reply_tx, params } => {
                    let (res, trapped) = call_reducer(None, params);
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_reducer", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::CallView { reply_tx, cmd } => {
                    let (res, trapped) = instance_common.handle_cmd(cmd, &mut inst);
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_view", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::CallProcedure { reply_tx, params } => {
                    let (res, trapped) = instance_common
                        .call_procedure(params, &mut inst)
                        .now_or_never()
                        .expect("our call_procedure implementation is not actually async");
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_procedure", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::ClearAllClients(reply_tx) => {
                    let res = instance_common.clear_all_clients();
                    send_worker_reply("clear_all_clients", reply_tx, res, false);
                }
                JsWorkerRequest::CallIdentityConnected {
                    reply_tx,
                    caller_auth,
                    caller_connection_id,
                } => {
                    let mut trapped = false;
                    let res =
                        call_identity_connected(caller_auth, caller_connection_id, info, call_reducer, &mut trapped);
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_identity_connected", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::CallIdentityDisconnected {
                    reply_tx,
                    caller_identity,
                    caller_connection_id,
                } => {
                    let mut trapped = false;
                    let res = ModuleHost::call_identity_disconnected_inner(
                        caller_identity,
                        caller_connection_id,
                        info,
                        call_reducer,
                        &mut trapped,
                    );
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_identity_disconnected", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::DisconnectClient { reply_tx, client_id } => {
                    let mut trapped = false;
                    let res = ModuleHost::disconnect_client_inner(client_id, info, call_reducer, &mut trapped);
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("disconnect_client", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::InitDatabase { reply_tx, program } => {
                    let (res, trapped): (Result<Option<ReducerCallResult>, anyhow::Error>, bool) =
                        init_database(replica_ctx, &module_common.info().module_def, program, call_reducer);
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("init_database", reply_tx, res, trapped);
                    should_exit = trapped;
                }
                JsWorkerRequest::CallScheduledFunction { reply_tx, params } => {
                    let (res, trapped) = instance_common
                        .call_scheduled_function(params, &mut inst)
                        .now_or_never()
                        .expect("our call_procedure implementation is not actually async");
                    worker_trapped.store(trapped, Ordering::Relaxed);
                    send_worker_reply("call_scheduled_function", reply_tx, res, trapped);
                    should_exit = trapped;
                }
            }

            if !should_exit {
                let request_check_due = heap_policy.heap_check_request_interval.is_some_and(|interval| {
                    requests_since_heap_check += 1;
                    requests_since_heap_check >= interval
                });
                let time_check_due = heap_policy
                    .heap_check_time_interval
                    .is_some_and(|interval| last_heap_check_at.elapsed() >= interval);
                if request_check_due || time_check_due {
                    requests_since_heap_check = 0;
                    last_heap_check_at = Instant::now();
                    if let Some((used, limit)) = should_retire_worker_for_heap(inst.scope, &heap_metrics, heap_policy) {
                        worker_trapped.store(true, Ordering::Relaxed);
                        should_exit = true;
                        log::warn!(
                            "retiring JS worker after V8 heap stayed high post-GC: used={}MiB limit={}MiB",
                            used / (1024 * 1024),
                            limit / (1024 * 1024),
                        );
                    }
                }
            }

            // Once a JS instance traps, we must not let later queued work execute
            // on that poisoned isolate. We reply to the trapping request first so
            // the caller can observe the actual reducer/procedure error, and then
            // shut the worker down so later callers retry on a fresh instance.
            //
            // We also retire workers when they stay near the V8 heap limit after
            // a GC. The instance lane intentionally keeps a JS worker alive for
            // a long time, so this gives it a bounded-memory replacement policy
            // instead of letting one isolate absorb the entire module lifetime.
            if should_exit {
                break;
            }
        }
    });

    // Get the module, if any, and get any setup errors from the worker.
    let res: Result<ModuleCommon, anyhow::Error> = result_rx.await.expect("should have a sender");
    res.map(|opt_mc| {
        let inst = JsInstance {
            id: NEXT_JS_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
            request_tx,
            trapped,
        };
        (opt_mc, inst)
    })
}

/// The embedder data slot for the `__get_error_constructor__` function.
const GET_ERROR_CONSTRUCTOR_SLOT: i32 = syscall::ModuleHookKey::GetErrorConstructor as i32;

/// Compiles, instantiate, and evaluate `code` as a module.
fn eval_module<'scope>(
    scope: &mut PinScope<'scope, '_>,
    resource_name: Local<'scope, Value>,
    code: Local<'_, v8::String>,
    resolve_deps: impl MapFnTo<ResolveModuleCallback<'scope>>,
) -> ExcResult<Local<'scope, v8::Module>> {
    // Assemble the source. v8 figures out things like the `script_id` and
    // `source_map_url` itself, so we don't actually have to provide them.
    let origin = ScriptOrigin::new(scope, resource_name, 0, 0, false, 0, None, false, false, true, None);
    let source = &mut Source::new(code, Some(&origin));

    // Compile the module.
    let module = compile_module(scope, source).ok_or_else(exception_already_thrown)?;

    // Instantiate the module.
    module
        .instantiate_module(scope, resolve_deps)
        .filter(|x| *x)
        .ok_or_else(exception_already_thrown)?;

    // Evaluate the module.
    let value = module.evaluate(scope).ok_or_else(exception_already_thrown)?;

    if module.get_status() == v8::ModuleStatus::Errored {
        // If there's an exception while evaluating the code of the module, `evaluate()` won't
        // throw, but instead the status will be `Errored` and the exception can be obtained from
        // `get_exception()`.
        return Err(error::ExceptionValue(module.get_exception()).throw(scope));
    }

    let value = value.cast::<v8::Promise>();
    match value.state() {
        v8::PromiseState::Pending => {
            // If the user were to put top-level `await new Promise((resolve) => { /* do nothing */ })`
            // the module value would never actually resolve. For now, reject this entirely.
            Err(error::TypeError("module has top-level await and is pending").throw(scope))
        }
        v8::PromiseState::Rejected => Err(error::ExceptionValue(value.result(scope)).throw(scope)),
        v8::PromiseState::Fulfilled => Ok(module),
    }
}

/// Compiles, instantiate, and evaluate the user module with `code`.
/// Returns the exports of the module.
fn eval_user_module<'scope>(scope: &mut PinScope<'scope, '_>, code: &str) -> ExcResult<Local<'scope, v8::Object>> {
    // Convert the code to a string.
    let code = code.into_string(scope).map_err(|e| e.into_range_error().throw(scope))?;

    let name = str_from_ident!(spacetimedb_module).string(scope).into();
    let module = eval_module(scope, name, code, resolve_sys_module)?;
    Ok(module.get_module_namespace().cast())
}

/// Calls free function `fun` with `args`.
fn call_free_fun<'scope>(
    scope: &PinScope<'scope, '_>,
    fun: Local<'scope, Function>,
    args: &[Local<'scope, Value>],
) -> FnRet<'scope> {
    let receiver = v8::undefined(scope).into();
    call_recv_fun(scope, fun, receiver, args)
}

/// Calls function `fun` with `recv` and `args`.
fn call_recv_fun<'scope>(
    scope: &PinScope<'scope, '_>,
    fun: Local<'_, Function>,
    recv: Local<'_, Value>,
    args: &[Local<'_, Value>],
) -> FnRet<'scope> {
    fun.call(scope, recv, args).ok_or_else(exception_already_thrown)
}

struct V8Instance<'a, 'scope, 'isolate> {
    scope: &'a mut PinScope<'scope, 'isolate>,
    replica_ctx: &'a Arc<ReplicaContext>,
    hooks: &'a HookFunctions<'scope>,
    reducer_args_buf: Local<'scope, ArrayBuffer>,
}

impl WasmInstance for V8Instance<'_, '_, '_> {
    fn extract_descriptions(&mut self) -> Result<RawModuleDef, DescribeError> {
        extract_description(self.scope, self.hooks, self.replica_ctx)
    }

    fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        self.replica_ctx
    }

    fn tx_slot(&self) -> TxSlot {
        self.scope.get_slot::<JsInstanceEnv>().unwrap().instance_env.tx.clone()
    }

    fn set_module_def(&mut self, module_def: Arc<ModuleDef>) {
        env_on_isolate_unwrap(self.scope).set_module_def(module_def);
    }

    fn call_reducer(&mut self, op: ReducerOp<'_>, budget: FunctionBudget) -> ReducerExecuteResult {
        common_call(self.scope, self.hooks, budget, op, |scope, op| {
            Ok(call_call_reducer(scope, self.hooks, op, self.reducer_args_buf)?)
        })
        .map_result(|call_result| call_result.and_then(|res| res.map_err(ExecutionError::User)))
    }

    fn call_view(&mut self, op: ViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self.scope, self.hooks, budget, op, |scope, op| {
            call_call_view(scope, self.hooks, op)
        })
    }

    fn call_view_anon(&mut self, op: AnonymousViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self.scope, self.hooks, budget, op, |scope, op| {
            call_call_view_anon(scope, self.hooks, op)
        })
    }

    fn log_traceback(&self, func_type: &str, func: &str, trap: &anyhow::Error) {
        log_traceback(self.replica_ctx, func_type, func, trap)
    }

    async fn call_procedure(
        &mut self,
        op: ProcedureOp,
        budget: FunctionBudget,
    ) -> (ProcedureExecuteResult, Option<TransactionOffset>) {
        let result = common_call(self.scope, self.hooks, budget, op, |scope, op| {
            call_call_procedure(scope, self.hooks, op)
        })
        .map_result(|call_result| {
            call_result.map_err(|e| match e {
                ExecutionError::User(e) => anyhow::Error::msg(e),
                ExecutionError::Recoverable(e) | ExecutionError::Trap(e) => e,
            })
        });
        let tx_offset = env_on_isolate_unwrap(self.scope)
            .instance_env
            .take_procedure_tx_offset();
        (result, tx_offset)
    }
}

fn common_call<'scope, R, O, F>(
    scope: &mut PinScope<'scope, '_>,
    hooks: &HookFunctions<'_>,
    budget: FunctionBudget,
    op: O,
    call: F,
) -> ExecutionResult<R, ExecutionError>
where
    O: InstanceOp,
    F: FnOnce(&mut PinTryCatch<'scope, '_, '_, '_>, O) -> Result<R, ErrorOrException<ExceptionThrown>>,
{
    // TODO(v8): Start the budget timeout and long-running logger.
    let env = env_on_isolate_unwrap(scope);

    // Start the timer.
    // We'd like this tightly around `call`.
    env.start_funcall(op.name().clone(), op.timestamp(), op.call_type());

    v8::tc_scope!(scope, scope);
    let call_result = call(scope, op).map_err(|mut e| {
        if let ErrorOrException::Exception(_) = e {
            // If we're terminating execution, don't try to check `instanceof`.
            if scope.can_continue()
                && let Some(exc) = scope.exception()
            {
                match process_thrown_exception(scope, hooks, exc) {
                    Ok(Some(err)) => return err,
                    Ok(None) => {}
                    Err(exc) => e = ErrorOrException::Exception(exc),
                }
            }
        }
        let e = e.map_exception(|exc| exc.into_error(scope)).into();
        if scope.can_continue() {
            // We can continue.
            ExecutionError::Recoverable(e)
        } else if scope.has_terminated() {
            // We can continue if we do `Isolate::cancel_terminate_execution`.
            scope.cancel_terminate_execution();
            ExecutionError::Recoverable(e)
        } else {
            // We cannot continue.
            ExecutionError::Trap(e)
        }
    });

    // Finish timings.
    let timings = env_on_isolate_unwrap(scope).finish_reducer();

    // Derive energy stats.
    let energy = energy_from_elapsed(budget, timings.total_duration);

    // Fetch the currently used heap size in V8.
    // The used size is ostensibly fairer than the total size.
    let memory_allocation = scope.get_heap_statistics().used_heap_size();

    let stats = ExecutionStats {
        energy,
        timings,
        memory_allocation,
    };
    ExecutionResult { stats, call_result }
}

/// Extracts the raw module def by running the registered `__describe_module__` hook.
fn extract_description<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hooks: &HookFunctions<'_>,
    replica_ctx: &ReplicaContext,
) -> Result<RawModuleDef, DescribeError> {
    run_describer(
        |a, b, c| log_traceback(replica_ctx, a, b, c),
        || {
            Ok(catch_exception(scope, |scope| {
                let def = call_describe_module(scope, hooks)?;
                Ok(def)
            })?)
        },
    )
}

#[cfg(test)]
mod test {
    use super::to_value::test::with_scope;
    use super::*;
    use crate::host::v8::error::{ErrorOrException, ExceptionThrown};
    use crate::host::wasm_common::module_host_actor::ReducerOp;
    use crate::host::ArgsTuple;
    use spacetimedb_lib::{ConnectionId, Identity};
    use spacetimedb_primitives::ReducerId;
    use spacetimedb_schema::reducer_name::ReducerName;

    fn with_module_catch<T>(
        code: &str,
        logic: impl for<'scope> FnOnce(
            &mut PinTryCatch<'scope, '_, '_, '_>,
            Local<v8::Object>,
        ) -> Result<T, ErrorOrException<ExceptionThrown>>,
    ) -> anyhow::Result<T> {
        with_scope(|scope| {
            Ok(catch_exception(scope, |scope| {
                let exports = eval_user_module(scope, code)?;
                let ret = logic(scope, exports)?;
                Ok(ret)
            })?)
        })
    }

    #[test]
    fn call_call_reducer_works() {
        let call = |code| {
            with_module_catch(code, |scope, exports| {
                let hooks = get_hooks(scope, exports)?.unwrap();
                let op = ReducerOp {
                    id: ReducerId(42),
                    name: &ReducerName::for_test("foobar"),
                    caller_identity: &Identity::ONE,
                    caller_connection_id: &ConnectionId::ZERO,
                    timestamp: Timestamp::from_micros_since_unix_epoch(24),
                    args: &ArgsTuple::nullary(),
                };
                let buffer = v8::ArrayBuffer::new(scope, 4096);
                Ok(call_call_reducer(scope, &hooks, op, buffer)?)
            })
        };

        // Test the trap case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({
                __describe_module__: function() {},
                __call_reducer__: function(reducer_id, sender, conn_id, timestamp, args) {
                    throw new Error("foobar");
                },
            })
        "#,
        );
        let actual = ret.expect_err("should trap").to_string().replace("\t", "    ");
        let expected = r#"
Uncaught Error: foobar
    at __call_reducer__ (spacetimedb_module:6:27)
        "#;
        assert_eq!(actual.trim(), expected.trim());

        // Test the error case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({
                __describe_module__: function() {},
                __call_reducer__: function(reducer_id, sender, conn_id, timestamp, args) {
                    return {
                        "tag": "err",
                        "value": "foobar",
                    };
                },
            })
        "#,
        );
        assert_eq!(&*ret.expect("should not trap").expect_err("should error"), "foobar");

        // Test the error case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({
                __describe_module__: function() {},
                __call_reducer__: function(reducer_id, sender, conn_id, timestamp, args) {
                    return {
                        "tag": "ok",
                        "value": {},
                    };
                },
            })
        "#,
        );
        ret.expect("should not trap").expect("should not error");
    }

    #[test]
    fn call_describe_module_works() {
        let code = r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({
                __call_reducer__: function() {},
                __describe_module__: function() {
                    return new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                },
            })
        "#;
        let raw_mod = with_module_catch(code, |scope, exports| {
            let hooks = get_hooks(scope, exports)?.unwrap();
            call_describe_module(scope, &hooks)
        })
        .map_err(|e| e.to_string());
        assert_eq!(raw_mod, Ok(RawModuleDef::V9(<_>::default())));
    }
}
