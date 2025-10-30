use self::budget::energy_from_elapsed;
use self::error::{
    catch_exception, exception_already_thrown, log_traceback, BufferTooSmall, CanContinue, CodeError, ExcResult,
    JsStackTrace, TerminationError, Throwable,
};
use self::ser::serialize_to_js;
use self::string::{str_from_ident, IntoJsString};
use self::syscall::{call_call_reducer, call_describe_module, call_reducer_fun, resolve_sys_module, FnRet};
use super::module_common::{build_common_module_from_raw, run_describer, ModuleCommon};
use super::module_host::{CallProcedureParams, CallReducerParams, Module, ModuleInfo, ModuleRuntime};
use super::UpdateDatabaseResult;
use crate::host::instance_env::{ChunkPool, InstanceEnv};
use crate::host::module_host::Instance;
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{DescribeError, ExecuteResult, ExecutionTimings, InstanceCommon};
use crate::host::wasm_common::{RowIters, TimingSpanSet};
use crate::host::{ReducerCallResult, Scheduler};
use crate::module_host_context::{ModuleCreationContext, ModuleCreationContextLimited};
use crate::replica_context::ReplicaContext;
use crate::util::asyncify;
use core::str;
use itertools::Either;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{RawModuleDef, Timestamp};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{mpsc, Arc, LazyLock};
use std::time::Instant;
use tokio::sync::oneshot;
use v8::script_compiler::{compile_module, Source};
use v8::{
    scope_with_context, Context, Function, Isolate, Local, MapFnTo, OwnedIsolate, PinScope, ResolveModuleCallback,
    ScriptOrigin, Value,
};

mod budget;
mod de;
mod error;
mod from_value;
mod ser;
mod string;
mod syscall;
mod to_value;

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

    /// The point in time the last reducer call started at.
    reducer_start: Instant,

    /// Track time spent in all wasm instance env calls (aka syscall time).
    ///
    /// Each function, like `insert`, will add the `Duration` spent in it
    /// to this tracker.
    call_times: CallTimes,

    /// The last, including current, reducer to be executed by this environment.
    reducer_name: String,

    /// A pool of unused allocated chunks that can be reused.
    // TODO(Centril): consider using this pool for `console_timer_start` and `bytes_sink_write`.
    chunk_pool: ChunkPool,
}

impl JsInstanceEnv {
    /// Returns a new [`JsInstanceEnv`] wrapping `instance_env` with some defaults.
    fn new(instance_env: InstanceEnv) -> Self {
        Self {
            instance_env,
            reducer_start: Instant::now(),
            call_times: CallTimes::new(),
            iters: <_>::default(),
            reducer_name: "<initializing>".into(),
            chunk_pool: <_>::default(),
            timing_spans: <_>::default(),
        }
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is beginning.
    ///
    /// Returns the handle used by reducers to read from `args`
    /// as well as the handle used to write the error message, if any.
    fn start_reducer(&mut self, name: &str, ts: Timestamp) {
        self.reducer_start = Instant::now();
        name.clone_into(&mut self.reducer_name);
        self.instance_env.start_funcall(ts);
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    fn reducer_name(&self) -> &str {
        &self.reducer_name
    }

    /// Returns the name of the most recent reducer to be run in this environment,
    /// or `None` if no reducer is actively being invoked.
    fn log_record_function(&self) -> Option<&str> {
        let function = self.reducer_name();
        (!function.is_empty()).then_some(function)
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    fn reducer_start(&self) -> Instant {
        self.reducer_start
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is over.
    /// This resets all of the state associated to a single reducer call,
    /// and returns instrumentation records.
    fn finish_reducer(&mut self) -> ExecutionTimings {
        let total_duration = self.reducer_start.elapsed();

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
    request_tx: SyncSender<JsWorkerRequest>,
    update_response_rx: Receiver<anyhow::Result<UpdateDatabaseResult>>,
    call_reducer_response_rx: Receiver<(ReducerCallResult, bool)>,
    trapped: bool,
}

impl JsInstance {
    pub fn trapped(&self) -> bool {
        self.trapped
    }

    pub fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        // Send the request.
        let request = JsWorkerRequest::UpdateDatabase {
            program,
            old_module_info,
            policy,
        };
        self.request_tx
            .send(request)
            .expect("worker's `request_rx` should be live as `JsInstance::drop` hasn't happened");

        // Wait for the response.
        self.update_response_rx
            .recv()
            .expect("worker's `update_response_tx` should be live as `JsInstance::drop` hasn't happened")
    }

    pub fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> super::ReducerCallResult {
        // Send the request.
        let request = JsWorkerRequest::CallReducer { tx, params };
        self.request_tx
            .send(request)
            .expect("worker's `request_rx` should be live as `JsInstance::drop` hasn't happened");

        // Wait for the response.
        let (response, trapped) = self
            .call_reducer_response_rx
            .recv()
            .expect("worker's `call_reducer_response_tx` should be live as `JsInstance::drop` hasn't happened");

        self.trapped = trapped;

        response
    }

    pub async fn call_procedure(
        &mut self,
        _params: CallProcedureParams,
    ) -> Result<super::ProcedureCallResult, super::ProcedureCallError> {
        todo!("JS/TS module procedure support")
    }
}

/// A request for the worker in [`spawn_instance_worker`].
// We care about optimizing for `CallReducer` as it happens frequently,
// so we don't want to box anything in it.
#[allow(clippy::large_enum_variant)]
enum JsWorkerRequest {
    /// See [`JsInstance::update_database`].
    UpdateDatabase {
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    },
    /// See [`JsInstance::call_reducer`].
    CallReducer {
        tx: Option<MutTxId>,
        params: CallReducerParams,
    },
}

/// Performs some of the startup work of [`spawn_instance_worker`].
///
/// NOTE(centril): in its own function due to lack of `try` blocks.
fn startup_instance_worker<'scope>(
    scope: &mut PinScope<'scope, '_>,
    program: Arc<str>,
    module_or_mcc: Either<ModuleCommon, ModuleCreationContextLimited>,
) -> anyhow::Result<(Local<'scope, Function>, Either<ModuleCommon, ModuleCommon>)> {
    // Start-up the user's module.
    eval_user_module_catch(scope, &program).map_err(DescribeError::Setup)?;

    // Find the `__call_reducer__` function.
    let call_reducer_fun = catch_exception(scope, |scope| Ok(call_reducer_fun(scope)?)).map_err(|(e, _)| e)?;

    // If we don't have a module, make one.
    let module_common = match module_or_mcc {
        Either::Left(module_common) => Either::Left(module_common),
        Either::Right(mcc) => {
            let def = extract_description(scope, &mcc.replica_ctx)?;

            // Validate and create a common module from the raw definition.
            Either::Right(build_common_module_from_raw(mcc, def)?)
        }
    };

    Ok((call_reducer_fun, module_common))
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
    let (request_tx, request_rx) = mpsc::sync_channel(0);
    // The Worker --UpdateResponse-> Instance channel:
    let (update_response_tx, update_response_rx) = mpsc::sync_channel(0);
    // The Worker --ReducerCallResult-> Instance channel:
    let (call_reducer_response_tx, call_reducer_response_rx) = mpsc::sync_channel(0);

    // This one-shot channel is used for initial startup error handling within the thread.
    let (result_tx, result_rx) = oneshot::channel();

    std::thread::spawn(move || {
        // Create the isolate and scope.
        let mut isolate = new_isolate();
        scope_with_context!(let scope, &mut isolate, Context::new(scope, Default::default()));

        // Setup the JS module, find call_reducer, and maybe build the module.
        let send_result = |res| {
            if result_tx.send(res).is_err() {
                unreachable!("should have a live receiver");
            }
        };
        let (call_reducer_fun, module_common) = match startup_instance_worker(scope, program, module_or_mcc) {
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
        let mut instance_common = InstanceCommon::new(&module_common);
        let replica_ctx: &Arc<ReplicaContext> = module_common.replica_ctx();
        let scheduler = module_common.scheduler().clone();
        let instance_env = InstanceEnv::new(replica_ctx.clone(), scheduler);
        scope.set_slot(JsInstanceEnv::new(instance_env));

        // Process requests to the worker.
        //
        // The loop is terminated when a `JsInstance` is dropped.
        // This will cause channels, scopes, and the isolate to be cleaned up.
        for request in request_rx.iter() {
            match request {
                JsWorkerRequest::UpdateDatabase {
                    program,
                    old_module_info,
                    policy,
                } => {
                    // Update the database.
                    let res = instance_common.update_database(replica_ctx, program, old_module_info, policy);

                    // Reply to `JsInstance::update_database`.
                    if let Err(e) = update_response_tx.send(res) {
                        // This should never happen as `JsInstance::update_database` immediately
                        // does `.recv` on the other end of the channel.
                        unreachable!("should have receiver for `update_database` response, {e}");
                    }
                }
                JsWorkerRequest::CallReducer { tx, params } => {
                    // Call the reducer.
                    // If execution trapped, we don't end the loop here,
                    // but rather let this happen by `return_instance` using `JsInstance::trapped`
                    // which will cause `JsInstance` to be dropped,
                    // which in turn results in the loop being terminated.
                    let res = call_reducer(&mut instance_common, replica_ctx, scope, call_reducer_fun, tx, params);

                    // Reply to `JsInstance::call_reducer`.
                    if let Err(e) = call_reducer_response_tx.send(res) {
                        // This should never happen as `JsInstance::call_reducer` immediately
                        // does `.recv` on the other end of the channel.
                        unreachable!("should have receiver for `call_reducer` response, {e}");
                    }
                }
            }
        }
    });

    // Get the module, if any, and get any setup errors from the worker.
    let res = result_rx.blocking_recv().expect("should have a sender");
    res.map(|opt_mc| {
        let inst = JsInstance {
            request_tx,
            update_response_rx,
            call_reducer_response_rx,
            trapped: false,
        };
        (opt_mc, inst)
    })
}

/// Finds the source map in `code`, if any.
fn find_source_map(code: &str) -> Option<&str> {
    let sm_ref = "//# sourceMappingURL=";
    code.match_indices(sm_ref).find_map(|(i, _)| {
        let (before, after) = code.split_at(i);
        (before.is_empty() || before.ends_with(['\r', '\n']))
            .then(|| &after.lines().next().unwrap_or(after)[sm_ref.len()..])
    })
}

/// Compiles, instantiate, and evaluate `code` as a module.
fn eval_module<'scope>(
    scope: &PinScope<'scope, '_>,
    resource_name: Local<'scope, Value>,
    script_id: i32,
    code: &str,
    resolve_deps: impl MapFnTo<ResolveModuleCallback<'scope>>,
) -> ExcResult<(Local<'scope, v8::Module>, Local<'scope, v8::Promise>)> {
    // Get the source map, if any.
    let source_map_url = find_source_map(code)
        .map(|sm| sm.into_string(scope))
        .transpose()
        .map_err(|e| e.into_range_error().throw(scope))?
        .map(Into::into);

    // Convert the code to a string.
    let code = code.into_string(scope).map_err(|e| e.into_range_error().throw(scope))?;

    // Assemble the source.
    let origin = ScriptOrigin::new(
        scope,
        resource_name,
        0,
        0,
        false,
        script_id,
        source_map_url,
        false,
        false,
        true,
        None,
    );
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
    scope: &PinScope<'scope, '_>,
    code: &str,
) -> ExcResult<(Local<'scope, v8::Module>, Local<'scope, v8::Promise>)> {
    let name = str_from_ident!(spacetimedb_module).string(scope).into();
    eval_module(scope, name, 0, code, resolve_sys_module)
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

fn call_reducer<'scope>(
    instance_common: &mut InstanceCommon,
    replica_ctx: &ReplicaContext,
    scope: &mut PinScope<'scope, '_>,
    fun: Local<'scope, Function>,
    tx: Option<MutTxId>,
    params: CallReducerParams,
) -> (super::ReducerCallResult, bool) {
    let mut trapped = false;

    let (res, _) = instance_common.call_reducer_with_tx(
        replica_ctx,
        tx,
        params,
        move |a, b, c| log_traceback(replica_ctx, a, b, c),
        |tx, op, budget| {
            // TODO(v8): Start the budget timeout and long-running logger.
            let env = env_on_isolate_unwrap(scope);
            let mut tx_slot = env.instance_env.tx.clone();

            // Start the timer.
            // We'd like this tightly around `__call_reducer__`.
            env.start_reducer(op.name, op.timestamp);

            // Call `__call_reducer__` with `tx` provided.
            // It should not be available before.
            let (tx, call_result) = tx_slot.set(tx, || {
                catch_exception(scope, |scope| {
                    let res = call_call_reducer(scope, fun, op)?;
                    Ok(res)
                })
                .map_err(|(e, can_continue)| {
                    // Convert `can_continue` to whether the isolate has "trapped".
                    // Also cancel execution termination if needed,
                    // that can occur due to terminating long running reducers.
                    trapped = match can_continue {
                        CanContinue::No => false,
                        CanContinue::Yes => true,
                        CanContinue::YesCancelTermination => {
                            scope.cancel_terminate_execution();
                            true
                        }
                    };

                    e
                })
                .map_err(anyhow::Error::from)
            });

            // Finish timings.
            let timings = env_on_isolate_unwrap(scope).finish_reducer();

            // Derive energy stats.
            let energy = energy_from_elapsed(budget, timings.total_duration);

            // Fetch the currently used heap size in V8.
            // The used size is ostensibly fairer than the total size.
            let memory_allocation = scope.get_heap_statistics().used_heap_size();

            let exec_result = ExecuteResult {
                energy,
                timings,
                memory_allocation,
                call_result,
            };
            (tx, exec_result)
        },
    );

    (res, trapped)
}

/// Extracts the raw module def by running the registered `__describe_module__` hook.
fn extract_description<'scope>(
    scope: &mut PinScope<'scope, '_>,
    replica_ctx: &ReplicaContext,
) -> Result<RawModuleDef, DescribeError> {
    run_describer(
        |a, b, c| log_traceback(replica_ctx, a, b, c),
        || {
            catch_exception(scope, |scope| {
                let def = call_describe_module(scope)?;
                Ok(def)
            })
            .map_err(|(e, _)| e)
            .map_err(Into::into)
        },
    )
}
#[cfg(test)]
mod test {
    use super::to_value::test::with_scope;
    use super::*;
    use crate::host::wasm_common::module_host_actor::ReducerOp;
    use crate::host::ArgsTuple;
    use spacetimedb_lib::{ConnectionId, Identity};
    use spacetimedb_primitives::ReducerId;

    fn with_module_catch<T>(
        code: &str,
        logic: impl for<'scope> FnOnce(&mut PinScope<'scope, '_>) -> ExcResult<T>,
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
                let fun = call_reducer_fun(scope)?;
                let op = ReducerOp {
                    id: ReducerId(42),
                    name: "foobar",
                    caller_identity: &Identity::ONE,
                    caller_connection_id: &ConnectionId::ZERO,
                    timestamp: Timestamp::from_micros_since_unix_epoch(24),
                    args: &ArgsTuple::nullary(),
                };
                call_call_reducer(scope, fun, op)
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
        let raw_mod = with_module_catch(code, call_describe_module).map_err(|e| e.to_string());
        assert_eq!(raw_mod, Ok(RawModuleDef::V9(<_>::default())));
    }
}
