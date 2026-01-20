use self::budget::energy_from_elapsed;
use self::error::{
    catch_exception, exception_already_thrown, log_traceback, BufferTooSmall, CanContinue, ErrorOrException, ExcResult,
    ExceptionThrown, JsStackTrace, TerminationError, Throwable,
};
use self::ser::serialize_to_js;
use self::string::{str_from_ident, IntoJsString};
use self::syscall::{
    call_call_procedure, call_call_reducer, call_call_view, call_call_view_anon, call_describe_module, get_hooks,
    resolve_sys_module, FnRet, HookFunctions,
};
use super::module_common::{build_common_module_from_raw, run_describer, ModuleCommon};
use super::module_host::{CallProcedureParams, CallReducerParams, Module, ModuleInfo, ModuleRuntime};
use super::UpdateDatabaseResult;
use crate::client::ClientActorId;
use crate::host::host_controller::CallProcedureReturn;
use crate::host::instance_env::{ChunkPool, InstanceEnv, TxSlot};
use crate::host::module_host::{
    call_identity_connected, init_database, ClientConnectedError, Instance, ViewCommand, ViewCommandResult,
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
use crate::module_host_context::{ModuleCreationContext, ModuleCreationContextLimited};
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_manager::TransactionOffset;
use crate::util::asyncify;
use anyhow::Context as _;
use core::any::type_name;
use core::str;
use enum_as_inner::EnumAsInner;
use futures::FutureExt;
use itertools::Either;
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_client_api_messages::energy::FunctionBudget;
use spacetimedb_datastore::locking_tx_datastore::FuncCallType;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{ConnectionId, Identity, RawModuleDef, Timestamp};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::sync::oneshot;
use v8::script_compiler::{compile_module, Source};
use v8::{
    scope_with_context, Context, Function, Isolate, Local, MapFnTo, OwnedIsolate, PinScope, ResolveModuleCallback,
    ScriptOrigin, Value,
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
#[derive(Default)]
pub struct V8Runtime {
    _priv: (),
}

impl ModuleRuntime for V8Runtime {
    fn make_actor(&self, mcc: ModuleCreationContext) -> anyhow::Result<(Module, Instance)> {
        V8_RUNTIME_GLOBAL.make_actor(mcc)
    }
}

#[cfg(test)]
impl V8Runtime {
    fn init_for_test() {
        LazyLock::force(&V8_RUNTIME_GLOBAL);
    }
}

static V8_RUNTIME_GLOBAL: LazyLock<V8RuntimeInner> = LazyLock::new(V8RuntimeInner::init);

/// The actual V8 runtime, with initialization of V8.
struct V8RuntimeInner {
    _priv: (),
}

impl V8RuntimeInner {
    /// Initializes the V8 platform and engine.
    ///
    /// Should only be called once but it isn't unsound to call it more times.
    fn init() -> Self {
        v8::V8::set_flags_from_string("--single-threaded");
        // Our current configuration:
        // - will pick a number of worker threads for background jobs based on the num CPUs.
        // - does not allow idle tasks
        let platform = v8::new_single_threaded_default_platform(false).make_shared();
        // Initialize V8. Internally, this uses a global lock so it's safe that we don't.
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();

        Self { _priv: () }
    }
}

impl ModuleRuntime for V8RuntimeInner {
    fn make_actor(&self, mcc: ModuleCreationContext) -> anyhow::Result<(Module, Instance)> {
        #![allow(unreachable_code, unused_variables)]

        log::trace!(
            "Making new V8 module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program.hash,
        );

        // Convert program to a string.
        let program: Arc<str> = str::from_utf8(&mcc.program.bytes)?.into();

        // Validate/create the module and spawn the first instance.
        let mcc = Either::Right(mcc.into_limited());
        let (common, init_inst) = spawn_instance_worker(program.clone(), mcc)?;

        let module = Module::Js(JsModule { common, program });
        let init_inst = Instance::Js(Box::new(init_inst));
        Ok((module, init_inst))
    }
}

#[derive(Clone)]
pub struct JsModule {
    common: ModuleCommon,
    program: Arc<str>,
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

        asyncify(move || {
            // This has to be done in a blocking context because of `blocking_recv`.
            let (_, instance) = spawn_instance_worker(program, Either::Left(common))
                .expect("`spawn_instance_worker` should succeed when passed `ModuleCommon`");
            instance
        })
        .await
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
    fn start_funcall(&mut self, name: &str, ts: Timestamp, func_type: FuncCallType) {
        self.instance_env.start_funcall(name, ts, func_type);
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    fn funcall_name(&self) -> &str {
        &self.instance_env.func_name
    }

    /// Returns the name of the most recent reducer to be run in this environment,
    /// or `None` if no reducer is actively being invoked.
    fn log_record_function(&self) -> Option<&str> {
        let function = self.funcall_name();
        (!function.is_empty()).then_some(function)
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
}

/// An instance for a [`JsModule`].
///
/// The actual work happens in a worker thread,
/// which the instance communicates with through channels.
///
/// When the instance is dropped, the channels will hang up,
/// which will cause the worker's loop to terminate
/// and cleanup the isolate and friends.
pub struct JsInstance {
    request_tx: flume::Sender<JsWorkerRequest>,
    reply_rx: flume::Receiver<(JsWorkerReply, bool)>,
    trapped: bool,
}

impl JsInstance {
    pub fn trapped(&self) -> bool {
        self.trapped
    }

    /// Send a request to the worker and wait for a reply.
    async fn send_recv<T>(
        mut self: Box<Self>,
        extract: impl FnOnce(JsWorkerReply) -> Result<T, JsWorkerReply>,
        request: JsWorkerRequest,
    ) -> (T, Box<Self>) {
        // Send the request.
        self.request_tx
            .send_async(request)
            .await
            .expect("worker's `request_rx` should be live as `JsInstance::drop` hasn't happened");

        // Wait for the response.
        let (reply, trapped) = self
            .reply_rx
            .recv_async()
            .await
            .expect("worker's `reply_tx` should be live as `JsInstance::drop` hasn't happened");

        self.trapped = trapped;

        match extract(reply) {
            Err(err) => unreachable!("should have received {} but got {err:?}", type_name::<T>()),
            Ok(reply) => (reply, self),
        }
    }

    pub async fn update_database(
        self: Box<Self>,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> (anyhow::Result<UpdateDatabaseResult>, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_update_database,
            JsWorkerRequest::UpdateDatabase {
                program,
                old_module_info,
                policy,
            },
        )
        .await
    }

    pub async fn call_reducer(self: Box<Self>, params: CallReducerParams) -> (ReducerCallResult, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_call_reducer,
            JsWorkerRequest::CallReducer { params },
        )
        .await
    }

    pub async fn clear_all_clients(self: Box<Self>) -> (anyhow::Result<()>, Box<Self>) {
        self.send_recv(JsWorkerReply::into_clear_all_clients, JsWorkerRequest::ClearAllClients)
            .await
    }

    pub async fn call_identity_connected(
        self: Box<Self>,
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> (Result<(), ClientConnectedError>, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_call_identity_connected,
            JsWorkerRequest::CallIdentityConnected(caller_auth, caller_connection_id),
        )
        .await
    }

    pub async fn call_identity_disconnected(
        self: Box<Self>,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        drop_view_subscribers: bool,
    ) -> (Result<(), ReducerCallError>, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_call_identity_disconnected,
            JsWorkerRequest::CallIdentityDisconnected(caller_identity, caller_connection_id, drop_view_subscribers),
        )
        .await
    }

    pub async fn disconnect_client(
        self: Box<Self>,
        client_id: ClientActorId,
    ) -> (Result<(), ReducerCallError>, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_disconnect_client,
            JsWorkerRequest::DisconnectClient(client_id),
        )
        .await
    }

    pub async fn init_database(
        self: Box<Self>,
        program: Program,
    ) -> (anyhow::Result<Option<ReducerCallResult>>, Box<Self>) {
        self.send_recv(
            JsWorkerReply::into_init_database,
            JsWorkerRequest::InitDatabase(program),
        )
        .await
    }

    pub async fn call_procedure(self: Box<Self>, params: CallProcedureParams) -> (CallProcedureReturn, Box<Self>) {
        // Get a handle to the current tokio runtime, and pass it to the worker
        // so that it can execute futures.
        let rt = tokio::runtime::Handle::current();
        let (r, s) = self
            .send_recv(
                JsWorkerReply::into_call_procedure,
                JsWorkerRequest::CallProcedure { params, rt },
            )
            .await;
        (*r, s)
    }

    pub async fn call_view(self: Box<Self>, cmd: ViewCommand) -> (ViewCommandResult, Box<Self>) {
        let (r, s) = self
            .send_recv(JsWorkerReply::into_call_view, JsWorkerRequest::CallView { cmd })
            .await;
        (*r, s)
    }

    pub(in crate::host) async fn call_scheduled_function(
        self: Box<Self>,
        params: ScheduledFunctionParams,
    ) -> (CallScheduledFunctionResult, Box<Self>) {
        // Get a handle to the current tokio runtime, and pass it to the worker
        // so that it can execute futures.
        let rt = tokio::runtime::Handle::current();
        self.send_recv(
            JsWorkerReply::into_call_scheduled_function,
            JsWorkerRequest::CallScheduledFunction(params, rt),
        )
        .await
    }
}

/// A reply from the worker in [`spawn_instance_worker`].
#[derive(EnumAsInner, Debug)]
enum JsWorkerReply {
    UpdateDatabase(anyhow::Result<UpdateDatabaseResult>),
    CallReducer(ReducerCallResult),
    CallView(Box<ViewCommandResult>),
    CallProcedure(Box<CallProcedureReturn>),
    ClearAllClients(anyhow::Result<()>),
    CallIdentityConnected(Result<(), ClientConnectedError>),
    CallIdentityDisconnected(Result<(), ReducerCallError>),
    DisconnectClient(Result<(), ReducerCallError>),
    InitDatabase(anyhow::Result<Option<ReducerCallResult>>),
    CallScheduledFunction(CallScheduledFunctionResult),
}

/// A request for the worker in [`spawn_instance_worker`].
// We care about optimizing for `CallReducer` as it happens frequently,
// so we don't want to box anything in it.
enum JsWorkerRequest {
    /// See [`JsInstance::update_database`].
    UpdateDatabase {
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    },
    /// See [`JsInstance::call_reducer`].
    CallReducer { params: CallReducerParams },
    /// See [`JsInstance::call_view`].
    CallView { cmd: ViewCommand },
    /// See [`JsInstance::call_procedure`].
    CallProcedure {
        params: CallProcedureParams,
        rt: tokio::runtime::Handle,
    },
    /// See [`JsInstance::clear_all_clients`].
    ClearAllClients,
    /// See [`JsInstance::call_identity_connected`].
    CallIdentityConnected(ConnectionAuthCtx, ConnectionId),
    /// See [`JsInstance::call_identity_disconnected`].
    CallIdentityDisconnected(Identity, ConnectionId, bool),
    /// See [`JsInstance::disconnect_client`].
    DisconnectClient(ClientActorId),
    /// See [`JsInstance::init_database`].
    InitDatabase(Program),
    /// See [`JsInstance::call_scheduled_function`].
    CallScheduledFunction(ScheduledFunctionParams, tokio::runtime::Handle),
}

/// Performs some of the startup work of [`spawn_instance_worker`].
///
/// NOTE(centril): in its own function due to lack of `try` blocks.
fn startup_instance_worker<'scope>(
    scope: &mut PinScope<'scope, '_>,
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContextLimited>,
) -> anyhow::Result<(HookFunctions<'scope>, Either<ModuleCommon, ModuleCommon>)> {
    // Start-up the user's module.
    eval_user_module_catch(scope, &program).map_err(DescribeError::Setup)?;

    // Find the `__call_reducer__` function.
    let hook_functions = get_hooks(scope).context("The `spacetimedb/server` module was never imported")?;

    // If we don't have a module, make one.
    let module_common = match module_or_mcc {
        Either::Left(module_common) => Either::Left(module_common),
        Either::Right(mcc) => {
            let def = extract_description(scope, &hook_functions, &mcc.replica_ctx)?;

            // Validate and create a common module from the raw definition.
            Either::Right(build_common_module_from_raw(mcc, def)?)
        }
    };

    Ok((hook_functions, module_common))
}

/// Returns a new isolate.
fn new_isolate() -> OwnedIsolate {
    let mut isolate = Isolate::new(<_>::default());
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 1024);
    isolate
}

/// Spawns an instance worker for `program`
/// and returns on success the corresponding [`JsInstance`]
/// that talks to the worker.
///
/// When [`ModuleCommon`] is passed,
/// it's assumed that `spawn_instance_worker` has already happened once for this `program`
/// and that it has been validated.
/// In that case, `Ok(_)` should be returned.
///
/// Otherwise, when [`ModuleCreationContextLimited`] is passed,
/// this is the first time both the module and instance are created.
fn spawn_instance_worker(
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContextLimited>,
) -> anyhow::Result<(ModuleCommon, JsInstance)> {
    // Spawn channels for bidirectional communication between worker and instance.
    // The use-case is SPSC and all channels are rendezvous channels
    // where each `.send` blocks until it's received.
    // The Instance --Request-> Worker channel:
    let (request_tx, request_rx) = flume::bounded(0);
    // The Worker --Reply-> Instance channel:
    let (reply_tx, reply_rx) = flume::bounded(0);

    // This one-shot channel is used for initial startup error handling within the thread.
    let (result_tx, result_rx) = oneshot::channel();

    std::thread::spawn(move || {
        // Create the isolate and scope.
        let mut isolate = new_isolate();
        scope_with_context!(let scope, &mut isolate, Context::new(scope, Default::default()));

        catch_exception(scope, |scope| Ok(builtins::evalute_builtins(scope)?))
            .expect("our builtin code shouldn't error");

        // Setup the JS module, find call_reducer, and maybe build the module.
        let send_result = |res| {
            if result_tx.send(res).is_err() {
                unreachable!("should have a live receiver");
            }
        };
        let (hooks, module_common) = match startup_instance_worker(scope, program, module_or_mcc) {
            Err(err) => {
                // There was some error in module setup.
                // Return the error and terminate the worker.
                send_result(Err(err));
                return;
            }
            Ok((crf, module_common)) => {
                // Success! Send `module_common` to the spawner.
                let module_common = module_common.into_inner();
                send_result(Ok(module_common.clone()));
                (crf, module_common)
            }
        };

        // Setup the instance common and environment.
        let info = &module_common.info();
        let mut instance_common = InstanceCommon::new(&module_common);
        let replica_ctx: &Arc<ReplicaContext> = module_common.replica_ctx();
        let scheduler = module_common.scheduler().clone();
        let instance_env = InstanceEnv::new(replica_ctx.clone(), scheduler);
        scope.set_slot(JsInstanceEnv::new(instance_env));

        let mut inst = V8Instance {
            scope,
            replica_ctx,
            hooks: &hooks,
        };

        // Process requests to the worker.
        //
        // The loop is terminated when a `JsInstance` is dropped.
        // This will cause channels, scopes, and the isolate to be cleaned up.
        let reply = |ctx: &str, reply: JsWorkerReply, trapped| {
            if let Err(e) = reply_tx.send((reply, trapped)) {
                // This should never happen as `JsInstance::$function` immediately
                // does `.recv` on the other end of the channel.
                unreachable!("should have receiver for `{ctx}` response, {e}");
            }
        };
        for request in request_rx.iter() {
            let mut call_reducer = |tx, params| instance_common.call_reducer_with_tx(tx, params, &mut inst);

            use JsWorkerReply::*;
            match request {
                JsWorkerRequest::UpdateDatabase {
                    program,
                    old_module_info,
                    policy,
                } => {
                    // Update the database and reply to `JsInstance::update_database`.
                    let res = instance_common.update_database(program, old_module_info, policy, &mut inst);
                    reply("update_database", UpdateDatabase(res), false);
                }
                JsWorkerRequest::CallReducer { params } => {
                    // Call the reducer.
                    // If execution trapped, we don't end the loop here,
                    // but rather let this happen by `return_instance` using `JsInstance::trapped`
                    // which will cause `JsInstance` to be dropped,
                    // which in turn results in the loop being terminated.
                    let (res, trapped) = call_reducer(None, params);
                    reply("call_reducer", CallReducer(res), trapped);
                }
                JsWorkerRequest::CallView { cmd } => {
                    let (res, trapped) = instance_common.handle_cmd(cmd, &mut inst);
                    reply("call_view", JsWorkerReply::CallView(res.into()), trapped);
                }
                JsWorkerRequest::CallProcedure { params, rt } => {
                    // The callee passed us a handle to their tokio runtime - enter its
                    // context so that we can execute futures.
                    let _guard = rt.enter();

                    let (res, trapped) = instance_common
                        .call_procedure(params, &mut inst)
                        .now_or_never()
                        .expect("our call_procedure implementation is not actually async");

                    reply("call_procedure", JsWorkerReply::CallProcedure(res.into()), trapped);
                }
                JsWorkerRequest::ClearAllClients => {
                    let res = instance_common.clear_all_clients();
                    reply("clear_all_clients", ClearAllClients(res), false);
                }
                JsWorkerRequest::CallIdentityConnected(caller_auth, caller_connection_id) => {
                    let mut trapped = false;
                    let res =
                        call_identity_connected(caller_auth, caller_connection_id, info, call_reducer, &mut trapped);
                    reply("call_identity_connected", CallIdentityConnected(res), trapped);
                }
                JsWorkerRequest::CallIdentityDisconnected(
                    caller_identity,
                    caller_connection_id,
                    drop_view_subcribers,
                ) => {
                    let mut trapped = false;
                    let res = ModuleHost::call_identity_disconnected_inner(
                        caller_identity,
                        caller_connection_id,
                        info,
                        drop_view_subcribers,
                        call_reducer,
                        &mut trapped,
                    );
                    reply("call_identity_disconnected", CallIdentityDisconnected(res), trapped);
                }
                JsWorkerRequest::DisconnectClient(client_id) => {
                    let mut trapped = false;
                    let res = ModuleHost::disconnect_client_inner(client_id, info, call_reducer, &mut trapped);
                    reply("disconnect_client", DisconnectClient(res), trapped);
                }
                JsWorkerRequest::InitDatabase(program) => {
                    let (res, trapped): (Result<Option<ReducerCallResult>, anyhow::Error>, bool) =
                        init_database(replica_ctx, &module_common.info().module_def, program, call_reducer);
                    reply("init_database", InitDatabase(res), trapped);
                }
                JsWorkerRequest::CallScheduledFunction(params, rt) => {
                    let _guard = rt.enter();

                    let (res, trapped) = instance_common
                        .call_scheduled_function(params, &mut inst)
                        .now_or_never()
                        .expect("our call_procedure implementation is not actually async");
                    reply("call_scheduled_function", CallScheduledFunction(res), trapped);
                }
            }
        }
    });

    // Get the module, if any, and get any setup errors from the worker.
    let res: Result<ModuleCommon, anyhow::Error> = result_rx.blocking_recv().expect("should have a sender");
    res.map(|opt_mc| {
        let inst = JsInstance {
            request_tx,
            reply_rx,
            trapped: false,
        };
        (opt_mc, inst)
    })
}

/// Compiles, instantiate, and evaluate `code` as a module.
fn eval_module<'scope>(
    scope: &mut PinScope<'scope, '_>,
    resource_name: Local<'scope, Value>,
    code: Local<'_, v8::String>,
    resolve_deps: impl MapFnTo<ResolveModuleCallback<'scope>>,
) -> ExcResult<(Local<'scope, v8::Module>, Local<'scope, v8::Promise>)> {
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
    if value.state() == v8::PromiseState::Pending {
        // If the user were to put top-level `await new Promise((resolve) => { /* do nothing */ })`
        // the module value would never actually resolve. For now, reject this entirely.
        return Err(error::TypeError("module has top-level await and is pending").throw(scope));
    }

    Ok((module, value))
}

/// Compiles, instantiate, and evaluate the user module with `code`.
fn eval_user_module<'scope>(
    scope: &mut PinScope<'scope, '_>,
    code: &str,
) -> ExcResult<(Local<'scope, v8::Module>, Local<'scope, v8::Promise>)> {
    // Convert the code to a string.
    let code = code.into_string(scope).map_err(|e| e.into_range_error().throw(scope))?;

    let name = str_from_ident!(spacetimedb_module).string(scope).into();
    eval_module(scope, name, code, resolve_sys_module)
}

/// Compiles, instantiate, and evaluate the user module with `code`
/// and catch any exceptions.
fn eval_user_module_catch<'scope>(scope: &mut PinScope<'scope, '_>, program: &str) -> anyhow::Result<()> {
    catch_exception(scope, |scope| {
        eval_user_module(scope, program)?;
        Ok(())
    })
    .map_err(|(e, _)| e)
    .map_err(Into::into)
}

/// Calls free function `fun` with `args`.
fn call_free_fun<'scope>(
    scope: &PinScope<'scope, '_>,
    fun: Local<'scope, Function>,
    args: &[Local<'scope, Value>],
) -> FnRet<'scope> {
    let receiver = v8::undefined(scope).into();
    fun.call(scope, receiver, args).ok_or_else(exception_already_thrown)
}

struct V8Instance<'a, 'scope, 'isolate> {
    scope: &'a mut PinScope<'scope, 'isolate>,
    replica_ctx: &'a Arc<ReplicaContext>,
    hooks: &'a HookFunctions<'a>,
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

    fn call_reducer(&mut self, op: ReducerOp<'_>, budget: FunctionBudget) -> ReducerExecuteResult {
        common_call(self.scope, budget, op, |scope, op| {
            Ok(call_call_reducer(scope, self.hooks, op)?)
        })
        .map_result(|call_result| call_result.and_then(|res| res.map_err(ExecutionError::User)))
    }

    fn call_view(&mut self, op: ViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self.scope, budget, op, |scope, op| {
            call_call_view(scope, self.hooks, op)
        })
    }

    fn call_view_anon(&mut self, op: AnonymousViewOp<'_>, budget: FunctionBudget) -> ViewExecuteResult {
        common_call(self.scope, budget, op, |scope, op| {
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
        let result = common_call(self.scope, budget, op, |scope, op| {
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
    budget: FunctionBudget,
    op: O,
    call: F,
) -> ExecutionResult<R, ExecutionError>
where
    O: InstanceOp,
    F: FnOnce(&mut PinScope<'scope, '_>, O) -> Result<R, ErrorOrException<ExceptionThrown>>,
{
    // TODO(v8): Start the budget timeout and long-running logger.
    let env = env_on_isolate_unwrap(scope);

    // Start the timer.
    // We'd like this tightly around `call`.
    env.start_funcall(op.name(), op.timestamp(), op.call_type());

    let call_result = catch_exception(scope, |scope| call(scope, op)).map_err(|(e, can_continue)| {
        // Convert `can_continue` to whether the isolate has "trapped".
        // Also cancel execution termination if needed,
        // that can occur due to terminating long running reducers.
        match can_continue {
            CanContinue::No => ExecutionError::Trap(e.into()),
            CanContinue::Yes => ExecutionError::Recoverable(e.into()),
            CanContinue::YesCancelTermination => {
                scope.cancel_terminate_execution();
                ExecutionError::Recoverable(e.into())
            }
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
            catch_exception(scope, |scope| {
                let def = call_describe_module(scope, hooks)?;
                Ok(def)
            })
            .map_err(|(e, _)| e.into())
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

    fn with_module_catch<T>(
        code: &str,
        logic: impl for<'scope> FnOnce(&mut PinScope<'scope, '_>) -> Result<T, ErrorOrException<ExceptionThrown>>,
    ) -> anyhow::Result<T> {
        with_scope(|scope| {
            eval_user_module_catch(scope, code).unwrap();
            catch_exception(scope, |scope| {
                let ret = logic(scope)?;
                Ok(ret)
            })
            .map_err(|(e, _)| e)
            .map_err(anyhow::Error::from)
        })
    }

    #[test]
    fn call_call_reducer_works() {
        let call = |code| {
            with_module_catch(code, |scope| {
                let hooks = get_hooks(scope).unwrap();
                let op = ReducerOp {
                    id: ReducerId(42),
                    name: "foobar",
                    caller_identity: &Identity::ONE,
                    caller_connection_id: &ConnectionId::ZERO,
                    timestamp: Timestamp::from_micros_since_unix_epoch(24),
                    args: &ArgsTuple::nullary(),
                };
                Ok(call_call_reducer(scope, &hooks, op)?)
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
        let actual = format!("{}", ret.expect_err("should trap")).replace("\t", "    ");
        let expected = r#"
js error Uncaught Error: foobar
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
        let raw_mod = with_module_catch(code, |scope| {
            let hooks = get_hooks(scope).unwrap();
            call_describe_module(scope, &hooks)
        })
        .map_err(|e| e.to_string());
        assert_eq!(raw_mod, Ok(RawModuleDef::V9(<_>::default())));
    }
}
