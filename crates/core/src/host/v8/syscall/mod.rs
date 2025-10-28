use spacetimedb_lib::RawModuleDef;
use v8::{callback_scope, Context, FixedArray, Local, Module, PinScope};

use crate::host::v8::de::scratch_buf;
use crate::host::v8::error::ExcResult;
use crate::host::v8::error::Throwable;
use crate::host::v8::error::TypeError;
use crate::host::wasm_common::module_host_actor::{ReducerOp, ReducerResult};

mod hooks;
mod v1;

pub(super) use self::hooks::{get_hook, HookFunction, ModuleHook};

/// The return type of a module -> host syscall.
pub(super) type FnRet<'scope> = ExcResult<Local<'scope, v8::Value>>;

/// The version of the ABI that is exposed to V8.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AbiVersion {
    V1,
}

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
