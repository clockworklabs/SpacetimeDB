//! V8 has two independent execution lanes. Non-procedure work uses the main lane and
//! is serialized through a single queue. Websocket operations enqueue and return to
//! the caller immediately. Blocking/sync callers, such as HTTP, also enqueue onto the
//! worker but do not resume the caller until their operation completes.
//!
//! Procedures use a separate bounded instance pool. A procedure call waits to check
//! out an exclusive procedure instance from the pool, and that instance is held
//! until the procedure finishes. Because procedures do not use the main lane, there
//! is no ordering guarantee between procedure work and non-procedure work, even when
//! both come from the same connection.
//!
//! Each worker owns its V8 isolate and replaces it inline after a trap or heap
//! retirement. Client-visible responses are emitted through SendWorker.
//!
//! Current V8 ModuleHost topology:
//!
//! ```text
//!                         +-------------------------------------------+
//!                         |              V8ModuleHost                 |
//!                         |                                           |
//!                         |  +-------------------------------------+  |
//!   reducers              |  | SharedJsMainInstanceManager         |  |
//!   subscriptions         |  | mpsc queue feeding a single worker  |--+----+
//!   one-off queries  ---->|  | thread                              |       |
//!                         |  +-------------------------------------+  |    v
//!                         |                                           |  +----------------------+
//!                         |                                           |  | os thread            |
//!                         |                                           |  | one V8 isolate       |
//!                         |                                           |  | inline replacement   |
//!                         |                                           |  +----------+-----------+
//!                         |                                           |             |
//!                         |                                           |             v
//!                         |                                           |       SendWorker
//!                         |                                           |
//!   procedure work        |  +-------------------------------------+  |
//!   waits here if   ----->|  | ModuleInstanceManager<JsModule>     |  |
//!   pool is full          |  | bounded procedure instance pool     |  |
//!                         |  | checkout held until procedure done  |--+----+
//!                         |  +-------------------------------------+  |    |
//!                         +-------------------------------------------+    |
//!                                                                          |
//!                              +-------------------------------------------+
//!                              |
//!                              v
//!                  +----------------------+      +----------------------+
//!                  | os thread 1          |      | os thread N          |
//!                  | one V8 isolate       |      | one V8 isolate       |
//!                  | inline replacement   |      | inline replacement   |
//!                  +----------+-----------+      +----------+-----------+
//!                             |                             |
//!                             +-------------+---------------+
//!                                           |
//!                                           v
//!                                      SendWorker
//! ```
use self::budget::energy_from_elapsed;
use self::error::{
    catch_exception, exception_already_thrown, log_traceback, ErrorOrException, ExcResult, ExceptionThrown,
    PinTryCatch, Throwable,
};
use self::ser::serialize_to_js;
use self::string::{str_from_ident, IntoJsString};
use self::syscall::{
    call_call_http_handler, call_call_procedure, call_call_reducer, call_call_view, call_call_view_anon,
    call_describe_module, get_hooks, process_thrown_exception, resolve_sys_module, FnRet, HookFunctions,
};
use super::module_common::{build_common_module_from_raw, run_describer, ModuleCommon};
use super::module_host::{
    CallHttpHandlerParams, CallProcedureParams, CallReducerParams, InstanceManagerMetrics, ModuleInfo,
    ModuleWithInstance,
};
use super::UpdateDatabaseResult;
use crate::client::{ClientActorId, MeteredUnboundedReceiver, MeteredUnboundedSender};
use crate::config::{V8Config, V8HeapPolicyConfig};
use crate::host::host_controller::CallProcedureReturn;
use crate::host::instance_env::{ChunkPool, InstanceEnv, TxSlot};
use crate::host::module_host::{
    call_identity_connected, init_database, ClientConnectedError, HttpHandlerCallError, OneOffQueryRequest, SqlCommand,
    SqlCommandResult, ViewCommand, ViewCommandMetric, ViewCommandResult,
};
use crate::host::scheduler::{CallScheduledFunctionResult, ScheduledFunctionParams};
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    AnonymousViewOp, DescribeError, ExecutionError, ExecutionResult, ExecutionStats, ExecutionTimings,
    HttpHandlerExecuteResult, HttpHandlerOp, InstanceCommon, InstanceOp, ProcedureExecuteResult, ProcedureOp,
    ReducerExecuteResult, ReducerOp, ViewExecuteResult, ViewOp, WasmInstance,
};
use crate::host::wasm_common::{RowIters, TimingSpanSet};
use crate::host::{ModuleHost, ReducerCallError, ReducerCallResult, Scheduler};
use crate::messages::control_db::HostType;
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_manager::TransactionOffset;
use crate::util::jobs::{AllocatedJobCore, CorePinner, LoadBalanceOnDropGuard};
use crate::worker_metrics::WORKER_METRICS;
use core::str;
use futures::FutureExt;
use itertools::Either;
use prometheus::{IntCounter, IntGauge};
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
use std::num::NonZeroUsize;
use std::os::raw::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use v8::script_compiler::{compile_module, Source};
use v8::{
    scope_with_context, ArrayBuffer, Context, Function, Global, Isolate, Local, MapFnTo, OwnedIsolate, PinScope,
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
    config: V8Config,
}

impl Default for V8Runtime {
    fn default() -> Self {
        Self::new(V8Config::default())
    }
}

impl V8Runtime {
    pub fn new(config: V8Config) -> Self {
        Self {
            config: config.normalized(),
        }
    }

    pub async fn make_actor(
        &self,
        mcc: ModuleCreationContext,
        program_bytes: &[u8],
        core: AllocatedJobCore,
    ) -> anyhow::Result<ModuleWithInstance> {
        V8_RUNTIME_GLOBAL
            .make_actor(mcc, program_bytes, core, self.config)
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
const REDUCER_ARGS_BUFFER_SIZE: usize = 4_096;
const JS_PROCEDURE_INSTANCE_QUEUE_CAPACITY: usize = 1;
pub(crate) const V8_WORKER_KIND_MAIN: &str = "main";

#[derive(Copy, Clone)]
enum JsWorkerKind {
    Main,
    Procedure,
}

impl JsWorkerKind {
    const fn checks_heap(self) -> bool {
        matches!(self, Self::Main)
    }
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
        config: V8Config,
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
        let metrics = InstanceManagerMetrics::new(HostType::Js, mcc.replica_ctx.database_identity);
        let mcc = Either::Right(mcc);
        let load_balance_guard = Arc::new(core.guard);
        let core_pinner = core.pinner;
        let heap_policy = config.heap_policy;
        let (common, init_inst) = spawn_main_instance_worker(
            program.clone(),
            mcc,
            load_balance_guard.clone(),
            core_pinner.clone(),
            heap_policy,
            metrics.clone(),
        )
        .await?;
        let module = JsModule {
            common,
            program,
            load_balance_guard,
            core_pinner,
            procedure_instance_pool_size: config.procedure_instance_pool_size,
            heap_policy: config.heap_policy,
            metrics,
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
    procedure_instance_pool_size: NonZeroUsize,
    heap_policy: V8HeapPolicyConfig,
    metrics: InstanceManagerMetrics,
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

    pub(in crate::host) fn metrics(&self) -> InstanceManagerMetrics {
        self.metrics.clone()
    }

    pub(in crate::host) fn procedure_instance_pool_size(&self) -> NonZeroUsize {
        self.procedure_instance_pool_size
    }

    async fn create_procedure_instance(&self) -> JsProcedureInstance {
        let program = self.program.clone();
        let common = self.common.clone();
        let load_balance_guard = self.load_balance_guard.clone();
        let core_pinner = self.core_pinner.clone();
        let heap_policy = self.heap_policy;
        let metrics = self.metrics.clone();

        // This has to be done in a blocking context because of `blocking_recv`.
        let (_, instance) = spawn_procedure_instance_worker(
            program,
            Either::Left(common),
            load_balance_guard,
            core_pinner,
            heap_policy,
            metrics,
        )
        .await
        .expect("`spawn_procedure_instance_worker` should succeed when passed `ModuleCommon`");
        instance
    }

    pub async fn create_instance(&self) -> JsProcedureInstance {
        self.create_procedure_instance().await
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

/// The environment bound to a JS worker isolate.
struct JsInstanceEnv {
    instance_env: InstanceEnv,
    module_def: Option<Arc<ModuleDef>>,
    /// Last used-heap sample captured by the worker's periodic heap checks.
    cached_used_heap_size: usize,

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
            cached_used_heap_size: 0,
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

    /// Signal to this `WasmInstanceEnv` that a reducer/view/procedure call is over.
    /// This resets all of the state associated to a single function call,
    /// and returns instrumentation records.
    fn finish_funcall(&mut self) -> ExecutionTimings {
        let total_duration = self.reducer_start().elapsed();
        let func_name = self.log_record_function().unwrap_or("<unknown>").to_owned();

        let leftover_iters = self.iters.len();
        if leftover_iters > 0 {
            log::warn!("force-clearing {leftover_iters} row iterator(s) left open by JS call `{func_name}`");
            self.iters.clear();
        }

        let leftover_timing_spans = self.timing_spans.len();
        if leftover_timing_spans > 0 {
            log::warn!("force-clearing {leftover_timing_spans} timing span(s) left open by JS call `{func_name}`");
            self.timing_spans.clear();
        }

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        }
    }

    /// Refresh the cached heap usage after an explicit V8 heap sample.
    fn set_cached_used_heap_size(&mut self, bytes: usize) {
        self.cached_used_heap_size = bytes;
    }

    /// Return the last heap sample without forcing a fresh V8 query.
    fn cached_used_heap_size(&self) -> usize {
        self.cached_used_heap_size
    }

    fn set_module_def(&mut self, module_def: Arc<ModuleDef>) {
        self.module_def = Some(module_def);
    }

    fn module_def(&self) -> Option<Arc<ModuleDef>> {
        self.module_def.clone()
    }
}

/// The main instance for a [`JsModule`].
///
/// The actual work happens in a worker thread,
/// which the instance communicates with through channels.
///
/// This handle is cloneable and shared by callers. Requests are queued FIFO
/// on the backing worker queue so the next reducer can start immediately after
/// the previous one finishes, without waiting for an outer task to hand the
/// instance back.
///
/// When the last handle is dropped, the channels will hang up,
/// which will cause the worker's loop to terminate and cleanup the isolate
/// and friends.
#[derive(Clone)]
pub struct JsMainInstance {
    tx: MeteredUnboundedSender<JsMainWorkerRequest>,
}

/// A procedure instance for a [`JsModule`].
///
/// Procedure instances are checked out exclusively from the procedure pool and
/// only execute procedure-style requests.
pub struct JsProcedureInstance {
    tx: mpsc::Sender<JsProcedureWorkerRequest>,
}

impl JsMainInstance {
    async fn request<R: JsMainRequest>(&self, request: R) -> R::Response {
        send_js_unbounded_request(R::CTX, &self.tx, |reply_tx| request.into_worker_request(reply_tx)).await
    }

    async fn send_detached_request(&self, ctx: &'static str, request: JsMainWorkerRequest) {
        if self.tx.send(request).is_err() {
            panic!("JS worker exited before accepting `{ctx}`");
        }
    }

    pub async fn update_database(
        &self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        self.request(UpdateDatabaseRequest {
            program,
            old_module_info,
            policy,
        })
        .await
    }

    pub async fn call_reducer(&self, params: CallReducerParams) -> ReducerCallResult {
        self.request(CallReducerRequest { params }).await
    }

    pub(in crate::host) async fn call_scheduled_reducer(
        &self,
        params: ScheduledFunctionParams,
    ) -> CallScheduledFunctionResult {
        self.request(ScheduledReducerRequest { params }).await
    }

    pub(in crate::host) async fn enqueue_reducer(&self, params: CallReducerParams, on_panic: JsFatalHook) {
        self.send_detached_request(
            "call_reducer",
            JsMainWorkerRequest::CallReducerDetached { params, on_panic },
        )
        .await
    }

    pub async fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.request(ClearAllClientsRequest).await
    }

    pub async fn call_identity_connected(
        &self,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        self.request(CallIdentityConnectedRequest {
            caller_auth,
            caller_connection_id,
        })
        .await
    }

    pub async fn call_identity_disconnected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ReducerCallError> {
        self.request(CallIdentityDisconnectedRequest {
            caller_identity,
            caller_connection_id,
        })
        .await
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) -> Result<(), ReducerCallError> {
        self.request(DisconnectClientRequest { client_id }).await
    }

    pub async fn init_database(&self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        self.request(InitDatabaseRequest { program }).await
    }

    pub async fn call_view(&self, cmd: ViewCommand) -> ViewCommandResult {
        self.request(CallViewRequest { cmd }).await
    }

    pub(in crate::host) async fn enqueue_call_view(
        &self,
        cmd: ViewCommand,
        metric: ViewCommandMetric,
        on_panic: JsFatalHook,
    ) {
        self.send_detached_request(
            "call_view",
            JsMainWorkerRequest::CallViewDetached { cmd, metric, on_panic },
        )
        .await
    }

    pub(in crate::host) async fn call_sql(&self, cmd: SqlCommand) -> SqlCommandResult {
        self.request(CallSqlRequest { cmd }).await
    }

    pub(in crate::host) async fn enqueue_one_off_query(&self, request: OneOffQueryRequest, on_panic: JsFatalHook) {
        let ctx = request.label();
        self.send_detached_request(ctx, JsMainWorkerRequest::OneOffQueryDetached { request, on_panic })
            .await
    }
}

trait JsMainRequest {
    type Response;

    const CTX: &'static str;

    fn into_worker_request(self, reply_tx: JsReplyTx<Self::Response>) -> JsMainWorkerRequest;
}

macro_rules! js_main_request {
    (
        $request:ident {
            $($field:ident: $field_ty:ty),* $(,)?
        } => $ctx:literal, $response:ty, $variant:ident
    ) => {
        struct $request {
            $($field: $field_ty),*
        }

        impl JsMainRequest for $request {
            type Response = $response;

            const CTX: &'static str = $ctx;

            fn into_worker_request(self, reply_tx: JsReplyTx<Self::Response>) -> JsMainWorkerRequest {
                JsMainWorkerRequest::$variant {
                    reply_tx,
                    $($field: self.$field),*
                }
            }
        }
    };
    (
        $request:ident => $ctx:literal, $response:ty, $variant:ident
    ) => {
        struct $request;

        impl JsMainRequest for $request {
            type Response = $response;

            const CTX: &'static str = $ctx;

            fn into_worker_request(self, reply_tx: JsReplyTx<Self::Response>) -> JsMainWorkerRequest {
                JsMainWorkerRequest::$variant(reply_tx)
            }
        }
    };
}

js_main_request! {
    UpdateDatabaseRequest {
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    } => "update_database", anyhow::Result<UpdateDatabaseResult>, UpdateDatabase
}

js_main_request! {
    CallReducerRequest {
        params: CallReducerParams,
    } => "call_reducer", ReducerCallResult, CallReducer
}

js_main_request! {
    ScheduledReducerRequest {
        params: ScheduledFunctionParams,
    } => "scheduled_reducer", CallScheduledFunctionResult, ScheduledReducer
}

js_main_request! {
    ClearAllClientsRequest => "clear_all_clients", anyhow::Result<()>, ClearAllClients
}

js_main_request! {
    CallIdentityConnectedRequest {
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    } => "call_identity_connected", Result<(), ClientConnectedError>, CallIdentityConnected
}

js_main_request! {
    CallIdentityDisconnectedRequest {
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    } => "call_identity_disconnected", Result<(), ReducerCallError>, CallIdentityDisconnected
}

js_main_request! {
    DisconnectClientRequest {
        client_id: ClientActorId,
    } => "disconnect_client", Result<(), ReducerCallError>, DisconnectClient
}

js_main_request! {
    InitDatabaseRequest {
        program: Program,
    } => "init_database", anyhow::Result<Option<ReducerCallResult>>, InitDatabase
}

js_main_request! {
    CallViewRequest {
        cmd: ViewCommand,
    } => "call_view", ViewCommandResult, CallView
}

js_main_request! {
    CallSqlRequest {
        cmd: SqlCommand,
    } => "call_sql", SqlCommandResult, CallSql
}

impl JsProcedureInstance {
    pub(in crate::host) fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    async fn send_request<T>(
        &self,
        ctx: &'static str,
        request: impl FnOnce(JsReplyTx<T>) -> JsProcedureWorkerRequest,
    ) -> T {
        send_js_request(ctx, &self.tx, request).await
    }

    pub async fn call_procedure(&self, params: CallProcedureParams) -> CallProcedureReturn {
        self.send_request("call_procedure", |reply_tx| JsProcedureWorkerRequest::CallProcedure {
            reply_tx,
            params,
        })
        .await
    }

    pub async fn call_http_handler(
        &self,
        params: CallHttpHandlerParams,
    ) -> Result<(spacetimedb_lib::http::Response, bytes::Bytes), HttpHandlerCallError> {
        self.send_request("call_http_handler", |reply_tx| {
            JsProcedureWorkerRequest::CallHttpHandler { reply_tx, params }
        })
        .await
    }

    pub(in crate::host) async fn enqueue_procedure(&self, params: CallProcedureParams) -> JsProcedureCall {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(JsProcedureWorkerRequest::CallProcedure { reply_tx, params })
            .await
            .is_err()
        {
            panic!("JS worker exited before accepting `call_procedure`");
        }
        JsProcedureCall { reply_rx }
    }

    pub(in crate::host) async fn call_scheduled_procedure(
        &self,
        params: ScheduledFunctionParams,
    ) -> CallScheduledFunctionResult {
        self.send_request("scheduled_procedure", |reply_tx| {
            JsProcedureWorkerRequest::ScheduledProcedure { reply_tx, params }
        })
        .await
    }
}

async fn send_js_request<Req, T>(
    ctx: &'static str,
    tx: &mpsc::Sender<Req>,
    request: impl FnOnce(JsReplyTx<T>) -> Req,
) -> T
where
    Req: Send + 'static,
{
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx.send(request(reply_tx)).await.is_err() {
        panic!("JS worker exited before accepting `{ctx}`");
    }
    match reply_rx.await {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => panic::resume_unwind(panic),
        Err(_) => panic!("JS worker exited before replying to `{ctx}`"),
    }
}

async fn send_js_unbounded_request<T>(
    ctx: &'static str,
    tx: &MeteredUnboundedSender<JsMainWorkerRequest>,
    request: impl FnOnce(JsReplyTx<T>) -> JsMainWorkerRequest,
) -> T {
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx.send(request(reply_tx)).is_err() {
        panic!("JS worker exited before accepting `{ctx}`");
    }
    match reply_rx.await {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => panic::resume_unwind(panic),
        Err(_) => panic!("JS worker exited before replying to `{ctx}`"),
    }
}

type JsPanicPayload = Box<dyn std::any::Any + Send + 'static>;
type JsReply<T> = Result<T, JsPanicPayload>;
type JsReplyTx<T> = oneshot::Sender<JsReply<T>>;
pub(in crate::host) type JsFatalHook = Arc<dyn Fn() + Send + Sync + 'static>;

pub(in crate::host) struct JsProcedureCall {
    reply_rx: oneshot::Receiver<JsReply<CallProcedureReturn>>,
}

pub(in crate::host) enum JsProcedureCallCompletion {
    Completed(CallProcedureReturn),
    Panicked,
    WorkerExited,
}

impl JsProcedureCall {
    pub(in crate::host) async fn receive(self) -> JsProcedureCallCompletion {
        match self.reply_rx.await {
            Ok(Ok(ret)) => JsProcedureCallCompletion::Completed(ret),
            Ok(Err(_panic)) => JsProcedureCallCompletion::Panicked,
            Err(_) => JsProcedureCallCompletion::WorkerExited,
        }
    }
}

/// Requests sent to the main JS worker thread.
///
/// Most variants carry a `reply_tx` because the worker thread owns the isolate,
/// executes the request there, and then has to send the typed result back to
/// the async caller.
enum JsMainWorkerRequest {
    /// See [`JsMainInstance::update_database`].
    UpdateDatabase {
        reply_tx: JsReplyTx<anyhow::Result<UpdateDatabaseResult>>,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    },
    /// See [`JsMainInstance::call_reducer`].
    CallReducer {
        reply_tx: JsReplyTx<ReducerCallResult>,
        params: CallReducerParams,
    },
    /// See [`JsMainInstance::enqueue_reducer`].
    CallReducerDetached {
        params: CallReducerParams,
        on_panic: JsFatalHook,
    },
    /// See [`JsMainInstance::call_scheduled_reducer`].
    ScheduledReducer {
        reply_tx: JsReplyTx<CallScheduledFunctionResult>,
        params: ScheduledFunctionParams,
    },
    /// See [`JsMainInstance::call_view`].
    CallView {
        reply_tx: JsReplyTx<ViewCommandResult>,
        cmd: ViewCommand,
    },
    /// See [`JsMainInstance::enqueue_call_view`].
    CallViewDetached {
        cmd: ViewCommand,
        metric: ViewCommandMetric,
        on_panic: JsFatalHook,
    },
    /// See [`JsMainInstance::call_sql`].
    CallSql {
        reply_tx: JsReplyTx<SqlCommandResult>,
        cmd: SqlCommand,
    },
    /// See [`JsMainInstance::enqueue_one_off_query`].
    OneOffQueryDetached {
        request: OneOffQueryRequest,
        on_panic: JsFatalHook,
    },
    /// See [`JsInstance::clear_all_clients`].
    ClearAllClients(JsReplyTx<anyhow::Result<()>>),
    /// See [`JsMainInstance::call_identity_connected`].
    CallIdentityConnected {
        reply_tx: JsReplyTx<Result<(), ClientConnectedError>>,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    },
    /// See [`JsMainInstance::call_identity_disconnected`].
    CallIdentityDisconnected {
        reply_tx: JsReplyTx<Result<(), ReducerCallError>>,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    },
    /// See [`JsMainInstance::disconnect_client`].
    DisconnectClient {
        reply_tx: JsReplyTx<Result<(), ReducerCallError>>,
        client_id: ClientActorId,
    },
    /// See [`JsMainInstance::init_database`].
    InitDatabase {
        reply_tx: JsReplyTx<anyhow::Result<Option<ReducerCallResult>>>,
        program: Program,
    },
}

/// Requests sent to a procedure JS worker thread.
enum JsProcedureWorkerRequest {
    /// See [`JsProcedureInstance::call_procedure`].
    CallProcedure {
        reply_tx: JsReplyTx<CallProcedureReturn>,
        params: CallProcedureParams,
    },
    /// See [`JsProcedureInstance::call_scheduled_procedure`].
    ScheduledProcedure {
        reply_tx: JsReplyTx<CallScheduledFunctionResult>,
        params: ScheduledFunctionParams,
    },
    /// See [`JsInstance::call_http_handler`].
    CallHttpHandler {
        reply_tx: JsReplyTx<Result<(spacetimedb_lib::http::Response, bytes::Bytes), HttpHandlerCallError>>,
        params: CallHttpHandlerParams,
    },
}

static_assert_size!(CallReducerParams, 192);

fn send_worker_reply<T>(ctx: &str, reply_tx: JsReplyTx<T>, value: T) {
    if reply_tx.send(Ok(value)).is_err() {
        log::error!("should have receiver for `{ctx}` response");
    }
}

fn send_worker_panic_reply<T>(ctx: &str, reply_tx: JsReplyTx<T>, panic: JsPanicPayload) {
    if reply_tx.send(Err(panic)).is_err() {
        log::error!("should have receiver for `{ctx}` response");
    }
}

enum WorkerRequestOutcome {
    Continue,
    RecreateInstance,
    Fatal,
}

impl WorkerRequestOutcome {
    fn recreate_instance(self) -> Self {
        match self {
            Self::Continue | Self::RecreateInstance => Self::RecreateInstance,
            Self::Fatal => Self::Fatal,
        }
    }
}

fn handle_worker_request<T: 'static>(
    ctx: &'static str,
    reply_tx: JsReplyTx<T>,
    f: impl FnOnce() -> (T, bool),
) -> WorkerRequestOutcome {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok((value, recreate_instance)) => {
            send_worker_reply(ctx, reply_tx, value);
            if recreate_instance {
                WorkerRequestOutcome::RecreateInstance
            } else {
                WorkerRequestOutcome::Continue
            }
        }
        Err(panic) => {
            send_worker_panic_reply(ctx, reply_tx, panic);
            WorkerRequestOutcome::Fatal
        }
    }
}

fn handle_detached_worker_request(
    ctx: &'static str,
    on_panic: JsFatalHook,
    f: impl FnOnce() -> bool,
) -> WorkerRequestOutcome {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(recreate_instance) => {
            if recreate_instance {
                WorkerRequestOutcome::RecreateInstance
            } else {
                WorkerRequestOutcome::Continue
            }
        }
        Err(_) => {
            log::warn!("detached JS worker request `{ctx}` panicked");
            on_panic();
            WorkerRequestOutcome::Fatal
        }
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
    last_observed: V8HeapSnapshot,
}

#[derive(Clone, Copy, Default)]
struct V8HeapSnapshot {
    total_heap_size_bytes: i64,
    total_physical_size_bytes: i64,
    used_global_handles_size_bytes: i64,
    used_heap_size_bytes: i64,
    heap_size_limit_bytes: i64,
    external_memory_bytes: i64,
    native_contexts: i64,
    detached_contexts: i64,
}

impl V8HeapSnapshot {
    fn from_stats(stats: &v8::HeapStatistics) -> Self {
        Self {
            total_heap_size_bytes: stats.total_heap_size() as i64,
            total_physical_size_bytes: stats.total_physical_size() as i64,
            used_global_handles_size_bytes: stats.used_global_handles_size() as i64,
            used_heap_size_bytes: stats.used_heap_size() as i64,
            heap_size_limit_bytes: stats.heap_size_limit() as i64,
            external_memory_bytes: stats.external_memory() as i64,
            native_contexts: stats.number_of_native_contexts() as i64,
            detached_contexts: stats.number_of_detached_contexts() as i64,
        }
    }
}

impl V8HeapMetrics {
    fn new(database_identity: &Identity) -> Self {
        Self {
            total_heap_size_bytes: WORKER_METRICS
                .v8_total_heap_size_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            total_physical_size_bytes: WORKER_METRICS
                .v8_total_physical_size_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            used_global_handles_size_bytes: WORKER_METRICS
                .v8_used_global_handles_size_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            used_heap_size_bytes: WORKER_METRICS
                .v8_used_heap_size_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            heap_size_limit_bytes: WORKER_METRICS
                .v8_heap_size_limit_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            external_memory_bytes: WORKER_METRICS
                .v8_external_memory_bytes
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            native_contexts: WORKER_METRICS
                .v8_native_contexts
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            detached_contexts: WORKER_METRICS
                .v8_detached_contexts
                .with_label_values(database_identity, V8_WORKER_KIND_MAIN),
            last_observed: V8HeapSnapshot::default(),
        }
    }

    fn adjust_by(&self, delta: V8HeapSnapshot) {
        adjust_gauge(&self.total_heap_size_bytes, delta.total_heap_size_bytes);
        adjust_gauge(&self.total_physical_size_bytes, delta.total_physical_size_bytes);
        adjust_gauge(
            &self.used_global_handles_size_bytes,
            delta.used_global_handles_size_bytes,
        );
        adjust_gauge(&self.used_heap_size_bytes, delta.used_heap_size_bytes);
        adjust_gauge(&self.heap_size_limit_bytes, delta.heap_size_limit_bytes);
        adjust_gauge(&self.external_memory_bytes, delta.external_memory_bytes);
        adjust_gauge(&self.native_contexts, delta.native_contexts);
        adjust_gauge(&self.detached_contexts, delta.detached_contexts);
    }

    fn observe(&mut self, stats: &v8::HeapStatistics) {
        let next = V8HeapSnapshot::from_stats(stats);
        self.adjust_by(V8HeapSnapshot {
            total_heap_size_bytes: next.total_heap_size_bytes - self.last_observed.total_heap_size_bytes,
            total_physical_size_bytes: next.total_physical_size_bytes - self.last_observed.total_physical_size_bytes,
            used_global_handles_size_bytes: next.used_global_handles_size_bytes
                - self.last_observed.used_global_handles_size_bytes,
            used_heap_size_bytes: next.used_heap_size_bytes - self.last_observed.used_heap_size_bytes,
            heap_size_limit_bytes: next.heap_size_limit_bytes - self.last_observed.heap_size_limit_bytes,
            external_memory_bytes: next.external_memory_bytes - self.last_observed.external_memory_bytes,
            native_contexts: next.native_contexts - self.last_observed.native_contexts,
            detached_contexts: next.detached_contexts - self.last_observed.detached_contexts,
        });
        self.last_observed = next;
    }
}

impl Drop for V8HeapMetrics {
    fn drop(&mut self) {
        self.adjust_by(V8HeapSnapshot {
            total_heap_size_bytes: -self.last_observed.total_heap_size_bytes,
            total_physical_size_bytes: -self.last_observed.total_physical_size_bytes,
            used_global_handles_size_bytes: -self.last_observed.used_global_handles_size_bytes,
            used_heap_size_bytes: -self.last_observed.used_heap_size_bytes,
            heap_size_limit_bytes: -self.last_observed.heap_size_limit_bytes,
            external_memory_bytes: -self.last_observed.external_memory_bytes,
            native_contexts: -self.last_observed.native_contexts,
            detached_contexts: -self.last_observed.detached_contexts,
        });
    }
}

fn adjust_gauge(gauge: &IntGauge, delta: i64) {
    if delta > 0 {
        gauge.add(delta);
    } else if delta < 0 {
        gauge.sub(-delta);
    }
}

fn sample_heap_stats(scope: &mut PinScope<'_, '_>, metrics: &mut V8HeapMetrics) -> v8::HeapStatistics {
    // Whenever we sample heap statistics, we cache them on the isolate so that
    // the per-call execution stats can avoid querying them on each invocation.
    let stats = scope.get_heap_statistics();
    env_on_isolate_unwrap(scope).set_cached_used_heap_size(stats.used_heap_size());
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
    metrics: &mut V8HeapMetrics,
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

/// Performs some of the shared startup work for JS worker isolates.
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
    let params = v8::CreateParams::default().heap_limits(0, heap_policy.heap_limit_bytes);
    let mut isolate = Isolate::new(params);
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 1024);
    isolate
}

/// Run the closure `f` with `callback` set as `scope`'s near-heap-limit callback.
///
/// Upon return, the callback will be unregistered and the heap limit will be set back down to as
/// close to `reset_heap_limit` as the runtime deems reasonable.
fn with_near_heap_limit_callback<I, Cb, R, F>(scope: &mut I, reset_heap_limit: usize, mut callback: Cb, f: F) -> R
where
    Cb: FnMut(usize, usize) -> usize,
    I: AsMut<Isolate>,
    F: FnOnce(&mut I) -> R,
{
    unsafe extern "C" fn callback_wrapper<F>(
        data: *mut c_void,
        current_heap_limit: usize,
        initial_heap_limit: usize,
    ) -> usize
    where
        F: FnMut(usize, usize) -> usize,
    {
        let callback = data.cast::<F>();
        unsafe { (*callback)(current_heap_limit, initial_heap_limit) }
    }

    let data = std::ptr::from_mut(&mut callback).cast::<c_void>();
    let raw_callback: v8::NearHeapLimitCallback = callback_wrapper::<Cb>;

    scope.as_mut().add_near_heap_limit_callback(raw_callback, data);

    // Immediately set up a guard that will remove the callback when this scope exits, because
    // `data` points to a stack-allocated object and it cannot be allowed to hang around after
    // this stack frame exits.
    let mut guard = scopeguard::guard(scope, |isolate| {
        isolate
            .as_mut()
            .remove_near_heap_limit_callback(raw_callback, reset_heap_limit)
    });

    f(&mut guard)
}

async fn spawn_main_instance_worker(
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContext>,
    load_balance_guard: Arc<LoadBalanceOnDropGuard>,
    core_pinner: CorePinner,
    heap_policy: V8HeapPolicyConfig,
    metrics: InstanceManagerMetrics,
) -> anyhow::Result<(ModuleCommon, JsMainInstance)> {
    spawn_instance_worker::<MainJsWorker>(
        program,
        module_or_mcc,
        load_balance_guard,
        core_pinner,
        heap_policy,
        metrics,
    )
    .await
}

async fn spawn_procedure_instance_worker(
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContext>,
    load_balance_guard: Arc<LoadBalanceOnDropGuard>,
    core_pinner: CorePinner,
    heap_policy: V8HeapPolicyConfig,
    metrics: InstanceManagerMetrics,
) -> anyhow::Result<(ModuleCommon, JsProcedureInstance)> {
    spawn_instance_worker::<ProcedureJsWorker>(
        program,
        module_or_mcc,
        load_balance_guard,
        core_pinner,
        heap_policy,
        metrics,
    )
    .await
}

struct MainJsWorker;

struct ProcedureJsWorker;

trait JsWorkerSpec {
    type Request: Send + 'static;
    type Instance;
    type Sender: Send + 'static;
    type Receiver: Send + 'static;

    const KIND: JsWorkerKind;

    fn channel(database_identity: &Identity) -> (Self::Sender, Self::Receiver);

    fn make_instance(tx: Self::Sender) -> Self::Instance;

    fn blocking_recv(rx: &mut Self::Receiver) -> Option<Self::Request>;

    fn handle_request(
        request: Self::Request,
        instance_common: &mut InstanceCommon,
        inst: &mut V8Instance<'_, '_, '_>,
        module_common: &ModuleCommon,
        replica_ctx: &Arc<ReplicaContext>,
    ) -> WorkerRequestOutcome;
}

impl JsWorkerSpec for MainJsWorker {
    type Request = JsMainWorkerRequest;
    type Instance = JsMainInstance;
    type Sender = MeteredUnboundedSender<Self::Request>;
    type Receiver = MeteredUnboundedReceiver<Self::Request>;

    const KIND: JsWorkerKind = JsWorkerKind::Main;

    fn channel(database_identity: &Identity) -> (Self::Sender, Self::Receiver) {
        let queue_length = WORKER_METRICS
            .v8_request_queue_length
            .with_label_values(database_identity);
        let (tx, rx) = mpsc::unbounded_channel();
        (
            MeteredUnboundedSender::with_gauge(tx, queue_length.clone()),
            MeteredUnboundedReceiver::with_gauge(rx, queue_length),
        )
    }

    fn make_instance(tx: Self::Sender) -> Self::Instance {
        JsMainInstance { tx }
    }

    fn blocking_recv(rx: &mut Self::Receiver) -> Option<Self::Request> {
        rx.blocking_recv()
    }

    fn handle_request(
        request: Self::Request,
        instance_common: &mut InstanceCommon,
        inst: &mut V8Instance<'_, '_, '_>,
        module_common: &ModuleCommon,
        replica_ctx: &Arc<ReplicaContext>,
    ) -> WorkerRequestOutcome {
        handle_main_worker_request(request, instance_common, inst, module_common, replica_ctx)
    }
}

impl JsWorkerSpec for ProcedureJsWorker {
    type Request = JsProcedureWorkerRequest;
    type Instance = JsProcedureInstance;
    type Sender = mpsc::Sender<Self::Request>;
    type Receiver = mpsc::Receiver<Self::Request>;

    const KIND: JsWorkerKind = JsWorkerKind::Procedure;

    fn channel(_database_identity: &Identity) -> (Self::Sender, Self::Receiver) {
        mpsc::channel(JS_PROCEDURE_INSTANCE_QUEUE_CAPACITY)
    }

    fn make_instance(tx: Self::Sender) -> Self::Instance {
        JsProcedureInstance { tx }
    }

    fn blocking_recv(rx: &mut Self::Receiver) -> Option<Self::Request> {
        rx.blocking_recv()
    }

    fn handle_request(
        request: Self::Request,
        instance_common: &mut InstanceCommon,
        inst: &mut V8Instance<'_, '_, '_>,
        _module_common: &ModuleCommon,
        _replica_ctx: &Arc<ReplicaContext>,
    ) -> WorkerRequestOutcome {
        handle_procedure_worker_request(request, instance_common, inst)
    }
}

fn handle_main_worker_request(
    request: JsMainWorkerRequest,
    instance_common: &mut InstanceCommon,
    inst: &mut V8Instance<'_, '_, '_>,
    module_common: &ModuleCommon,
    replica_ctx: &Arc<ReplicaContext>,
) -> WorkerRequestOutcome {
    let info = module_common.info();

    match request {
        JsMainWorkerRequest::UpdateDatabase {
            reply_tx,
            program,
            old_module_info,
            policy,
        } => handle_worker_request("update_database", reply_tx, || {
            let res = instance_common.update_database(program, old_module_info, policy, inst);
            (res, false)
        }),
        JsMainWorkerRequest::CallReducer { reply_tx, params } => {
            handle_worker_request("call_reducer", reply_tx, || {
                let mut call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
                let (res, trapped) = call_reducer(None, params);
                (res, trapped)
            })
        }
        JsMainWorkerRequest::CallReducerDetached { params, on_panic } => {
            handle_detached_worker_request("call_reducer", on_panic, || {
                let mut call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
                let (_res, trapped) = call_reducer(None, params);
                trapped
            })
        }
        JsMainWorkerRequest::ScheduledReducer { reply_tx, params } => {
            handle_worker_request("scheduled_reducer", reply_tx, || {
                let (res, trapped) = instance_common
                    .call_scheduled_function(params, inst)
                    .now_or_never()
                    .expect("our call_scheduled_function implementation is not actually async");
                (res, trapped)
            })
        }
        JsMainWorkerRequest::CallView { reply_tx, cmd } => handle_worker_request("call_view", reply_tx, || {
            let (res, trapped) = instance_common.handle_cmd(cmd, inst);
            (res, trapped)
        }),
        JsMainWorkerRequest::CallViewDetached { cmd, metric, on_panic } => {
            handle_detached_worker_request("call_view", on_panic, || {
                let (_, trapped) = instance_common.handle_cmd(cmd, inst);
                ModuleHost::record_view_command_round_trip(&module_common.info(), metric);
                trapped
            })
        }
        JsMainWorkerRequest::CallSql { reply_tx, cmd } => handle_worker_request("call_sql", reply_tx, || {
            let (res, trapped) = instance_common.handle_sql_cmd(cmd, inst);
            (res, trapped)
        }),
        JsMainWorkerRequest::OneOffQueryDetached { request, on_panic } => {
            let label = request.label();
            handle_detached_worker_request(label, on_panic, || {
                let timer = request.timer();
                let res = request.run();
                if let Err(err) = &res {
                    log::warn!("detached one-off query failed: {err:#}");
                }
                ModuleHost::record_one_off_query_round_trip(&module_common.info(), timer);
                false
            })
        }
        JsMainWorkerRequest::ClearAllClients(reply_tx) => handle_worker_request("clear_all_clients", reply_tx, || {
            let res = instance_common.clear_all_clients();
            (res, false)
        }),
        JsMainWorkerRequest::CallIdentityConnected {
            reply_tx,
            caller_auth,
            caller_connection_id,
        } => handle_worker_request("call_identity_connected", reply_tx, || {
            let call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
            let mut trapped = false;
            let res = call_identity_connected(caller_auth, caller_connection_id, &info, call_reducer, &mut trapped);
            (res, trapped)
        }),
        JsMainWorkerRequest::CallIdentityDisconnected {
            reply_tx,
            caller_identity,
            caller_connection_id,
        } => handle_worker_request("call_identity_disconnected", reply_tx, || {
            let call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
            let mut trapped = false;
            let res = ModuleHost::call_identity_disconnected_inner(
                caller_identity,
                caller_connection_id,
                &info,
                call_reducer,
                &mut trapped,
            );
            (res, trapped)
        }),
        JsMainWorkerRequest::DisconnectClient { reply_tx, client_id } => {
            handle_worker_request("disconnect_client", reply_tx, || {
                let call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
                let mut trapped = false;
                let res = ModuleHost::disconnect_client_inner(client_id, &info, call_reducer, &mut trapped);
                (res, trapped)
            })
        }
        JsMainWorkerRequest::InitDatabase { reply_tx, program } => {
            handle_worker_request("init_database", reply_tx, || {
                let call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, inst);
                let (res, trapped): (Result<Option<ReducerCallResult>, anyhow::Error>, bool) =
                    init_database(replica_ctx, &info.module_def, program, call_reducer);
                (res, trapped)
            })
        }
    }
}

fn handle_procedure_worker_request(
    request: JsProcedureWorkerRequest,
    instance_common: &mut InstanceCommon,
    inst: &mut V8Instance<'_, '_, '_>,
) -> WorkerRequestOutcome {
    match request {
        JsProcedureWorkerRequest::CallProcedure { reply_tx, params } => {
            handle_worker_request("call_procedure", reply_tx, || {
                let (res, trapped) = instance_common
                    .call_procedure(params, inst)
                    .now_or_never()
                    .expect("our call_procedure implementation is not actually async");
                (res, trapped)
            })
        }
        JsProcedureWorkerRequest::CallHttpHandler { reply_tx, params } => {
            handle_worker_request("call_http_handler", reply_tx, || {
                let (res, trapped) = instance_common
                    .call_http_handler(params, inst)
                    .now_or_never()
                    .expect("our call_http_handler implementation is not actually async");
                (res, trapped)
            })
        }
        JsProcedureWorkerRequest::ScheduledProcedure { reply_tx, params } => {
            handle_worker_request("scheduled_procedure", reply_tx, || {
                let (res, trapped) = instance_common
                    .call_scheduled_function(params, inst)
                    .now_or_never()
                    .expect("our call_scheduled_function implementation is not actually async");
                (res, trapped)
            })
        }
    }
}

const THREAD_NAME_DATABASE_ID_SUFFIX_LEN: usize = 8;

fn js_main_worker_thread_name(database_identity: Identity) -> String {
    let hex = database_identity.to_hex();
    // We use the tail of the identity to avoid the common structured prefix.
    let suffix = &hex.as_str()[hex.as_str().len() - THREAD_NAME_DATABASE_ID_SUFFIX_LEN..];
    format!("js-main-{suffix}")
}

fn spawn_v8_worker_thread(worker_kind: JsWorkerKind, database_identity: Identity, f: impl FnOnce() + Send + 'static) {
    match worker_kind {
        JsWorkerKind::Main => {
            std::thread::Builder::new()
                .name(js_main_worker_thread_name(database_identity))
                .spawn(f)
                .expect("failed to spawn V8 worker thread");
        }
        JsWorkerKind::Procedure => {
            std::thread::spawn(f);
        }
    }
}

/// Spawns an instance worker for `program` and returns on success the
/// corresponding instance handle that talks to the worker.
///
/// When [`ModuleCommon`] is passed, it's assumed that this program has already
/// been validated. In that case, `Ok(_)` should be returned.
///
/// Otherwise, when [`ModuleCreationContext`] is passed, this is the first time
/// both the module and instance are created.
///
/// `load_balance_guard` and `core_pinner` should both be from the same
/// [`AllocatedJobCore`], and are used to manage the core pinning of this thread.
async fn spawn_instance_worker<W>(
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContext>,
    load_balance_guard: Arc<LoadBalanceOnDropGuard>,
    mut core_pinner: CorePinner,
    heap_policy: V8HeapPolicyConfig,
    instance_metrics: InstanceManagerMetrics,
) -> anyhow::Result<(ModuleCommon, W::Instance)>
where
    W: JsWorkerSpec + 'static,
{
    // This one-shot channel is used for initial startup error handling within the thread.
    let (result_tx, result_rx) = oneshot::channel();
    let worker_kind = W::KIND;
    let database_identity = match &module_or_mcc {
        Either::Left(module_common) => module_common.info().database_identity,
        Either::Right(mcc) => mcc.replica_ctx.database_identity,
    };
    let (request_tx, mut request_rx) = W::channel(&database_identity);

    let rt = tokio::runtime::Handle::current();

    spawn_v8_worker_thread(worker_kind, database_identity, move || {
        let _guard = load_balance_guard;
        core_pinner.pin_now();

        let _entered = rt.enter();
        let mut initial_module_or_mcc = Some(module_or_mcc);
        let mut startup_result_tx = Some(result_tx);
        let mut module_common_for_recreate = None::<ModuleCommon>;

        'worker: loop {
            let replacing_instance = module_common_for_recreate.is_some();
            let generation_start_time = replacing_instance.then(Instant::now);
            let generation_module_or_mcc = match module_common_for_recreate.clone() {
                Some(module_common) => Either::Left(module_common),
                None => initial_module_or_mcc
                    .take()
                    .expect("first JS worker generation should have startup input"),
            };

            // Create the isolate and enter one worker scope/context for this generation.
            //
            // Traps and heap-limit retirement both end the current generation. The worker
            // thread itself stays alive, immediately rebuilds the isolate, and then continues
            // draining the same request queue.
            {
                let mut isolate = new_isolate(heap_policy);
                scope_with_context!(let scope, &mut isolate, Context::new(scope, Default::default()));

                // Setup the instance environment.
                let (replica_ctx, scheduler) = match &generation_module_or_mcc {
                    Either::Left(module) => (module.replica_ctx(), module.scheduler()),
                    Either::Right(mcc) => (&mcc.replica_ctx, &mcc.scheduler),
                };
                let instance_env = InstanceEnv::new(replica_ctx.clone(), scheduler.clone());
                scope.set_slot(JsInstanceEnv::new(instance_env));

                let startup_result = panic::catch_unwind(AssertUnwindSafe(|| {
                    catch_exception(scope, |scope| Ok(builtins::evaluate_builtins(scope)?))
                        .expect("our builtin code shouldn't error");

                    // Setup the JS module, find call_reducer, and maybe build the module.
                    startup_instance_worker(scope, program.clone(), generation_module_or_mcc)
                }));

                let (hooks, module_common) = match startup_result {
                    Err(_) => {
                        if let Some(result_tx) = startup_result_tx.take() {
                            if result_tx
                                .send(Err(anyhow::anyhow!("JS worker panicked during startup")))
                                .is_err()
                            {
                                log::error!("startup result receiver disconnected");
                            }
                        } else {
                            log::error!("JS worker panicked while recreating isolate");
                        }
                        return;
                    }
                    Ok(Err(err)) => {
                        if let Some(result_tx) = startup_result_tx.take() {
                            if result_tx.send(Err(err)).is_err() {
                                log::error!("startup result receiver disconnected");
                            }
                        } else {
                            log::error!("failed to restart JS worker: {err:#}");
                        }
                        return;
                    }
                    Ok(Ok((crf, module_common))) => {
                        env_on_isolate_unwrap(scope).set_module_def(module_common.info().module_def.clone());

                        if let Some(result_tx) = startup_result_tx.take()
                            && result_tx.send(Ok(module_common.clone())).is_err()
                        {
                            log::error!("startup result receiver disconnected");
                            return;
                        }

                        module_common_for_recreate = Some(module_common.clone());
                        if let Some(start_time) = generation_start_time {
                            instance_metrics.observe_instance_created(start_time.elapsed());
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
                let args = Global::new(scope, ArrayBuffer::new(scope, REDUCER_ARGS_BUFFER_SIZE));
                let info = &module_common.info();
                let mut instance_common = InstanceCommon::new(&module_common);
                let replica_ctx: &Arc<ReplicaContext> = module_common.replica_ctx();
                let mut heap_metrics = worker_kind
                    .checks_heap()
                    .then(|| V8HeapMetrics::new(&info.database_identity));

                let mut inst = V8Instance {
                    scope,
                    replica_ctx,
                    hooks: &hooks,
                    args: &args,
                    heap_limit_hit_metric: &WORKER_METRICS
                        .v8_heap_limit_hit
                        .with_label_values(&info.database_identity),
                    initial_heap_limit: heap_policy.heap_limit_bytes,
                };
                if let Some(heap_metrics) = heap_metrics.as_mut() {
                    let _initial_heap_stats = sample_heap_stats(inst.scope, heap_metrics);
                }

                // Process requests to the worker.
                //
                // The loop is terminated when the last worker instance handle is dropped.
                // This will cause channels, scopes, and the isolate to be cleaned up.
                let mut requests_since_heap_check = 0u64;
                let mut last_heap_check_at = Instant::now();
                while let Some(request) = W::blocking_recv(&mut request_rx) {
                    core_pinner.pin_if_changed();

                    let mut outcome =
                        W::handle_request(request, &mut instance_common, &mut inst, &module_common, replica_ctx);

                    if let WorkerRequestOutcome::Continue = outcome
                        && let Some(heap_metrics) = heap_metrics.as_mut()
                    {
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
                            if let Some((used, limit)) =
                                should_retire_worker_for_heap(inst.scope, heap_metrics, heap_policy)
                            {
                                outcome = outcome.recreate_instance();
                                log::warn!(
                                    "recreating JS isolate after V8 heap stayed high post-GC: used={}MiB limit={}MiB",
                                    used / (1024 * 1024),
                                    limit / (1024 * 1024),
                                );
                            }
                        }
                    }

                    match outcome {
                        WorkerRequestOutcome::Continue => {}
                        WorkerRequestOutcome::RecreateInstance => {
                            instance_metrics.track_instance_removed();
                            continue 'worker;
                        }
                        WorkerRequestOutcome::Fatal => return,
                    }
                }
                return;
            }
        }
    });

    // Get the module, if any, and get any setup errors from the worker.
    let res: Result<ModuleCommon, anyhow::Error> = result_rx.await.expect("should have a sender");
    res.map(|opt_mc| {
        let inst = W::make_instance(request_tx);
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
    args: &'a Global<ArrayBuffer>,
    /// Metric for the number of times the v8 heap limit has been hit.
    heap_limit_hit_metric: &'a IntCounter,
    initial_heap_limit: usize,
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
        let args = self.args;
        common_call(self, budget, op, |scope, hooks, op| {
            let reducer_args_buf = Local::new(scope, args);
            Ok(call_call_reducer(scope, hooks, op, reducer_args_buf)?)
        })
        .map_result(|call_result| call_result.and_then(|res| res.map_err(ExecutionError::User)))
    }

    fn call_view(&mut self, op: ViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self, budget, op, |scope, hooks, op| call_call_view(scope, hooks, op))
    }

    fn call_view_anon(&mut self, op: AnonymousViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self, budget, op, |scope, hooks, op| {
            call_call_view_anon(scope, hooks, op)
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
        let result = common_call(self, budget, op, |scope, hooks, op| {
            call_call_procedure(scope, hooks, op)
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

    async fn call_http_handler(
        &mut self,
        op: HttpHandlerOp,
        budget: FunctionBudget,
    ) -> (HttpHandlerExecuteResult, Option<TransactionOffset>) {
        let result = common_call(self, budget, op, |scope, hooks, op| {
            call_call_http_handler(scope, hooks, op)
        })
        .map_result(|call_result| {
            call_result.map_err(|e| match e {
                ExecutionError::User(e) => anyhow::Error::msg(e),
                ExecutionError::Recoverable(e) | ExecutionError::Trap(e) => e,
            })
        });
        (result, None)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Javascript module exceeded memory limit (current limit: {current}, initial: {initial})")]
struct ExceededMemoryLimit {
    current: usize,
    initial: usize,
}

fn common_call<R, O, F>(
    inst: &mut V8Instance<'_, '_, '_>,
    budget: FunctionBudget,
    op: O,
    call: F,
) -> ExecutionResult<R, ExecutionError>
where
    O: InstanceOp,
    F: FnOnce(&mut PinTryCatch<'_, '_, '_, '_>, &HookFunctions<'_>, O) -> Result<R, ErrorOrException<ExceptionThrown>>,
{
    let scope = &mut *inst.scope;

    let heap_limit_hit = Cell::new(0u32);

    // This closure gets called when the configured near heap limit is hit.
    // We use a near-heap limit as V8 aborts the process when the hard limit is hit.
    // We'd like to gracefully terminate execution, and not abort,
    // so we ask V8 to terminate execution as soon as possible
    // and double the heap limit in the hopes
    // that V8 manages to stop execution before hitting the new limit.
    // Note that V8 does not terminate execution immediately when requested
    // so this mechanism is unfortunately not fool-proof
    // but we consider it to be good enough.
    let terminator = error::RemoteTerminator::new(scope);
    let termination_flag = &terminator.flag;

    let near_heap_limit_callback = |current: usize, initial: usize| {
        heap_limit_hit.update(|x| x + 1);
        inst.heap_limit_hit_metric.inc();
        terminator.terminate_execution(ExceededMemoryLimit { current, initial }.into());
        current.saturating_mul(2)
    };

    with_near_heap_limit_callback(scope, inst.initial_heap_limit, near_heap_limit_callback, |scope| {
        // Open a fresh HandleScope for this invocation so call-local V8 handles
        // are released when the reducer/view/procedure returns.
        v8::scope!(let scope, scope);

        // TODO(v8): Start the budget timeout and long-running logger.
        let env = env_on_isolate_unwrap(scope);

        // Start the timer.
        // We'd like this tightly around `call`.
        env.start_funcall(op.name().clone(), op.timestamp(), op.call_type());

        // Wrap the call in `TryCatch`.
        //
        // `v8::tc_scope!` adds exception handling on top of the current scope; it
        // does not create a new HandleScope. The fresh per-call HandleScope is
        // opened by the caller before entering `common_call`.
        v8::tc_scope!(let scope, scope);

        let call_result = call(scope, inst.hooks, op).map_err(|mut e| {
            if let ErrorOrException::Exception(_) = e {
                // If we're terminating execution, don't try to check `instanceof`.
                if scope.can_continue()
                    && let Some(exc) = scope.exception()
                {
                    match process_thrown_exception(scope, inst.hooks, exc) {
                        Ok(Some(err)) => return err,
                        Ok(None) => {}
                        Err(exc) => e = ErrorOrException::Exception(exc),
                    }
                }
            }

            let e = e.exc_into_error(scope).map(anyhow::Error::from);
            let termination_error = termination_flag.clear();
            if scope.can_continue() {
                // We can continue.
                ExecutionError::Recoverable(e.unwrap_or_else(Into::into))
            } else if scope.has_terminated() {
                // We can continue if we do `Isolate::cancel_terminate_execution`.
                // Must be called *after* we check `has_terminated()`, or else it will
                // cause it to return `false`.
                scope.cancel_terminate_execution();
                let e = e.unwrap_or_else(|unknown| termination_error.unwrap_or_else(|| unknown.into()));
                ExecutionError::Recoverable(e)
            } else {
                // We cannot continue.
                ExecutionError::Trap(e.unwrap_or_else(Into::into))
            }
        });

        // Ensure there's no lingering termination request.
        termination_flag.clear();
        scope.cancel_terminate_execution();

        let env = env_on_isolate_unwrap(scope);

        // Finish timings.
        let timings = env.finish_funcall();

        // Derive energy stats.
        let energy = energy_from_elapsed(budget, timings.total_duration);

        // Reuse the last periodic heap sample instead of querying V8 on every call.
        // We use this statistic for energy tracking, so eventual consistency is fine.
        let memory_allocation = env.cached_used_heap_size();

        if heap_limit_hit.get() > 1 {
            let database_identity = *env.instance_env.database_identity();
            tracing::warn!(
                %database_identity,
                used_heap_size = memory_allocation,
                current_heap_limit = scope.get_heap_statistics().heap_size_limit(),
                "Module hit heap limit multiple times in single call, even after doubling!",
            )
        }

        let stats = ExecutionStats {
            energy,
            timings,
            memory_allocation,
        };
        ExecutionResult { stats, call_result }
    })
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
