#![allow(dead_code)]

use super::module_common::{build_common_module_from_raw, ModuleCommon};
use super::module_host::{CallReducerParams, DynModule, Module, ModuleInfo, ModuleInstance, ModuleRuntime};
use super::UpdateDatabaseResult;
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    EnergyStats, ExecuteResult, ExecutionTimings, InstanceCommon, ReducerOp,
};
use crate::host::ArgsTuple;
use crate::{host::Scheduler, module_host_context::ModuleCreationContext, replica_context::ReplicaContext};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use de::deserialize_js;
use error::{catch_exception, exception_already_thrown, log_traceback, ExcResult, Throwable};
use from_value::cast;
use key_cache::get_or_create_key_cache;
use ser::serialize_to_js;
use spacetimedb_client_api_messages::energy::ReducerBudget;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::RawModuleDef;
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_schema::auto_migrate::MigrationPolicy;
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::Instant;
use v8::{Context, ContextOptions, ContextScope, Function, HandleScope, Isolate, IsolateHandle, Local, Value};

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
    fn init() -> Self {
        // Our current configuration:
        // - will pick a number of worker threads for background jobs based on the num CPUs.
        // - does not allow idle tasks
        let platform = v8::new_default_platform(0, false).make_shared();
        // Initialize V8. Internally, this uses a global lock so it's safe that we don't.
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();

        Self { _priv: () }
    }

    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
        #![allow(unreachable_code, unused_variables)]

        log::trace!(
            "Making new V8 module host actor for database {} with module {}",
            mcc.replica_ctx.database_identity,
            mcc.program.hash,
        );

        // TODO(v8): determine min required ABI by module and check that it's supported?

        // TODO(v8): validate function signatures like in WASM? Is that possible with V8?

        let desc = todo!();
        // Validate and create a common module rom the raw definition.
        let common = build_common_module_from_raw(mcc, desc)?;

        Ok(JsModule { common })
    }
}

#[derive(Clone)]
struct JsModule {
    common: ModuleCommon,
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

    type InitialInstances<'a> = std::iter::Empty<JsInstance>;

    fn initial_instances(&mut self) -> Self::InitialInstances<'_> {
        std::iter::empty()
    }

    fn info(&self) -> Arc<ModuleInfo> {
        self.common.info().clone()
    }

    fn create_instance(&self) -> Self::Instance {
        // TODO(v8): consider some equivalent to `epoch_deadline_callback`
        // where we report `Js has been running for ...`.

        // TODO(v8): timeout things like `extract_description`.

        // TODO(v8): do we care about preinits / setup or are they unnecessary?

        // TODO(v8): create `InstanceEnv`.

        // TODO(v8): extract description.

        todo!()
    }
}

struct JsInstance {
    common: InstanceCommon,
    replica_ctx: Arc<ReplicaContext>,
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
        let replica_ctx = &self.replica_ctx;
        self.common
            .update_database(replica_ctx, program, old_module_info, policy)
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> super::ReducerCallResult {
        self.common.call_reducer_with_tx(
            &self.replica_ctx.clone(),
            tx,
            params,
            log_traceback,
            |tx, op, budget| {
                // TODO(v8): snapshots, module->host calls
                // Setup V8 scope.
                let mut isolate: v8::OwnedIsolate = Isolate::new(<_>::default());
                let isolate_handle = isolate.thread_safe_handle();
                let mut scope_1 = HandleScope::new(&mut isolate);
                let context = Context::new(&mut scope_1, ContextOptions::default());
                let mut scope_2 = ContextScope::new(&mut scope_1, context);

                let timeout_thread_cancel_flag = run_reducer_timeout(isolate_handle, budget);

                // Call the reducer.
                let start = Instant::now();
                let call_result = call_call_reducer_from_op(&mut scope_2, op);
                let total_duration = start.elapsed();
                // Cancel the execution timeout in `run_reducer_timeout`.
                timeout_thread_cancel_flag.store(true, Ordering::Relaxed);

                // Handle energy and timings.
                let used = duration_to_budget(total_duration);
                let remaining = budget - used;
                let energy = EnergyStats { budget, remaining };
                let timings = ExecutionTimings {
                    total_duration,
                    // TODO(v8): call times.
                    wasm_instance_env_call_times: CallTimes::new(),
                };

                // Fetch the currently used heap size in V8.
                // The used size is ostensibly fairer than the total size.
                drop(scope_2);
                drop(scope_1);
                let memory_allocation = isolate.get_heap_statistics().used_heap_size();

                let exec_result = ExecuteResult {
                    energy,
                    timings,
                    memory_allocation,
                    call_result,
                };
                (tx, exec_result)
            },
        )
    }
}

/// Spawns a thread that will terminate reducer execution
/// when `budget` has been used up.
fn run_reducer_timeout(isolate_handle: IsolateHandle, budget: ReducerBudget) -> Arc<AtomicBool> {
    let execution_done_flag = Arc::new(AtomicBool::new(false));
    let execution_done_flag2 = execution_done_flag.clone();
    let timeout = budget_to_duration(budget);

    // TODO(v8): Using an OS thread is a bit heavy handed...?
    thread::spawn(move || {
        // Sleep until the timeout.
        thread::sleep(timeout);

        if execution_done_flag2.load(Ordering::Relaxed) {
            // The reducer completed successfully.
            return;
        }

        // Reducer is still running.
        // Terminate V8 execution.
        isolate_handle.terminate_execution();
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

/// Returns the global property `key`.
fn get_global_property<'scope>(
    scope: &mut HandleScope<'scope>,
    key: Local<'scope, v8::String>,
) -> ExcResult<Local<'scope, Value>> {
    scope
        .get_current_context()
        .global(scope)
        .get(scope, key.into())
        .ok_or_else(exception_already_thrown)
}

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
