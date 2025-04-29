use std::sync::{Arc, LazyLock};

use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::Program;
use crate::energy::EnergyMonitor;
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;

use super::instance_env::InstanceEnv;
use super::module_host::{CallReducerParams, Module, ModuleInfo, ModuleInstance};
use super::{ReducerCallResult, Scheduler, UpdateDatabaseResult};

mod util;

use indexmap::IndexMap;
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v9::{sats_name_to_scoped_name, RawModuleDefV9, RawReducerDefV9, RawTypeDefV9};
use spacetimedb_lib::sats;
use util::{
    ascii_str, iter_array, module, scratch_buf, strings, throw, ErrorOrException, ExcResult, ExceptionOptionExt,
    ExceptionThrown, ObjectExt, ThrowExceptionResultExt, TypeError,
};

#[allow(unused)]
struct V8InstanceEnv {
    instance_env: InstanceEnv,
}

#[allow(unused)]
pub struct JsModule {
    replica_context: Arc<ReplicaContext>,
    scheduler: Scheduler,
    info: Arc<ModuleInfo>,
    energy_monitor: Arc<dyn EnergyMonitor>,
    snapshot: Arc<[u8]>,
    module_builder: ModuleBuilder,
}

#[allow(unused)]
pub fn compile_real(mcc: ModuleCreationContext) -> anyhow::Result<JsModule> {
    let program = std::str::from_utf8(&mcc.program.bytes)?;
    let (snapshot, module_builder) = compile(program, Arc::new(Logger))?;
    Ok(JsModule {
        replica_context: mcc.replica_ctx,
        scheduler: mcc.scheduler,
        info: todo!(),
        energy_monitor: mcc.energy_monitor,
        snapshot,
        module_builder,
    })
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
    let isolate = v8::Isolate::snapshot_creator(Some(&EXTERN_REFS), None);
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

static EXTERN_REFS: LazyLock<v8::ExternalReferences> =
    LazyLock::new(|| v8::ExternalReferences::new(&spacetime_sys_10_0::external_refs().collect_vec()));

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

fn js_to_type(scope: &mut v8::HandleScope<'_>, val: v8::Local<'_, v8::Value>) -> ExcResult<sats::AlgebraicType> {
    strings!(
        REF = "ref",
        TYPE = "type",
        SUM = "sum",
        PRODUCT = "product",
        ARRAY = "array",
        STRING = "string",
        BOOL = "bool",
        I8 = "i8",
        U8 = "u8",
        I16 = "i16",
        U16 = "u16",
        I32 = "i32",
        U32 = "u32",
        I64 = "i64",
        U64 = "u64",
        I128 = "i128",
        U128 = "u128",
        I256 = "i256",
        U256 = "u256",
        F32 = "f32",
        F64 = "f64",
    );

    let val = val.cast::<v8::Object>();
    let ty = val.get_str(scope, &TYPE).err()?;

    if ty == REF.string(scope) {
        let r = val.get_str(scope, &REF).err()?.cast::<v8::Number>().value() as u32;
        Ok(sats::AlgebraicType::Ref(sats::AlgebraicTypeRef(r)))
    } else if ty == PRODUCT.string(scope) {
        let elements = val.get_str(scope, ascii_str!("elements")).err()?.cast();
        let elements = iter_array(scope, elements, |scope, elem| {
            let elem = elem.cast::<v8::Object>();

            let name = elem.get_str(scope, ascii_str!("name")).err()?;
            let name = if name.is_null_or_undefined() {
                None
            } else {
                Some(name.cast::<v8::String>().to_rust_string_lossy(scope).into())
            };

            let ty = elem.get_str(scope, ascii_str!("algebraic_type")).err()?;
            let ty = js_to_type(scope, ty)?;

            Ok(sats::ProductTypeElement::new(ty, name))
        })
        .collect::<Result<Box<[_]>, _>>()?;

        Ok(sats::AlgebraicType::product(elements))
    } else if ty == ARRAY.string(scope) {
        let elem_ty = val.get_str(scope, ascii_str!("elem_ty")).err()?;
        let elem_ty = js_to_type(scope, elem_ty)?;
        Ok(sats::AlgebraicType::array(elem_ty))
    } else if ty == STRING.string(scope) {
        Ok(sats::AlgebraicType::String)
    } else if ty == BOOL.string(scope) {
        Ok(sats::AlgebraicType::Bool)
    } else if ty == I8.string(scope) {
        Ok(sats::AlgebraicType::I8)
    } else if ty == U8.string(scope) {
        Ok(sats::AlgebraicType::U8)
    } else if ty == I16.string(scope) {
        Ok(sats::AlgebraicType::I16)
    } else if ty == U16.string(scope) {
        Ok(sats::AlgebraicType::U16)
    } else if ty == I32.string(scope) {
        Ok(sats::AlgebraicType::I32)
    } else if ty == U32.string(scope) {
        Ok(sats::AlgebraicType::U32)
    } else if ty == I64.string(scope) {
        Ok(sats::AlgebraicType::I64)
    } else if ty == U64.string(scope) {
        Ok(sats::AlgebraicType::U64)
    } else if ty == I128.string(scope) {
        Ok(sats::AlgebraicType::I128)
    } else if ty == U128.string(scope) {
        Ok(sats::AlgebraicType::U128)
    } else if ty == I256.string(scope) {
        Ok(sats::AlgebraicType::I256)
    } else if ty == U256.string(scope) {
        Ok(sats::AlgebraicType::U256)
    } else if ty == F32.string(scope) {
        Ok(sats::AlgebraicType::F32)
    } else if ty == F64.string(scope) {
        Ok(sats::AlgebraicType::F64)
    } else {
        throw(scope, TypeError(ascii_str!("Unknown type")))
    }
}

fn register_reducer(scope: &mut v8::HandleScope<'_>, args: v8::FunctionCallbackArguments<'_>) -> ExcResult<()> {
    if scope.get_slot::<ModuleBuilder>().is_none() {
        throw(scope, TypeError(ascii_str!("You cannot dynamically register reducers")))?;
    }

    let name = args.get(0).cast::<v8::String>();
    let params = args.get(1);

    let params = js_to_type(scope, params)?.into_product().unwrap();

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

    let ty = js_to_type(scope, ty)?;

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
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    let program = include_str!("./test_code.js");
    let (_snapshot, module) = compile(program, Arc::new(Logger)).unwrap();
    // dbg!(module);
    // dbg!(module_idx, bytes::Bytes::copy_from_slice(&snapshot));
    // panic!();
}

#[allow(unused)]
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

pub struct JsInstance {}

#[allow(unused)]
impl ModuleInstance for JsInstance {
    fn trapped(&self) -> bool {
        false
    }

    fn init_database(&mut self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        todo!()
    }

    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        todo!()
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        todo!()
    }
}
