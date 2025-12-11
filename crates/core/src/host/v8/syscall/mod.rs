use bytes::Bytes;
use spacetimedb_lib::{RawModuleDef, VersionTuple};
use v8::{callback_scope, Context, FixedArray, Local, Module, PinScope};

use crate::host::v8::de::scratch_buf;
use crate::host::v8::error::{ErrorOrException, ExcResult, ExceptionThrown, Throwable, TypeError};
use crate::host::wasm_common::abi::parse_abi_version;
use crate::host::wasm_common::module_host_actor::{
    AnonymousViewOp, ProcedureOp, ReducerOp, ReducerResult, ViewOp, ViewReturnData,
};

mod hooks;
mod v1;
mod v2;

pub(super) use self::hooks::{get_hooks, HookFunctions, ModuleHookKey};

/// The return type of a module -> host syscall.
pub(super) type FnRet<'scope> = ExcResult<Local<'scope, v8::Value>>;

/// The version of the ABI that is exposed to V8.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AbiVersion {
    V1,
    V2,
}

/// A dependency resolver for the user's module
/// that will resolve `spacetimedb_sys` to a module that exposes the ABI.
pub(super) fn resolve_sys_module<'scope>(
    context: Local<'scope, Context>,
    spec: Local<'scope, v8::String>,
    _attrs: Local<'scope, FixedArray>,
    _referrer: Local<'scope, Module>,
) -> Option<Local<'scope, Module>> {
    callback_scope!(unsafe scope, context);
    resolve_sys_module_inner(scope, spec).ok()
}

fn resolve_sys_module_inner<'scope>(
    scope: &mut PinScope<'scope, '_>,
    spec: Local<'scope, v8::String>,
) -> ExcResult<Local<'scope, Module>> {
    let scratch = &mut scratch_buf::<32>();
    let spec = spec.to_rust_cow_lossy(scope, scratch);

    let generic_error = || TypeError(format!("Could not find module {spec:?}"));

    let (module, ver) = spec
        .strip_prefix("spacetime:")
        .and_then(|spec| spec.split_once('@'))
        .ok_or_else(|| generic_error().throw(scope))?;

    let VersionTuple { major, minor } = parse_abi_version(ver)
        .ok_or_else(|| TypeError(format!("Invalid version in module spec {spec:?}")).throw(scope))?;

    match module {
        "sys" => match (major, minor) {
            (1, 0) => Ok(v1::sys_v1_0(scope)),
            (1, 1) => Ok(v1::sys_v1_1(scope)),
            (1, 2) => Ok(v1::sys_v1_2(scope)),
            (1, 3) => Ok(v1::sys_v1_3(scope)),
            (2, 0) => Ok(v2::sys_v2_0(scope)),
            _ => Err(TypeError(format!(
                "Could not import {spec:?}, likely because this module was built for a newer version of SpacetimeDB.\n\
            It requires sys module v{major}.{minor}, but that version is not supported by the database."
            ))
            .throw(scope)),
        },
        _ => Err(generic_error().throw(scope)),
    }
}

/// Calls the registered `__call_reducer__` function hook.
///
/// This handles any (future) ABI version differences.
pub(super) fn call_call_reducer(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ReducerOp<'_>,
) -> ExcResult<ReducerResult> {
    match hooks.abi {
        AbiVersion::V1 => v1::call_call_reducer(scope, hooks, op),
        AbiVersion::V2 => v2::call_call_reducer(scope, hooks, op),
    }
}

/// Calls the registered `__call_view__` function hook.
///
/// This handles any (future) ABI version differences.
pub(super) fn call_call_view(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ViewOp<'_>,
) -> Result<ViewReturnData, ErrorOrException<ExceptionThrown>> {
    match hooks.abi {
        AbiVersion::V1 => v1::call_call_view(scope, hooks, op),
        AbiVersion::V2 => v2::call_call_view(scope, hooks, op),
    }
}

/// Calls the registered `__call_view_anon__` function hook.
///
/// This handles any (future) ABI version differences.
pub(super) fn call_call_view_anon(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: AnonymousViewOp<'_>,
) -> Result<ViewReturnData, ErrorOrException<ExceptionThrown>> {
    match hooks.abi {
        AbiVersion::V1 => v1::call_call_view_anon(scope, hooks, op),
        AbiVersion::V2 => v2::call_call_view_anon(scope, hooks, op),
    }
}

/// Calls the registered `__call_procedure__` function hook.
///
/// This handles any (future) ABI version differences.
pub(super) fn call_call_procedure(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ProcedureOp,
) -> Result<Bytes, ErrorOrException<ExceptionThrown>> {
    match hooks.abi {
        AbiVersion::V1 => v1::call_call_procedure(scope, hooks, op),
        AbiVersion::V2 => v2::call_call_procedure(scope, hooks, op),
    }
}

/// Calls the registered `__describe_module__` function hook.
///
/// This handles any (future) ABI version differences.
pub(super) fn call_describe_module<'scope>(
    scope: &mut PinScope<'scope, '_>,
    hooks: &HookFunctions<'_>,
) -> Result<RawModuleDef, ErrorOrException<ExceptionThrown>> {
    match hooks.abi {
        AbiVersion::V1 => v1::call_describe_module(scope, hooks),
        AbiVersion::V2 => v2::call_describe_module(scope, hooks),
    }
}
