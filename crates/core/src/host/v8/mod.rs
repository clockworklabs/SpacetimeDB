#![allow(dead_code)]

use super::module_common::{build_common_module_from_raw, ModuleCommon};
use super::module_host::{CallReducerParams, DynModule, Module, ModuleInfo, ModuleInstance, ModuleRuntime};
use super::UpdateDatabaseResult;
use crate::host::instance_env::InstanceEnv;
use crate::host::module_common::run_describer;
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    DescribeError, EnergyStats, ExecuteResult, ExecutionTimings, InstanceCommon, ReducerOp,
};
use crate::host::wasmtime::{epoch_ticker, ticks_in_duration, EPOCH_TICKS_PER_SECOND};
use crate::host::ArgsTuple;
use crate::{host::Scheduler, module_host_context::ModuleCreationContext, replica_context::ReplicaContext};
use core::ffi::c_void;
use core::iter;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use core::{ptr, str};
use de::deserialize_js;
use error::{catch_exception, exception_already_thrown, log_traceback, ExcResult, FnRet, Throwable};
use from_value::cast;
use key_cache::get_or_create_key_cache;
use ser::serialize_to_js;
use spacetimedb_client_api_messages::energy::ReducerBudget;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{ConnectionId, Identity, RawModuleDef, Timestamp};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use v8::{
    Context, ContextOptions, ContextScope, Function, HandleScope, Isolate, IsolateHandle, Local, Object, OwnedIsolate,
    Value,
};

mod de;
mod error;
mod from_value;
mod key_cache;
mod ser;
mod to_value;

/// The V8 runtime, for modules written in e.g., JS or TypeScript.
#[derive(Default)]
pub struct V8Runtime {
    _priv: (),
}

impl ModuleRuntime for V8Runtime {
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
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
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
        #![allow(unreachable_code, unused_variables)]

        log::trace!(
            "Making new V8 module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program.hash,
        );

        if true {
            return Err::<JsModule, _>(anyhow::anyhow!("v8_todo"));
        }

        // TODO(v8): determine min required ABI by module and check that it's supported?

        // TODO(v8): validate function signatures like in WASM? Is that possible with V8?

        // Convert program to a string.
        let program: Arc<str> = str::from_utf8(&mcc.program.bytes)?.into();

        // Run the program as a script and extract the raw module def.
        let desc = extract_description(&program)?;

        // Validate and create a common module rom the raw definition.
        let common = build_common_module_from_raw(mcc, desc)?;

        Ok(JsModule { common, program })
    }
}

#[derive(Clone)]
struct JsModule {
    common: ModuleCommon,
    program: Arc<str>,
}

impl DynModule for JsModule {
    fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        self.common.replica_ctx()
    }

    fn scheduler(&self) -> &Scheduler {
        self.common.scheduler()
    }
}

impl Module for JsModule {
    type Instance = JsInstance;

    type InitialInstances<'a> = iter::Empty<JsInstance>;

    fn initial_instances(&mut self) -> Self::InitialInstances<'_> {
        iter::empty()
    }

    fn info(&self) -> Arc<ModuleInfo> {
        self.common.info().clone()
    }

    fn create_instance(&self) -> Self::Instance {
        // TODO(v8): do we care about preinits / setup or are they unnecessary?

        let common = &self.common;
        let instance_env = InstanceEnv::new(common.replica_ctx().clone(), common.scheduler().clone());
        let instance = Some(JsInstanceEnv {
            instance_env,
            reducer_start: Instant::now(),
            call_times: CallTimes::new(),
            reducer_name: String::from("<initializing>"),
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

const EXPECT_ENV: &str = "there should be a `JsInstanceEnv`";

fn env_on_isolate(isolate: &mut Isolate) -> &mut JsInstanceEnv {
    isolate.get_slot_mut().expect(EXPECT_ENV)
}

fn env_on_instance(inst: &mut JsInstance) -> &mut JsInstanceEnv {
    inst.instance.as_mut().expect(EXPECT_ENV)
}

struct JsInstanceEnv {
    instance_env: InstanceEnv,

    /// The point in time the last reducer call started at.
    reducer_start: Instant,

    /// Track time spent in all wasm instance env calls (aka syscall time).
    ///
    /// Each function, like `insert`, will add the `Duration` spent in it
    /// to this tracker.
    call_times: CallTimes,

    /// The last, including current, reducer to be executed by this environment.
    reducer_name: String,
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
}

struct JsInstance {
    common: InstanceCommon,
    instance: Option<JsInstanceEnv>,
    program: Arc<str>,
}

impl ModuleInstance for JsInstance {
    fn trapped(&self) -> bool {
        self.common.trapped
    }

    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let replica_ctx = &env_on_instance(self).instance_env.replica_ctx.clone();
        self.common
            .update_database(replica_ctx, program, old_module_info, policy)
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> super::ReducerCallResult {
        let replica_ctx = env_on_instance(self).instance_env.replica_ctx.clone();

        self.common
            .call_reducer_with_tx(&replica_ctx, tx, params, log_traceback, |tx, op, budget| {
                let callback_every = EPOCH_TICKS_PER_SECOND;
                extern "C" fn callback(isolate: &mut Isolate, _: *mut c_void) {
                    let env = env_on_isolate(isolate);
                    let database = env.instance_env.replica_ctx.database_identity;
                    let reducer = env.reducer_name();
                    let dur = env.reducer_start().elapsed();
                    tracing::warn!(reducer, ?database, "Wasm has been running for {dur:?}");
                }

                // Prepare the isolate with the env.
                let mut isolate = Isolate::new(<_>::default());
                isolate.set_slot(self.instance.take().expect(EXPECT_ENV));

                // TODO(v8): snapshots, module->host calls
                // Call the reducer.
                env_on_isolate(&mut isolate).instance_env.start_reducer(op.timestamp);
                let (mut isolate, (tx, call_result)) =
                    with_script(isolate, &self.program, callback_every, callback, budget, |scope, _| {
                        let (tx, call_result) = env_on_isolate(scope)
                            .instance_env
                            .tx
                            .clone()
                            .set(tx, || call_call_reducer_from_op(scope, op));
                        (tx, call_result)
                    });
                let timings = env_on_isolate(&mut isolate).finish_reducer();
                self.instance = isolate.remove_slot();

                // Derive energy stats.
                let used = duration_to_budget(timings.total_duration);
                let remaining = budget - used;
                let energy = EnergyStats { budget, remaining };

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

fn with_script<R>(
    isolate: OwnedIsolate,
    code: &str,
    callback_every: u64,
    callback: IsolateCallback,
    budget: ReducerBudget,
    logic: impl for<'scope> FnOnce(&mut HandleScope<'scope>, Local<'scope, Value>) -> R,
) -> (OwnedIsolate, R) {
    with_scope(isolate, callback_every, callback, budget, |scope| {
        let code = v8::String::new(scope, code).unwrap();
        let script_val = v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();
        logic(scope, script_val)
    })
}

/// Sets up an isolate and run `logic` with a [`HandleScope`].
pub(crate) fn with_scope<R>(
    mut isolate: OwnedIsolate,
    callback_every: u64,
    callback: IsolateCallback,
    budget: ReducerBudget,
    logic: impl FnOnce(&mut HandleScope<'_>) -> R,
) -> (OwnedIsolate, R) {
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 1024);
    let isolate_handle = isolate.thread_safe_handle();
    let mut scope_1 = HandleScope::new(&mut isolate);
    let context = Context::new(&mut scope_1, ContextOptions::default());
    let mut scope_2 = ContextScope::new(&mut scope_1, context);

    let timeout_thread_cancel_flag = run_reducer_timeout(callback_every, callback, budget, isolate_handle);

    let ret = logic(&mut scope_2);
    drop(scope_2);
    drop(scope_1);

    // Cancel the execution timeout in `run_reducer_timeout`.
    timeout_thread_cancel_flag.store(true, Ordering::Relaxed);

    (isolate, ret)
}

type IsolateCallback = extern "C" fn(&mut Isolate, *mut c_void);

/// Spawns a thread that will terminate reducer execution
/// when `budget` has been used up.
///
/// Every `callback_every` ticks, `callback` is called.
fn run_reducer_timeout(
    callback_every: u64,
    callback: IsolateCallback,
    budget: ReducerBudget,
    isolate_handle: IsolateHandle,
) -> Arc<AtomicBool> {
    let execution_done_flag = Arc::new(AtomicBool::new(false));
    let execution_done_flag2 = execution_done_flag.clone();
    let timeout = budget_to_duration(budget);
    let max_ticks = ticks_in_duration(timeout);

    let mut num_ticks = 0;
    epoch_ticker(move || {
        // Check if execution completed.
        if execution_done_flag2.load(Ordering::Relaxed) {
            return None;
        }

        // We've reached the number of ticks to call `callback`.
        if num_ticks % callback_every == 0 && isolate_handle.request_interrupt(callback, ptr::null_mut()) {
            return None;
        }

        if num_ticks == max_ticks {
            // Execution still ongoing while budget has been exhausted.
            // Terminate V8 execution.
            // This implements "gas" for v8.
            isolate_handle.terminate_execution();
        }

        num_ticks += 1;
        Some(())
    });

    execution_done_flag
}

/// Converts a [`ReducerBudget`] to a [`Duration`].
fn budget_to_duration(_budget: ReducerBudget) -> Duration {
    // TODO(v8): This is fake logic that allows a maximum timeout.
    // Replace with sensible math.
    Duration::MAX
}

/// Converts a [`Duration`] to a [`ReducerBudget`].
fn duration_to_budget(_duration: Duration) -> ReducerBudget {
    // TODO(v8): This is fake logic that allows minimum energy usage.
    // Replace with sensible math.
    ReducerBudget::ZERO
}

/// Returns the global object.
fn global<'scope>(scope: &mut HandleScope<'scope>) -> Local<'scope, Object> {
    scope.get_current_context().global(scope)
}

/// Returns the global property `key`.
fn get_global_property<'scope>(scope: &mut HandleScope<'scope>, key: Local<'scope, v8::String>) -> FnRet<'scope> {
    global(scope)
        .get(scope, key.into())
        .ok_or_else(exception_already_thrown)
}

/// Calls free function `fun` with `args`.
fn call_free_fun<'scope>(
    scope: &mut HandleScope<'scope>,
    fun: Local<'scope, Function>,
    args: &[Local<'scope, Value>],
) -> ExcResult<Local<'scope, Value>> {
    let receiver = v8::undefined(scope).into();
    fun.call(scope, receiver, args).ok_or_else(exception_already_thrown)
}

// Calls the `__call_reducer__` function on the global proxy object using `op`.
fn call_call_reducer_from_op(scope: &mut HandleScope<'_>, op: ReducerOp<'_>) -> anyhow::Result<Result<(), Box<str>>> {
    call_call_reducer(
        scope,
        op.id.into(),
        op.caller_identity,
        op.caller_connection_id,
        op.timestamp.to_micros_since_unix_epoch(),
        op.args,
    )
}

// Calls the `__call_reducer__` function on the global proxy object.
fn call_call_reducer(
    scope: &mut HandleScope<'_>,
    reducer_id: u32,
    sender: &Identity,
    conn_id: &ConnectionId,
    timestamp: i64,
    reducer_args: &ArgsTuple,
) -> anyhow::Result<Result<(), Box<str>>> {
    // Get a cached version of the `__call_reducer__` property.
    let key_cache = get_or_create_key_cache(scope);
    let call_reducer_key = key_cache.borrow_mut().call_reducer(scope);

    catch_exception(scope, |scope| {
        // Serialize the arguments.
        let reducer_id = serialize_to_js(scope, &reducer_id)?;
        let sender = serialize_to_js(scope, &sender.to_u256())?;
        let conn_id: v8::Local<'_, v8::Value> = serialize_to_js(scope, &conn_id.to_u128())?;
        let timestamp = serialize_to_js(scope, &timestamp)?;
        let reducer_args = serialize_to_js(scope, &reducer_args.tuple.elements)?;
        let args = &[reducer_id, sender, conn_id, timestamp, reducer_args];

        // Get the function on the global proxy object and convert to a function.
        let object = get_global_property(scope, call_reducer_key)?;
        let fun =
            cast!(scope, object, Function, "function export for `__call_reducer__`").map_err(|e| e.throw(scope))?;

        // Call the function.
        let ret = call_free_fun(scope, fun, args)?;

        // Deserialize the user result.
        let user_res = deserialize_js(scope, ret)?;

        Ok(user_res)
    })
    .map_err(Into::into)
}

/// Extracts the raw module def by running `__describe_module__` in `program`.
fn extract_description(program: &str) -> Result<RawModuleDef, DescribeError> {
    let budget = ReducerBudget::DEFAULT_BUDGET;
    let callback_every = EPOCH_TICKS_PER_SECOND;
    extern "C" fn callback(_: &mut Isolate, _: *mut c_void) {}

    let (_, ret) = with_script(
        Isolate::new(<_>::default()),
        program,
        callback_every,
        callback,
        budget,
        |scope, _| run_describer(log_traceback, || call_describe_module(scope)),
    );
    ret
}

// Calls the `__describe_module__` function on the global proxy object to extract a [`RawModuleDef`].
fn call_describe_module(scope: &mut HandleScope<'_>) -> anyhow::Result<RawModuleDef> {
    // Get a cached version of the `__describe_module__` property.
    let key_cache = get_or_create_key_cache(scope);
    let describe_module_key = key_cache.borrow_mut().describe_module(scope);

    catch_exception(scope, |scope| {
        // Get the function on the global proxy object and convert to a function.
        let object = get_global_property(scope, describe_module_key)?;
        let fun =
            cast!(scope, object, Function, "function export for `__describe_module__`").map_err(|e| e.throw(scope))?;

        // Call the function.
        let raw_mod_js = call_free_fun(scope, fun, &[])?;

        // Deserialize the raw module.
        let raw_mod: RawModuleDef = deserialize_js(scope, raw_mod_js)?;
        Ok(raw_mod)
    })
    .map_err(Into::into)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::host::v8::to_value::test::with_scope;
    use v8::{Local, Value};

    fn with_script<R>(
        code: &str,
        logic: impl for<'scope> FnOnce(&mut HandleScope<'scope>, Local<'scope, Value>) -> R,
    ) -> R {
        with_scope(|scope| {
            let code = v8::String::new(scope, code).unwrap();
            let script_val = v8::Script::compile(scope, code, None).unwrap().run(scope).unwrap();
            logic(scope, script_val)
        })
    }

    #[test]
    fn call_call_reducer_works() {
        let call = |code| {
            with_script(code, |scope, _| {
                call_call_reducer(
                    scope,
                    42,
                    &Identity::ONE,
                    &ConnectionId::ZERO,
                    24,
                    &ArgsTuple::nullary(),
                )
            })
        };

        // Test the trap case.
        let ret = call(
            r#"
            function __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                throw new Error("foobar");
            }
        "#,
        );
        let actual = format!("{}", ret.expect_err("should trap")).replace("\t", "    ");
        let expected = r#"
js error Uncaught Error: foobar
    at __call_reducer__ (<unknown location>:3:23)
        "#;
        assert_eq!(actual.trim(), expected.trim());

        // Test the error case.
        let ret = call(
            r#"
            function __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                return {
                    "tag": "err",
                    "value": "foobar",
                };
            }
        "#,
        );
        assert_eq!(&*ret.expect("should not trap").expect_err("should error"), "foobar");

        // Test the error case.
        let ret = call(
            r#"
            function __call_reducer__(reducer_id, sender, conn_id, timestamp, args) {
                return {
                    "tag": "ok",
                    "value": {},
                };
            }
        "#,
        );
        ret.expect("should not trap").expect("should not error");
    }

    #[test]
    fn call_describe_module_works() {
        let code = r#"
            function __describe_module__() {
                return {
                    "tag": "V9",
                    "value": {
                        "typespace": {
                            "types": [],
                        },
                        "tables": [],
                        "reducers": [],
                        "types": [],
                        "misc_exports": [],
                        "row_level_security": [],
                    },
                };
            }
        "#;
        let raw_mod = with_script(code, |scope, _| call_describe_module(scope).unwrap());
        assert_eq!(raw_mod, RawModuleDef::V9(<_>::default()));
    }
}
