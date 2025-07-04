use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Once};

use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::{IsolationLevel, Program};
use crate::energy::EnergyMonitor;
use crate::execution_context::{ReducerContext, Workload};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;

use super::instance_env::InstanceEnv;
use super::module_host::{CallReducerParams, Module, ModuleInfo, ModuleInstance};
use super::{ReducerCallResult, Scheduler, UpdateDatabaseResult};

mod convert;
mod de;
mod ser;
mod util;

use indexmap::IndexMap;
use spacetimedb_client_api_messages::energy::EnergyQuanta;
use spacetimedb_lib::db::raw_def::v9::{sats_name_to_scoped_name, RawModuleDefV9, RawReducerDefV9, RawTypeDefV9};
use spacetimedb_lib::sats;
use util::{
    ascii_str, module, scratch_buf, strings, throw, ErrorOrException, ExcResult, ExceptionOptionExt, ExceptionThrown,
    ThrowableResultExt, TypeError,
};

pub struct V8Runtime {
    _priv: (),
}

impl V8Runtime {
    pub fn new() -> Self {
        static V8_INIT: Once = Once::new();
        V8_INIT.call_once(|| {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });
        Self { _priv: () }
    }

    pub fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<impl Module> {
        let program = std::str::from_utf8(&mcc.program.bytes)?;
        let (snapshot, mut module_builder) = compile(program, Arc::new(Logger))?;
        let info = ModuleInfo::new(
            module_builder.inner.try_into()?,
            mcc.replica_ctx.owner_identity,
            mcc.replica_ctx.database_identity,
            mcc.program.hash,
            // TODO:
            tokio::sync::broadcast::channel(0).0,
            mcc.replica_ctx.subscriptions.clone(),
        );
        module_builder.reducers.shrink_to_fit();
        Ok(JsModule {
            replica_context: mcc.replica_ctx,
            scheduler: mcc.scheduler,
            info,
            _energy_monitor: mcc.energy_monitor,
            snapshot,
            reducers: Arc::new(module_builder.reducers),
        })
    }
}

#[derive(Clone)]
pub struct JsModule {
    replica_context: Arc<ReplicaContext>,
    scheduler: Scheduler,
    info: Arc<ModuleInfo>,
    _energy_monitor: Arc<dyn EnergyMonitor>,
    snapshot: Arc<[u8]>,
    reducers: Arc<IndexMap<String, usize>>,
}

#[derive(thiserror::Error, Debug)]
#[error("js error: {msg:?}")]
struct JsError {
    msg: String,
}

impl JsError {
    fn from_caught(scope: &mut v8::TryCatch<'_, v8::HandleScope<'_>>) -> Self {
        match scope.message() {
            Some(msg) => Self {
                msg: msg.get(scope).to_rust_string_lossy(scope),
            },
            None => Self {
                msg: "unknown error".to_owned(),
            },
        }
    }
}

fn catch_exception<'s, T>(
    scope: &mut v8::HandleScope<'s>,
    f: impl FnOnce(&mut v8::HandleScope<'s>) -> Result<T, ErrorOrException>,
) -> Result<T, ErrorOrException<JsError>> {
    let scope = &mut v8::TryCatch::new(scope);
    f(scope).map_err(|e| match e {
        ErrorOrException::Err(e) => ErrorOrException::Err(e),
        ErrorOrException::Exception(ExceptionThrown) => ErrorOrException::Exception(JsError::from_caught(scope)),
    })
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

#[derive(Default, Debug)]
struct ModuleBuilder {
    /// The module definition.
    inner: RawModuleDefV9,
    /// The reducers of the module.
    reducers: IndexMap<String, usize>,
}

#[repr(usize)]
enum GlobalInternalField {
    WrapperModule,
    Last,
}

// fn builtins_snapshot() -> anyhow::Result<Arc<[u8]>> {
fn builtins_snapshot() -> anyhow::Result<(v8::OwnedIsolate, v8::Global<v8::Context>)> {
    let isolate = v8::Isolate::snapshot_creator(Some(extern_refs().into()), None);
    let mut isolate = scopeguard::guard(isolate, |isolate| {
        // rusty_v8 panics if we don't call this when dropping isolate
        isolate.create_blob(v8::FunctionCodeHandling::Clear);
    });
    let context = {
        let isolate = &mut *isolate;
        let handle_scope = &mut v8::HandleScope::new(isolate);

        let global_template = v8::ObjectTemplate::new(handle_scope);
        global_template.set_internal_field_count(GlobalInternalField::Last as usize);
        let context = v8::Context::new(
            handle_scope,
            v8::ContextOptions {
                global_template: Some(global_template),
                ..Default::default()
            },
        );

        let scope = &mut v8::ContextScope::new(handle_scope, context);
        scope.set_default_context(context);
        assert_eq!(scope.add_context(context), 0);
        let global = context.global(scope);
        // scope.add_context_data(context, global);
        // scope.add_context_
        // scope.get_current_context().set_slot(logger);
        catch_exception(scope, |scope| {
            let name = ascii_str!("spacetime:wrapper").string(scope).into();
            let module = init_module(scope, name, 0, include_str!("./wrapper.js"), resolve_internal_module)?;

            // this is hacky
            global.set_internal_field(GlobalInternalField::WrapperModule as usize, module.into());

            Ok(())
        })?;
        v8::Global::new(scope, context)
    };

    // let snapshot = scopeguard::ScopeGuard::into_inner(isolate)
    //     .create_blob(v8::FunctionCodeHandling::Clear)
    //     .unwrap();

    // Ok((*snapshot).into())

    Ok((scopeguard::ScopeGuard::into_inner(isolate), context))
}

fn compile(program: &str, logger: Arc<Logger>) -> anyhow::Result<(Arc<[u8]>, ModuleBuilder)> {
    // let builtins = builtins_snapshot()?;
    // let isolate = v8::Isolate::snapshot_creator_from_existing_snapshot(builtins, Some(&EXTERN_REFS), None);
    let (isolate, context) = builtins_snapshot()?;
    let mut isolate = scopeguard::guard(isolate, |isolate| {
        // rusty_v8 panics if we don't call this when dropping isolate
        isolate.create_blob(v8::FunctionCodeHandling::Keep);
    });
    isolate.set_slot(ModuleBuilder::default());
    isolate.set_slot(logger.clone());

    {
        let isolate = &mut *isolate;
        let handle_scope = &mut v8::HandleScope::new(isolate);
        // let context = v8::Context::from_snapshot(handle_scope, 0, Default::default()).unwrap();
        let context = v8::Local::new(handle_scope, context);
        let scope = &mut v8::ContextScope::new(handle_scope, context);

        // scope.set_prepare_stack_trace_callback(prepare_stack_trace);

        // scope.set_default_context(context);

        catch_exception(scope, |scope| {
            let name = ascii_str!("spacetime:module").string(scope).into();
            init_module(scope, name, 1, program, resolve_wrapper_module)?;
            Ok(())
        })?;
    }

    let module_builder = isolate.remove_slot::<ModuleBuilder>().unwrap();

    let snapshot = scopeguard::ScopeGuard::into_inner(isolate)
        .create_blob(v8::FunctionCodeHandling::Keep)
        .unwrap();
    // d923b61bd4a4a000589af55b9ac5f046e97c4c756c96427fbc24d1253e7c9c77
    // dbg!(spacetimedb_lib::hash_bytes(&snapshot));
    let snapshot = <Arc<[u8]>>::from(&*snapshot);

    Ok((snapshot, module_builder))
}

// fn prepare_stack_trace<'s>(
//     scope: &mut v8::HandleScope<'s>,
//     error: v8::Local<'_, v8::Value>,
//     sites: v8::Local<'_, v8::Array>,
// ) -> v8::Local<'s, v8::Value> {
//     error.
//     todo!();
// }

fn find_source_map(program: &str) -> Option<&str> {
    let sm_ref = "//# sourceMappingURL=";
    program.match_indices(sm_ref).find_map(|(i, _)| {
        let (before, after) = program.split_at(i);
        (before.is_empty() || before.ends_with(['\r', '\n']))
            .then(|| &after.lines().next().unwrap_or(after)[sm_ref.len()..])
    })
}

fn init_module<'s>(
    scope: &mut v8::HandleScope<'s>,
    resource_name: v8::Local<'s, v8::Value>,
    script_id: i32,
    program: &str,
    resolve_module: impl v8::MapFnTo<v8::ResolveModuleCallback<'s>>,
) -> Result<v8::Local<'s, v8::Module>, ErrorOrException> {
    let source = v8::String::new(scope, program).err()?;
    let source_map_url = find_source_map(program).map(|r| v8::String::new(scope, r).unwrap().into());
    let origin = v8::ScriptOrigin::new(
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
    let module = v8::script_compiler::compile_module(scope, source).err()?;

    module.instantiate_module(scope, resolve_module).err()?;

    module.evaluate(scope).err()?;

    if module.get_status() == v8::ModuleStatus::Errored {
        let exc = v8::Local::new(scope, module.get_exception());
        throw(scope, exc)?;
    }

    Ok(module)
}

fn resolve_internal_module<'s>(
    context: v8::Local<'s, v8::Context>,
    spec: v8::Local<'s, v8::String>,
    _attrs: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    let scope = &mut *unsafe { v8::CallbackScope::new(context) };
    if spec == spacetime_sys_10_0::SPEC_STRING.string(scope) {
        Some(spacetime_sys_10_0::make(scope))
    } else {
        let exc = module_exception(scope, spec);
        throw(scope, exc).ok()
    }
}

strings!(SPACETIME_MODULE = "spacetimedb");

fn resolve_wrapper_module<'s>(
    context: v8::Local<'s, v8::Context>,
    spec: v8::Local<'s, v8::String>,
    _attrs: v8::Local<'s, v8::FixedArray>,
    _referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    let scope = &mut *unsafe { v8::CallbackScope::new(context) };
    if spec == SPACETIME_MODULE.string(scope) {
        let module = context
            .global(scope)
            .get_internal_field(scope, GlobalInternalField::WrapperModule as usize)
            .unwrap()
            .cast::<v8::Module>();
        Some(module)
    } else {
        let exc = module_exception(scope, spec);
        throw(scope, exc).ok()
    }
}

fn module_exception(scope: &mut v8::HandleScope<'_>, spec: v8::Local<'_, v8::String>) -> TypeError<String> {
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
    function(register_reducer),
    function(register_type),
);

fn extern_refs() -> Vec<v8::ExternalReference> {
    spacetime_sys_10_0::external_refs()
        .chain(Some(v8::ExternalReference {
            pointer: std::ptr::null_mut(),
        }))
        .collect()
}

fn console_log(scope: &mut v8::HandleScope<'_>, args: v8::FunctionCallbackArguments<'_>) -> ExcResult<()> {
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
        throw(scope, TypeError(ascii_str!("Invalid log level")))?
    };
    let msg = args.get(1).cast::<v8::String>();
    let mut buf = scratch_buf::<128>();
    let msg = msg.to_rust_cow_lossy(scope, &mut buf);
    let frame = v8::StackTrace::current_stack_trace(scope, 2)
        .err()?
        .get_frame(scope, 1)
        .err()?;
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

fn get_or_create_key_cache(scope: &mut v8::HandleScope<'_>) -> Rc<RefCell<de::KeyCache>> {
    let context = scope.get_current_context();
    context.get_slot::<RefCell<de::KeyCache>>().unwrap_or_else(|| {
        let cache = Rc::default();
        context.set_slot(Rc::clone(&cache));
        cache
    })
}

fn deserialize_js_seed<'de, T: sats::de::DeserializeSeed<'de>>(
    scope: &mut v8::HandleScope<'de>,
    val: v8::Local<'_, v8::Value>,
    seed: T,
) -> ExcResult<T::Output> {
    let key_cache = get_or_create_key_cache(scope);
    let key_cache = &mut *key_cache.borrow_mut();
    let de = de::Deserializer::new(scope, val, key_cache);
    seed.deserialize(de).throw(scope)
}

fn deserialize_js<'de, T: sats::Deserialize<'de>>(
    scope: &mut v8::HandleScope<'de>,
    val: v8::Local<'_, v8::Value>,
) -> ExcResult<T> {
    deserialize_js_seed(scope, val, std::marker::PhantomData)
}

fn serialize_to_js<'s, T: sats::Serialize>(
    scope: &mut v8::HandleScope<'s>,
    value: &T,
) -> ExcResult<v8::Local<'s, v8::Value>> {
    let key_cache = get_or_create_key_cache(scope);
    let key_cache = &mut *key_cache.borrow_mut();
    value.serialize(ser::Serializer::new(scope, key_cache)).throw(scope)
}

fn register_reducer(scope: &mut v8::HandleScope<'_>, args: v8::FunctionCallbackArguments<'_>) -> ExcResult<()> {
    if scope.get_slot::<ModuleBuilder>().is_none() {
        throw(scope, TypeError(ascii_str!("You cannot dynamically register reducers")))?;
    }

    let name = args.get(0).cast::<v8::String>();
    let params = args.get(1);

    let params: sats::ProductType = deserialize_js(scope, params)?;

    let function = args
        .get(2)
        .try_cast::<v8::Function>()
        .map_err(|_| TypeError(ascii_str!("Third argument to register_reducer must be function")))
        .throw(scope)?;

    function.set_name(name);

    let name = name.to_rust_string_lossy(scope);

    let context = scope.get_current_context();
    let function_idx = scope.add_context_data(context, function);

    let module = scope.get_slot_mut::<ModuleBuilder>().unwrap();
    module.inner.reducers.push(RawReducerDefV9 {
        name: (&*name).into(),
        params,
        lifecycle: None,
    });
    match module.reducers.entry(name) {
        indexmap::map::Entry::Vacant(v) => {
            v.insert(function_idx);
        }
        indexmap::map::Entry::Occupied(o) => {
            let msg = format!("Reducer {:?} already registered", o.key());
            throw(scope, TypeError(msg))?;
        }
    }

    Ok(())
}

fn register_type(scope: &mut v8::HandleScope<'_>, args: v8::FunctionCallbackArguments<'_>) -> ExcResult<u32> {
    if scope.get_slot::<ModuleBuilder>().is_none() {
        throw(scope, TypeError(ascii_str!("You cannot dynamically register reducers")))?;
    }

    let name = args.get(0).cast::<v8::String>();
    let ty = args.get(1);

    let mut buf = scratch_buf::<32>();
    let name = name.to_rust_cow_lossy(scope, &mut buf);
    let name = sats_name_to_scoped_name(&name);

    let ty: sats::AlgebraicType = deserialize_js(scope, ty)?;

    let module = scope.get_slot_mut::<ModuleBuilder>().unwrap();
    let r = module.inner.typespace.add(ty);
    module.inner.types.push(RawTypeDefV9 {
        name,
        ty: r,
        custom_ordering: false,
    });

    Ok(r.0)
}

#[test]
fn v8_compile_test() {
    let program = include_str!("./test_code.js");
    let (_snapshot, module) = compile(program, Arc::new(Logger)).unwrap();
    dbg!(module);
    // dbg!(module_idx, bytes::Bytes::copy_from_slice(&snapshot));
    // panic!();
}

impl Module for JsModule {
    type Instance = JsInstance;

    type InitialInstances<'a> = std::iter::Empty<JsInstance>;

    fn initial_instances(&mut self) -> Self::InitialInstances<'_> {
        std::iter::empty()
    }

    fn info(&self) -> Arc<ModuleInfo> {
        self.info.clone()
    }

    fn create_instance(&self) -> Self::Instance {
        todo!()
    }

    fn replica_ctx(&self) -> &ReplicaContext {
        &self.replica_context
    }

    fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }
}

pub struct JsInstance {
    module: JsModule,
}

#[allow(unused)]
impl ModuleInstance for JsInstance {
    fn trapped(&self) -> bool {
        false
    }

    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        todo!()
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        let stdb = self.module.replica_context.relational_db.clone();
        let module_def = &self.module.info.module_def;
        let reducer_def = module_def.reducer_by_id(params.reducer_id);

        let tx = tx.unwrap_or_else(|| {
            stdb.begin_mut_tx(
                IsolationLevel::Serializable,
                Workload::Reducer(ReducerContext {
                    name: (&*reducer_def.name).into(),
                    caller_identity: params.caller_identity,
                    caller_connection_id: params.caller_connection_id,
                    timestamp: params.timestamp,
                    arg_bsatn: params.args.get_bsatn().clone(),
                }),
            )
        });

        let mut instance_env = InstanceEnv::new(self.module.replica_context.clone(), self.module.scheduler.clone());
        instance_env.start_reducer(params.timestamp);
        let mut tx_slot = instance_env.tx.clone();

        let mut isolate = v8::Isolate::new(
            v8::CreateParams::default()
                .external_references(extern_refs().into())
                // have to reallocate :(
                .snapshot_blob(self.module.snapshot.to_vec().into()),
        );
        let reducer_function_idx = self.module.reducers[params.reducer_id.idx()];
        let start = std::time::Instant::now();
        let (tx, result) = tx_slot.set(tx, || {
            let mut scope = &mut v8::HandleScope::new(&mut isolate);
            let mut context = v8::Context::from_snapshot(scope, 0, v8::ContextOptions::default()).unwrap();
            let mut scope = &mut v8::ContextScope::new(scope, context);
            let func = scope
                .get_context_data_from_snapshot_once::<v8::Function>(reducer_function_idx)
                .unwrap();
            let args = module_def
                .typespace()
                .with_type(&reducer_def.params)
                .with_value(&params.args.tuple);
            catch_exception(scope, |scope| {
                let args = args
                    .elements()
                    .map(|x| serialize_to_js(scope, &x))
                    .collect::<Result<Vec<_>, _>>()?;
                let recv = v8::undefined(scope).into();
                func.call(scope, recv, &args).err()?;
                Ok(())
            })
        });
        let execution_duration = start.elapsed();
        let outcome = match result {
            Ok(()) => super::ReducerOutcome::Committed,
            Err(e) => super::ReducerOutcome::Failed(anyhow::Error::from(e).to_string()),
        };
        ReducerCallResult {
            outcome,
            energy_used: EnergyQuanta::ZERO,
            execution_duration,
        }
    }
}

fn _request_interrupt<F>(handle: &v8::IsolateHandle, f: F) -> bool
where
    F: FnOnce(&mut v8::Isolate),
{
    unsafe extern "C" fn cb<F>(isolate: &mut v8::Isolate, data: *mut std::ffi::c_void)
    where
        F: FnOnce(&mut v8::Isolate),
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
