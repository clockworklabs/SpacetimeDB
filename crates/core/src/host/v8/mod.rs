#![allow(dead_code)]

use super::module_common::{build_common_module_from_raw, ModuleCommon};
use super::module_host::{CallReducerParams, DynModule, Module, ModuleInfo, ModuleInstance, ModuleRuntime};
use super::{ReducerCallResult, Scheduler, UpdateDatabaseResult};
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::host::wasm_common::instrumentation::CallTimes;
use crate::host::wasm_common::module_host_actor::{
    EnergyStats, ExecuteResult, ExecutionTimings, InstanceCommon, ReducerOp,
};
use crate::host::ArgsTuple;
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use anyhow::anyhow;
use core::time::Duration;
use de::deserialize_js;
use error::{catch_exception, exception_already_thrown, ExcResult, Throwable};
use de::{deserialize_js, scratch_buf};
use error::{
    catch_exception, exception_already_thrown, ErrorOrException, ExcResult, ExceptionThrown, ExceptionValue, Throwable,
    TypeError,
};
use from_value::cast;
use key_cache::get_or_create_key_cache;
use ser::serialize_to_js;
use spacetimedb_client_api_messages::energy::{EnergyQuanta, ReducerBudget};
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::traits::Program;
use spacetimedb_lib::{ConnectionId, Identity, RawModuleDef};
use std::sync::{Arc, LazyLock};
use v8::{Context, ContextOptions, ContextScope, Function, HandleScope, Isolate, Local, Value};
use util::{ascii_str, module, strings};
use v8::{
    CallbackScope, Context, ContextOptions, ContextScope, ExternalReference, FixedArray, Function,
    FunctionCallbackArguments, FunctionCodeHandling, Global, HandleScope, Isolate, IsolateHandle, Local, MapFnTo,
    ModuleStatus, ObjectTemplate, OwnedIsolate, ResolveModuleCallback, ScriptOrigin, Value,
};

mod de;
mod error;
mod from_value;
mod key_cache;
mod ser;
mod to_value;
mod util;

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

        if true {
            return Err::<JsModule, _>(anyhow!("v8_todo"));
        }

        let program = std::str::from_utf8(&mcc.program.bytes)?;
        let (snapshot, desc) = compile(program, Arc::new(Logger))?;

        // Validate and create a common module rom the raw definition.
        let common = build_common_module_from_raw(mcc, desc)?;

        Ok(JsModule { common, snapshot })
    }
}

#[derive(Clone)]
pub struct JsModule {
    snapshot: Arc<[u8]>,
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

struct Logger;
impl Logger {
    fn write(&self, level: LogLevel, record: &Record<'_>, _bt: &dyn BacktraceProvider) {
        eprintln!(
            "{level:?} [{}] [{}:{}] {}",
            record.ts,
            record.filename.unwrap_or(""),
            record.line_number.unwrap_or(0),
            record.message,
        )
    }
}

#[repr(usize)]
enum GlobalInternalField {
    WrapperModule,
    Last,
}

// fn builtins_snapshot() -> anyhow::Result<Arc<[u8]>> {
fn builtins_snapshot() -> anyhow::Result<(OwnedIsolate, Global<Context>)> {
    let isolate = Isolate::snapshot_creator(Some(extern_refs().into()), None);
    let mut isolate = scopeguard::guard(isolate, |isolate| {
        // rusty_v8 panics if we don't call this when dropping isolate
        isolate.create_blob(FunctionCodeHandling::Clear);
    });
    let context = {
        let isolate = &mut *isolate;
        let handle_scope = &mut HandleScope::new(isolate);

        let global_template = ObjectTemplate::new(handle_scope);
        global_template.set_internal_field_count(GlobalInternalField::Last as usize);
        let context = Context::new(
            handle_scope,
            ContextOptions {
                global_template: Some(global_template),
                ..Default::default()
            },
        );

        let scope = &mut ContextScope::new(handle_scope, context);
        scope.set_default_context(context);
        assert_eq!(scope.add_context(context), 0);
        let global = context.global(scope);
        // scope.add_context_data(context, global);
        // scope.add_context_
        // scope.get_current_context().set_slot(logger);
        catch_exception(scope, |scope| {
            let name = ascii_str!("spacetime:wrapper").string(scope).into();
            let module = init_module(scope, name, 0, include_str!("./wrapper.ts"), resolve_internal_module)?;

            // this is hacky
            global.set_internal_field(GlobalInternalField::WrapperModule as usize, module.into());

            Ok(())
        })?;
        Global::new(scope, context)
    };

    // let snapshot = scopeguard::ScopeGuard::into_inner(isolate)
    //     .create_blob(v8::FunctionCodeHandling::Clear)
    //     .unwrap();

    // Ok((*snapshot).into())

    Ok((scopeguard::ScopeGuard::into_inner(isolate), context))
}

fn compile(program: &str, logger: Arc<Logger>) -> anyhow::Result<(Arc<[u8]>, RawModuleDef)> {
    // let builtins = builtins_snapshot()?;
    // let isolate = v8::Isolate::snapshot_creator_from_existing_snapshot(builtins, Some(&EXTERN_REFS), None);
    let (isolate, context) = builtins_snapshot()?;
    let mut isolate = scopeguard::guard(isolate, |isolate| {
        // rusty_v8 panics if we don't call this when dropping isolate
        isolate.create_blob(FunctionCodeHandling::Keep);
    });
    isolate.set_slot(logger.clone());

    let module_def = {
        let isolate = &mut *isolate;
        let handle_scope = &mut HandleScope::new(isolate);
        // let context = v8::Context::from_snapshot(handle_scope, 0, Default::default()).unwrap();
        let context = Local::new(handle_scope, context);
        let scope = &mut ContextScope::new(handle_scope, context);

        // scope.set_prepare_stack_trace_callback(prepare_stack_trace);

        // scope.set_default_context(context);

        catch_exception(scope, |scope| {
            let name = ascii_str!("spacetime:module").string(scope).into();
            init_module(scope, name, 1, program, resolve_wrapper_module)?;
            Ok(())
        })?;

        call_describe_module(scope)?
    };

    let snapshot = scopeguard::ScopeGuard::into_inner(isolate)
        .create_blob(FunctionCodeHandling::Keep)
        .unwrap();
    // d923b61bd4a4a000589af55b9ac5f046e97c4c756c96427fbc24d1253e7c9c77
    // dbg!(spacetimedb_lib::hash_bytes(&snapshot));
    let snapshot = <Arc<[u8]>>::from(&*snapshot);

    Ok((snapshot, module_def))
}

fn find_source_map(program: &str) -> Option<&str> {
    let sm_ref = "//# sourceMappingURL=";
    program.match_indices(sm_ref).find_map(|(i, _)| {
        let (before, after) = program.split_at(i);
        (before.is_empty() || before.ends_with(['\r', '\n']))
            .then(|| &after.lines().next().unwrap_or(after)[sm_ref.len()..])
    })
}

fn init_module<'s>(
    scope: &mut HandleScope<'s>,
    resource_name: Local<'s, Value>,
    script_id: i32,
    program: &str,
    resolve_module: impl MapFnTo<ResolveModuleCallback<'s>>,
) -> Result<Local<'s, v8::Module>, ErrorOrException<ExceptionThrown>> {
    let source = v8::String::new(scope, program).ok_or_else(exception_already_thrown)?;
    let source_map_url = find_source_map(program).map(|r| v8::String::new(scope, r).unwrap().into());
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
    let source = &mut v8::script_compiler::Source::new(source, Some(&origin));
    let module = v8::script_compiler::compile_module(scope, source).ok_or_else(exception_already_thrown)?;

    module
        .instantiate_module(scope, resolve_module)
        .ok_or_else(exception_already_thrown)?;

    module.evaluate(scope).ok_or_else(exception_already_thrown)?;

    if module.get_status() == ModuleStatus::Errored {
        let exc = ExceptionValue(Local::new(scope, module.get_exception()));
        Err(exc.throw(scope))?;
    }

    Ok(module)
}

fn resolve_internal_module<'s>(
    context: Local<'s, Context>,
    spec: Local<'s, v8::String>,
    _attrs: Local<'s, FixedArray>,
    _referrer: Local<'s, v8::Module>,
) -> Option<Local<'s, v8::Module>> {
    let scope = &mut *unsafe { CallbackScope::new(context) };
    if spec == spacetime_sys_10_0::SPEC_STRING.string(scope) {
        Some(spacetime_sys_10_0::make(scope))
    } else {
        module_exception(scope, spec).throw(scope);
        None
    }
}

strings!(SPACETIME_MODULE = "spacetimedb");

fn resolve_wrapper_module<'s>(
    context: Local<'s, Context>,
    spec: Local<'s, v8::String>,
    _attrs: Local<'s, FixedArray>,
    _referrer: Local<'s, v8::Module>,
) -> Option<Local<'s, v8::Module>> {
    let scope = &mut *unsafe { CallbackScope::new(context) };
    if spec == SPACETIME_MODULE.string(scope) {
        let module = context
            .global(scope)
            .get_internal_field(scope, GlobalInternalField::WrapperModule as usize)
            .unwrap()
            .cast::<v8::Module>();
        Some(module)
    } else {
        module_exception(scope, spec).throw(scope);
        None
    }
}

fn module_exception(scope: &mut HandleScope<'_>, spec: Local<'_, v8::String>) -> TypeError<String> {
    let mut buf = scratch_buf::<32>();
    let spec = spec.to_rust_cow_lossy(scope, &mut buf);
    TypeError(format!("Could not find module {spec:?}"))
}

module!(
    spacetime_sys_10_0 = "spacetime:sys/v10.0",
    function(console_log),
    symbol(console_level_error = "console.Level.Error"),
    symbol(console_level_warn = "console.Level.Warn"),
    symbol(console_level_info = "console.Level.Info"),
    symbol(console_level_debug = "console.Level.Debug"),
    symbol(console_level_trace = "console.Level.Trace"),
    symbol(console_level_panic = "console.Level.Panic"),
);

fn extern_refs() -> Vec<ExternalReference> {
    spacetime_sys_10_0::external_refs()
        .chain(Some(ExternalReference {
            pointer: std::ptr::null_mut(),
        }))
        .collect()
}

fn console_log(scope: &mut HandleScope<'_>, args: FunctionCallbackArguments<'_>) -> ExcResult<()> {
    let logger = scope.get_slot::<Arc<Logger>>().unwrap().clone();
    let level = args.get(0);
    let level = if level == spacetime_sys_10_0::console_level_error(scope) {
        LogLevel::Error
    } else if level == spacetime_sys_10_0::console_level_warn(scope) {
        LogLevel::Warn
    } else if level == spacetime_sys_10_0::console_level_info(scope) {
        LogLevel::Info
    } else if level == spacetime_sys_10_0::console_level_debug(scope) {
        LogLevel::Debug
    } else if level == spacetime_sys_10_0::console_level_trace(scope) {
        LogLevel::Trace
    } else if level == spacetime_sys_10_0::console_level_panic(scope) {
        LogLevel::Panic
    } else {
        return Err(TypeError(ascii_str!("Invalid log level")).throw(scope));
    };
    let msg = args.get(1).cast::<v8::String>();
    let mut buf = scratch_buf::<128>();
    let msg = msg.to_rust_cow_lossy(scope, &mut buf);
    let frame: Local<'_, v8::StackFrame> = v8::StackTrace::current_stack_trace(scope, 2)
        .ok_or_else(exception_already_thrown)?
        .get_frame(scope, 1)
        .ok_or_else(exception_already_thrown)?;
    let mut buf = scratch_buf::<32>();
    let filename = frame
        .get_script_name(scope)
        .map(|s| s.to_rust_cow_lossy(scope, &mut buf));
    let record = Record {
        // TODO: figure out whether to use walltime now or logical reducer now (env.reducer_start)
        ts: chrono::Utc::now(),
        target: None,
        filename: filename.as_deref(),
        line_number: Some(frame.get_line_number() as u32),
        message: &msg,
    };
    logger.write(level, &record, &());
    Ok(())
}

#[test]
fn v8_compile_test() {
    let program = include_str!("./test_code.ts");
    let (_snapshot, module) = compile(program, Arc::new(Logger)).unwrap();
    dbg!(module);
    // dbg!(module_idx, bytes::Bytes::copy_from_slice(&snapshot));
    // panic!();
}

fn _request_interrupt<F>(handle: &IsolateHandle, f: F) -> bool
where
    F: FnOnce(&mut Isolate),
{
    unsafe extern "C" fn cb<F>(isolate: &mut Isolate, data: *mut std::ffi::c_void)
    where
        F: FnOnce(&mut Isolate),
    {
        let f = unsafe { Box::<F>::from_raw(data.cast()) };
        f(isolate)
    }
    let data = Box::into_raw(Box::new(f));
    let already_destroyed = handle.request_interrupt(cb::<F>, data.cast());
    if already_destroyed {
        drop(unsafe { Box::from_raw(data) });
    }
    already_destroyed
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
        todo!()
    }
}

pub struct JsInstance {
    common: InstanceCommon,
    replica_ctx: Arc<ReplicaContext>,
}

#[allow(unused)]
impl ModuleInstance for JsInstance {
    fn trapped(&self) -> bool {
        self.common.trapped
    }

    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let replica_ctx = &self.replica_ctx;
        self.common.update_database(replica_ctx, program, old_module_info)
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> super::ReducerCallResult {
        // TODO(centril): snapshots, module->host calls
        let mut isolate = Isolate::new(<_>::default());
        let scope = &mut HandleScope::new(&mut isolate);
        let context = Context::new(scope, ContextOptions::default());
        let scope = &mut ContextScope::new(scope, context);

        self.common.call_reducer_with_tx(
            &self.replica_ctx.clone(),
            tx,
            params,
            // TODO(centril): logging.
            |_ty, _fun, _err| {},
            |tx, op, _budget| {
                let call_result = call_call_reducer_from_op(scope, op);
                // TODO(centril): energy metrering.
                let energy = EnergyStats {
                    used: EnergyQuanta::ZERO,
                    wasmtime_fuel_used: 0,
                    remaining: ReducerBudget::ZERO,
                };
                // TODO(centril): timings.
                let timings = ExecutionTimings {
                    total_duration: Duration::ZERO,
                    wasm_instance_env_call_times: CallTimes::new(),
                };
                let exec_result = ExecuteResult {
                    energy,
                    timings,
                    // TODO(centril): memory allocation.
                    memory_allocation: 0,
                    call_result,
                };
                (tx, exec_result)
            },
        )
    }
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
