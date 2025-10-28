use std::cell::OnceCell;
use std::rc::Rc;

use enum_map::EnumMap;
use spacetimedb_lib::RawModuleDef;
use v8::{callback_scope, Context, FixedArray, Function, Local, Module, Object, PinScope};

use crate::host::v8::de::property;
use crate::host::v8::de::scratch_buf;
use crate::host::v8::error::ExcResult;
use crate::host::v8::error::Throwable;
use crate::host::v8::error::TypeError;
use crate::host::v8::from_value::cast;
use crate::host::v8::string::StringConst;
use crate::host::wasm_common::module_host_actor::{ReducerOp, ReducerResult};

mod v1;

/// The return type of a module -> host syscall.
pub(super) type FnRet<'scope> = ExcResult<Local<'scope, v8::Value>>;

/// A dependency resolver for the user's module
/// that will resolve `spacetimedb_sys` to a module that exposes the ABI.
pub(super) fn resolve_sys_module<'s>(
    context: Local<'s, Context>,
    spec: Local<'s, v8::String>,
    _attrs: Local<'s, FixedArray>,
    _referrer: Local<'s, Module>,
) -> Option<Local<'s, Module>> {
    callback_scope!(unsafe scope, context);
    resolve_sys_module_inner(scope, spec).ok()
}

fn resolve_sys_module_inner<'s>(
    scope: &mut PinScope<'s, '_>,
    spec: Local<'s, v8::String>,
) -> ExcResult<Local<'s, Module>> {
    let scratch = &mut scratch_buf::<32>();
    let spec = spec.to_rust_cow_lossy(scope, scratch);

    let generic_error = || TypeError(format!("Could not find module {spec:?}"));

    let (module, ver) = spec
        .strip_prefix("spacetime:")
        .and_then(|spec| spec.split_once('@'))
        .ok_or_else(|| generic_error().throw(scope))?;

    let (maj, min) = ver
        .split_once('.')
        .and_then(|(maj, min)| Option::zip(maj.parse::<u32>().ok(), min.parse::<u32>().ok()))
        .ok_or_else(|| TypeError(format!("Invalid version in module spec {spec:?}")).throw(scope))?;

    match module {
        "sys" => match (maj, min) {
            (1, 0) => Ok(v1::sys_v1_0(scope)),
            _ => Err(TypeError(format!(
                "Could not import {spec:?}, likely because this module was built for a newer version of SpacetimeDB.\n\
                It requires sys module v{maj}.{min}, but that version is not supported by the database."
            ))
            .throw(scope)),
        },
        _ => Err(generic_error().throw(scope)),
    }
}

pub(super) fn call_call_reducer(
    scope: &mut PinScope<'_, '_>,
    fun: HookFunction<'_>,
    op: ReducerOp<'_>,
) -> ExcResult<ReducerResult> {
    let HookFunction(ver, fun) = fun;
    match ver {
        AbiVersion::V1 => v1::call_call_reducer(scope, fun, op),
    }
}

/// Calls the registered `__describe_module__` function hook.
pub(super) fn call_describe_module<'scope>(
    scope: &mut PinScope<'scope, '_>,
    fun: HookFunction<'_>,
) -> ExcResult<RawModuleDef> {
    let HookFunction(ver, fun) = fun;
    match ver {
        AbiVersion::V1 => v1::call_describe_module(scope, fun),
    }
}

fn get_hook_function<'s>(
    scope: &mut PinScope<'s, '_>,
    hooks_obj: Local<'_, Object>,
    name: &'static StringConst,
) -> ExcResult<Local<'s, Function>> {
    let key = name.string(scope);
    let object = property(scope, hooks_obj, key)?;
    cast!(scope, object, Function, "module function hook `{}`", name.as_str()).map_err(|e| e.throw(scope))
}

fn set_hook_slots(
    scope: &mut PinScope<'_, '_>,
    abi: AbiVersion,
    hooks: &[(ModuleHook, Local<'_, Function>)],
) -> ExcResult<()> {
    // Make sure to call `set_slot` first, as it creates the annex
    // and `set_embedder_data` is currently buggy.
    let ctx = scope.get_current_context();
    let hooks_info = HooksInfo::get_or_create(&ctx);
    for &(hook, func) in hooks {
        hooks_info
            .register(hook, abi)
            .map_err(|_| TypeError("cannot call register_hooks multiple times").throw(scope))?;
        ctx.set_embedder_data(hook.to_slot_index(), func.into());
    }
    Ok(())
}

#[derive(enum_map::Enum, Copy, Clone)]
pub(super) enum ModuleHook {
    DescribeModule,
    CallReducer,
}

impl ModuleHook {
    /// Get the `v8::Context::{get,set}_embedder_data` slot that holds this hook.
    fn to_slot_index(self) -> i32 {
        match self {
            ModuleHook::DescribeModule => 20,
            ModuleHook::CallReducer => 21,
        }
    }
}

/// The version of the ABI that is exposed to V8.
#[derive(Copy, Clone, PartialEq, Eq)]
enum AbiVersion {
    V1,
}

#[derive(Default)]
struct HooksInfo {
    abi: OnceCell<AbiVersion>,
    registered: EnumMap<ModuleHook, OnceCell<()>>,
}

impl HooksInfo {
    fn get_or_create(ctx: &Context) -> Rc<Self> {
        ctx.get_slot().unwrap_or_else(|| {
            let this = Rc::<Self>::default();
            ctx.set_slot(this.clone());
            this
        })
    }

    fn register(&self, hook: ModuleHook, abi: AbiVersion) -> Result<(), ()> {
        if *self.abi.get_or_init(|| abi) != abi {
            return Err(());
        }
        self.registered[hook].set(())
    }

    fn get(&self, hook: ModuleHook) -> Option<AbiVersion> {
        self.registered[hook].get().and(self.abi.get().copied())
    }
}

#[derive(Copy, Clone)]
pub(super) struct HookFunction<'s>(AbiVersion, Local<'s, Function>);

/// Returns the hook function previously registered in [`register_hooks`].
pub(super) fn get_hook<'scope>(scope: &mut PinScope<'scope, '_>, hook: ModuleHook) -> Option<HookFunction<'scope>> {
    let ctx = scope.get_current_context();
    let hooks = ctx.get_slot::<HooksInfo>()?;

    let abi_version = hooks.get(hook)?;

    let hooks = ctx
        .get_embedder_data(scope, hook.to_slot_index())
        .expect("if `AbiVersion` is set the hook must be set");
    Some(HookFunction(abi_version, hooks.cast()))
}
