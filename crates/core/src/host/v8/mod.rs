use self::budget::{energy_from_elapsed, with_timeout_and_cb_every};
use self::error::{
    catch_exception, exception_already_thrown, log_traceback, BufferTooSmall, CodeError, ExcResult, JsStackTrace,
    TerminationError, Throwable,
};
use self::from_value::cast;
use self::ser::serialize_to_js;
use self::string::{str_from_ident, IntoJsString};
use self::syscall::{resolve_sys_module, FnRet};
use super::module_common::{build_common_module_from_raw, run_describer, ModuleCommon};
use super::module_host::{CallReducerParams, Module, ModuleInfo, ModuleRuntime};
use super::UpdateDatabaseResult;
use crate::host::instance_env::{ChunkPool, InstanceEnv};
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    DescribeError, ExecuteResult, ExecutionTimings, InstanceCommon, ReducerOp, ReducerResult,
};
use crate::host::wasm_common::{RowIters, TimingSpanSet};
use crate::host::wasmtime::EPOCH_TICKS_PER_SECOND;
use crate::host::Scheduler;
use crate::{module_host_context::ModuleCreationContext, replica_context::ReplicaContext};
use core::ffi::c_void;
use core::str;
use spacetimedb_client_api_messages::energy::ReducerBudget;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{RawModuleDef, Timestamp};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use v8::script_compiler::{compile_module, Source};
use v8::{
    scope, Context, ContextScope, Function, Isolate, Local, MapFnTo, OwnedIsolate, PinScope, ResolveModuleCallback,
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
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<Module> {
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
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<Module> {
        #![allow(unreachable_code, unused_variables)]

        log::trace!(
            "Making new V8 module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program.hash,
        );

        // TODO(v8): determine min required ABI by module and check that it's supported?

        // TODO(v8): validate function signatures like in WASM? Is that possible with V8?

        // Convert program to a string.
        let program: Arc<str> = str::from_utf8(&mcc.program.bytes)?.into();

        // Run the program as a script and extract the raw module def.
        let desc = extract_description(&program)?;

        // Validate and create a common module rom the raw definition.
        let common = build_common_module_from_raw(mcc, desc)?;

        Ok(Module::Js(JsModule { common, program }))
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

    pub fn create_instance(&self) -> JsInstance {
        // TODO(v8): do we care about preinits / setup or are they unnecessary?

        let common = &self.common;
        let instance_env = InstanceEnv::new(common.replica_ctx().clone(), common.scheduler().clone());
        let instance = JsInstanceEnvSlot::new(JsInstanceEnv {
            instance_env,
            reducer_start: Instant::now(),
            call_times: CallTimes::new(),
            iters: <_>::default(),
            reducer_name: "<initializing>".into(),
            chunk_pool: <_>::default(),
            timing_spans: <_>::default(),
        });

        // NOTE(centril): We don't need to do `extract_description` here
        // as unlike WASM, we have to recreate the isolate every time.

        let common = InstanceCommon::new(common);
        let program = self.program.clone();

        JsInstance {
            common,
            instance,
            program,
        }
    }
}

/// The [`JsInstance`]'s way of holding a [`JsInstanceEnv`]
/// with possible temporary extraction.
struct JsInstanceEnvSlot {
    /// NOTE(centril): The `Option<_>` is due to moving the environment
    /// into [`Isolate`]s and back.
    instance: Option<JsInstanceEnv>,
}

impl JsInstanceEnvSlot {
    /// Creates a new slot to hold `instance`.
    fn new(instance: JsInstanceEnv) -> Self {
        Self {
            instance: Some(instance),
        }
    }

    const EXPECT_ENV: &str = "there should be a `JsInstanceEnv`";

    /// Provides exclusive access to the instance's environment,
    /// assuming it hasn't been moved to an [`Isolate`].
    fn get_mut(&mut self) -> &mut JsInstanceEnv {
        self.instance.as_mut().expect(Self::EXPECT_ENV)
    }

    /// Moves the instance's environment to `isolate`,
    /// assuming it hasn't already been moved there.
    fn move_to_isolate(&mut self, isolate: &mut Isolate) {
        isolate.set_slot(self.instance.take().expect(Self::EXPECT_ENV));
    }

    /// Steals the instance's environment back from `isolate`,
    /// assuming `isolate` still has it in a slot.
    fn take_from_isolate(&mut self, isolate: &mut Isolate) {
        self.instance = isolate.remove_slot();
    }
}

/// Access the `JsInstanceEnv` temporarily bound to an [`Isolate`].
///
/// This assumes that the slot has been set in the isolate already.
fn env_on_isolate(isolate: &mut Isolate) -> &mut JsInstanceEnv {
    isolate.get_slot_mut().expect(JsInstanceEnvSlot::EXPECT_ENV)
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
    /// Signal to this `WasmInstanceEnv` that a reducer call is beginning.
    ///
    /// Returns the handle used by reducers to read from `args`
    /// as well as the handle used to write the error message, if any.
    pub fn start_reducer(&mut self, name: &str, ts: Timestamp) {
        self.reducer_start = Instant::now();
        name.clone_into(&mut self.reducer_name);
        self.instance_env.start_reducer(ts);
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    pub fn reducer_name(&self) -> &str {
        &self.reducer_name
    }

    /// Returns the name of the most recent reducer to be run in this environment,
    /// or `None` if no reducer is actively being invoked.
    fn log_record_function(&self) -> Option<&str> {
        let function = self.reducer_name();
        (!function.is_empty()).then_some(function)
    }

    /// Returns the name of the most recent reducer to be run in this environment.
    pub fn reducer_start(&self) -> Instant {
        self.reducer_start
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is over.
    /// This resets all of the state associated to a single reducer call,
    /// and returns instrumentation records.
    pub fn finish_reducer(&mut self) -> ExecutionTimings {
        let total_duration = self.reducer_start.elapsed();

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        }
    }

    /// Returns the [`ReplicaContext`] for this environment.
    fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        &self.instance_env.replica_ctx
    }
}

pub struct JsInstance {
    /// Information common to instances of all runtimes.
    ///
    /// (The type is shared, the data is not.)
    common: InstanceCommon,

    /// The environment of the instance.
    instance: JsInstanceEnvSlot,

    /// The module's program (JS code).
    /// Used to startup the [`Isolate`]s.
    ///
    // TODO(v8): replace with snapshots.
    program: Arc<str>,
}

impl JsInstance {
    pub fn trapped(&self) -> bool {
        self.common.trapped
    }

    pub fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let replica_ctx = self.instance.get_mut().replica_ctx();
        self.common
            .update_database(replica_ctx, program, old_module_info, policy)
    }

    pub fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> super::ReducerCallResult {
        let replica_ctx = &self.instance.get_mut().replica_ctx().clone();

        self.common
            .call_reducer_with_tx(replica_ctx, tx, params, log_traceback, |tx, op, budget| {
                /// Called by a thread separate to V8 execution
                /// every [`EPOCH_TICKS_PER_SECOND`] ticks (~every 1 second)
                /// to log that the reducer is still running.
                extern "C" fn cb_log_long_running(isolate: &mut Isolate, _: *mut c_void) {
                    let env = env_on_isolate(isolate);
                    let database = env.instance_env.replica_ctx.database_identity;
                    let reducer = env.reducer_name();
                    let dur = env.reducer_start().elapsed();
                    tracing::warn!(reducer, ?database, "JavaScript has been running for {dur:?}");
                }

                // TODO(v8): snapshots
                // Prepare the isolate with the env.
                let mut isolate = Isolate::new(<_>::default());
                self.instance.move_to_isolate(&mut isolate);

                // Start the budget timeout and long-running logger.
                let handle = isolate.thread_safe_handle();
                let (tx, call_result, timings) =
                    with_timeout_and_cb_every(handle, EPOCH_TICKS_PER_SECOND, cb_log_long_running, budget, || {
                        // Enter the scope.
                        with_scope(&mut isolate, |scope| {
                            let res = catch_exception(scope, |scope| {
                                // Prepare the JS module that has `__call_reducer__`.
                                eval_user_module(scope, &self.program)?;
                                Ok(())
                            })
                            .map_err(anyhow::Error::from);

                            let env = env_on_isolate(scope);

                            // Start the timer.
                            // We'd like this tightly around `__call_reducer__`.
                            env.start_reducer(op.name, op.timestamp);

                            // Call `__call_reducer__` with `tx` provided.
                            // It should not be available before.
                            let (tx, call_result) = match res {
                                Ok(()) => env.instance_env.tx.clone().set(tx, || {
                                    catch_exception(scope, |scope| {
                                        let res = call_call_reducer_from_op(scope, op)?;
                                        Ok(res)
                                    })
                                    .map_err(anyhow::Error::from)
                                }),
                                Err(err) => (tx, Err(err)),
                            };

                            // Finish timings.
                            let timings = env_on_isolate(scope).finish_reducer();

                            (tx, call_result, timings)
                        })
                    });

                // Steal back the env.
                self.instance.take_from_isolate(&mut isolate);

                // Derive energy stats.
                let energy = energy_from_elapsed(budget, timings.total_duration);

                // Fetch the currently used heap size in V8.
                // The used size is ostensibly fairer than the total size.
                let memory_allocation = isolate.get_heap_statistics().used_heap_size();

                let exec_result = ExecuteResult {
                    energy,
                    timings,
                    memory_allocation,
                    call_result,
                };
                (tx, exec_result)
            })
    }
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

/// Compile, instantiate, and evaluate `code` as a module.
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

/// Compile, instantiate, and evaluate the user module with `code`.
fn eval_user_module<'scope>(
    scope: &PinScope<'scope, '_>,
    code: &str,
) -> ExcResult<(Local<'scope, v8::Module>, Local<'scope, v8::Promise>)> {
    let name = str_from_ident!(spacetimedb_module).string(scope).into();
    eval_module(scope, name, 0, code, resolve_sys_module)
}

/// Runs `logic` on `isolate`, providing the former with a [`PinScope`].
pub(crate) fn with_scope<R>(isolate: &mut OwnedIsolate, logic: impl FnOnce(&mut PinScope<'_, '_>) -> R) -> R {
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 1024);

    scope!(let scope, isolate);
    let context = Context::new(scope, Default::default());
    let scope = &mut ContextScope::new(scope, context);
    logic(scope)
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

/// Calls the `__call_reducer__` hook, if it's been registered.
fn call_call_reducer_from_op<'scope>(scope: &mut PinScope<'scope, '_>, op: ReducerOp<'_>) -> ExcResult<ReducerResult> {
    syscall::call_call_reducer(scope, op)
}

/// Extracts the raw module def by running `__describe_module__` in `program`.
fn extract_description(program: &str) -> Result<RawModuleDef, DescribeError> {
    let budget = ReducerBudget::DEFAULT_BUDGET;
    let callback_every = EPOCH_TICKS_PER_SECOND;
    extern "C" fn callback(_: &mut Isolate, _: *mut c_void) {}

    let mut isolate = Isolate::new(<_>::default());
    let handle = isolate.thread_safe_handle();
    with_timeout_and_cb_every(handle, callback_every, callback, budget, || {
        with_scope(&mut isolate, |scope| {
            catch_exception(scope, |scope| {
                eval_user_module(scope, program)?;
                Ok(())
            })
            .map_err(Into::into)
            .map_err(DescribeError::Setup)?;

            run_describer(log_traceback, || {
                catch_exception(scope, |scope| {
                    let def = call_describe_module(scope)?;
                    Ok(def)
                })
                .map_err(Into::into)
            })
        })
    })
}

/// Calls the `__describe_module__` hook, if it's been registered.
fn call_describe_module<'scope>(scope: &mut PinScope<'scope, '_>) -> ExcResult<RawModuleDef> {
    syscall::call_describe_module(scope)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::host::v8::to_value::test::with_scope;
    use crate::host::ArgsTuple;
    use spacetimedb_lib::{ConnectionId, Identity};
    use spacetimedb_primitives::ReducerId;
    use v8::{Local, Object};

    fn with_module<R>(
        code: &str,
        logic: impl for<'scope> FnOnce(&mut PinScope<'scope, '_>, Local<'scope, Object>) -> R,
    ) -> R {
        with_scope(|scope| {
            let (module, _) = eval_user_module(scope, code).unwrap();
            let ns = module.get_module_namespace().cast::<Object>();
            logic(scope, ns)
        })
    }

    fn with_module_catch<T>(
        code: &str,
        logic: impl for<'scope> FnOnce(&mut PinScope<'scope, '_>, Local<'scope, Object>) -> ExcResult<T>,
    ) -> anyhow::Result<T> {
        with_module(code, |scope, ns| {
            catch_exception(scope, |scope| {
                let ret = logic(scope, ns)?;
                Ok(ret)
            })
            .map_err(anyhow::Error::from)
        })
    }

    #[test]
    fn call_call_reducer_works() {
        let call = |code| {
            with_module_catch(code, |scope, _| {
                call_call_reducer_from_op(
                    scope,
                    ReducerOp {
                        id: ReducerId(42),
                        name: "foobar",
                        caller_identity: &Identity::ONE,
                        caller_connection_id: &ConnectionId::ZERO,
                        timestamp: Timestamp::from_micros_since_unix_epoch(24),
                        args: &ArgsTuple::nullary(),
                    },
                )
            })
        };

        // Test the trap case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({ __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                throw new Error("foobar");
            } })
        "#,
        );
        let actual = format!("{}", ret.expect_err("should trap")).replace("\t", "    ");
        let expected = r#"
js error Uncaught Error: foobar
    at __call_reducer__ (spacetimedb_module:4:23)
        "#;
        assert_eq!(actual.trim(), expected.trim());

        // Test the error case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({ __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                return {
                    "tag": "err",
                    "value": "foobar",
                };
            } })
        "#,
        );
        assert_eq!(&*ret.expect("should not trap").expect_err("should error"), "foobar");

        // Test the error case.
        let ret = call(
            r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({ __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                return {
                    "tag": "ok",
                    "value": {},
                };
            } })
        "#,
        );
        ret.expect("should not trap").expect("should not error");
    }

    #[test]
    fn call_describe_module_works() {
        let code = r#"
            import { register_hooks } from "spacetime:sys@1.0";
            register_hooks({ __describe_module__() {
                return new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            } })
        "#;
        let raw_mod = with_module_catch(code, |scope, _| call_describe_module(scope)).map_err(|e| e.to_string());
        assert_eq!(raw_mod, Ok(RawModuleDef::V9(<_>::default())));
    }
}
