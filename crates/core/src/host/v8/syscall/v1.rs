use super::super::de::deserialize_js;
use super::super::error::{
    handle_sys_call_error, no_such_iter, throw_if_terminated, BufferTooSmall, ErrorOrException, ExcResult,
    ExceptionThrown, SysCallResult,
};
use super::super::from_value::cast;
use super::super::ser::serialize_to_js;
use super::super::string::{str_from_ident, StringConst};
use super::super::{call_free_fun, env_on_isolate, Throwable};
use super::common::{
    console_log, console_timer_end, console_timer_start, datastore_index_scan_range_bsatn_inner,
    datastore_table_row_count, datastore_table_scan_bsatn, deserialize_row_iter_idx, get_env, get_jwt_payload,
    identity, index_id_from_name, procedure_abort_mut_tx, procedure_commit_mut_tx, procedure_http_request,
    procedure_start_mut_tx, row_iter_bsatn_close, table_id_from_name, volatile_nonatomic_schedule_immediate,
};
use super::hooks::HookFunctions;
use super::hooks::{get_hook_function, set_hook_slots};
use super::{AbiVersion, FnRet, ModuleHookKey};
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::instrumentation::span;
use crate::host::wasm_common::module_host_actor::{AnonymousViewOp, ReducerOp, ReducerResult, ViewOp, ViewReturnData};
use crate::host::AbiCall;
use anyhow::Context;
use bytes::Bytes;
use spacetimedb_primitives::{ColId, IndexId, ReducerId, TableId, ViewFnPtr};
use spacetimedb_sats::Serialize;
use v8::{
    callback_scope, ConstructorBehavior, Function, FunctionCallbackArguments, Local, Module, Object, PinCallbackScope,
    PinScope,
};

macro_rules! create_synthetic_module {
    ($scope:expr, $module_name:expr $(, ($wrapper:ident, $abi_call:expr, $fun:ident))* $(,)?) => {{
        let export_names = &[$(str_from_ident!($fun).string($scope)),*];
        let eval_steps = |context, module| {
            callback_scope!(unsafe scope, context);
            $(
                register_module_fun(scope, &module, str_from_ident!($fun), |s, a| {
                    $wrapper($abi_call, s, a, $fun)
                })?;
            )*

            Some(v8::undefined(scope).into())
        };

        Module::create_synthetic_module(
            $scope,
            const { StringConst::new($module_name) }.string($scope),
            export_names,
            eval_steps,
        )
    }}
}

/// Registers all module -> host syscalls in the JS module `spacetimedb_sys`.
pub(super) fn sys_v1_0<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, Module> {
    use register_hooks_v1_0 as register_hooks;
    create_synthetic_module!(
        scope,
        "spacetime:sys@1.0",
        (with_nothing, (), register_hooks),
        (with_sys_result_ret, AbiCall::TableIdFromName, table_id_from_name),
        (with_sys_result_ret, AbiCall::IndexIdFromName, index_id_from_name),
        (
            with_sys_result_ret,
            AbiCall::DatastoreTableRowCount,
            datastore_table_row_count
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreTableScanBsatn,
            datastore_table_scan_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreIndexScanRangeBsatn,
            datastore_index_scan_range_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::RowIterBsatnAdvance,
            row_iter_bsatn_advance
        ),
        (with_sys_result_noret, AbiCall::RowIterBsatnClose, row_iter_bsatn_close),
        (
            with_sys_result_ret,
            AbiCall::DatastoreInsertBsatn,
            datastore_insert_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreUpdateBsatn,
            datastore_update_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreDeleteByIndexScanRangeBsatn,
            datastore_delete_by_index_scan_range_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreDeleteAllByEqBsatn,
            datastore_delete_all_by_eq_bsatn
        ),
        (
            with_sys_result_noret,
            AbiCall::VolatileNonatomicScheduleImmediate,
            volatile_nonatomic_schedule_immediate
        ),
        (with_sys_result_noret, AbiCall::ConsoleLog, console_log),
        (with_sys_result_ret, AbiCall::ConsoleTimerStart, console_timer_start),
        (with_sys_result_noret, AbiCall::ConsoleTimerEnd, console_timer_end),
        (with_sys_result_ret, AbiCall::Identity, identity),
        (with_sys_result_ret, AbiCall::GetJwt, get_jwt_payload),
    )
}

pub(super) fn sys_v1_1<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, Module> {
    use register_hooks_v1_1 as register_hooks;
    create_synthetic_module!(scope, "spacetime:sys@1.1", (with_nothing, (), register_hooks))
}

pub(super) fn sys_v1_2<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, Module> {
    use register_hooks_v1_2 as register_hooks;
    create_synthetic_module!(
        scope,
        "spacetime:sys@1.2",
        (with_nothing, (), register_hooks),
        (
            with_sys_result_value,
            AbiCall::ProcedureHttpRequest,
            procedure_http_request
        ),
        (
            with_sys_result_ret,
            AbiCall::ProcedureStartMutTransaction,
            procedure_start_mut_tx
        ),
        (
            with_sys_result_noret,
            AbiCall::ProcedureAbortMutTransaction,
            procedure_abort_mut_tx
        ),
        (
            with_sys_result_noret,
            AbiCall::ProcedureCommitMutTransaction,
            procedure_commit_mut_tx
        ),
    )
}

pub(super) fn sys_v1_3<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, Module> {
    create_synthetic_module!(
        scope,
        "spacetime:sys@1.2",
        (
            with_sys_result_ret,
            AbiCall::DatastoreIndexScanPointBsatn,
            datastore_index_scan_point_bsatn
        ),
        (
            with_sys_result_ret,
            AbiCall::DatastoreDeleteByIndexScanPointBsatn,
            datastore_delete_by_index_scan_point_bsatn
        ),
    )
}

/// Registers a function in `module`
/// where the function has `name` and does `body`.
fn register_module_fun(
    scope: &mut PinCallbackScope<'_, '_>,
    module: &Local<'_, Module>,
    name: &'static StringConst,
    body: impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> Option<bool> {
    // Convert the name.
    let name = name.string(scope);

    // Convert the function.
    let fun = Function::builder(adapt_fun(body)).constructor_behavior(ConstructorBehavior::Throw);
    let fun = fun.build(scope)?.into();

    // Set the export on the module.
    module.set_synthetic_module_export(scope, name, fun)
}

/// Adapts `fun`, which returns a [`Value`] to one that works on [`v8::ReturnValue`].
fn adapt_fun(
    fun: impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>, v8::ReturnValue) {
    move |scope, args, mut rv| {
        if throw_if_terminated(scope) {
            return;
        }

        // Set the result `value` on success.
        if let Ok(value) = fun(scope, args) {
            rv.set(value);
        }
    }
}

/// Wraps `run` in [`with_span`] and returns the return value of `run` to JS.
/// Handles [`SysCallError`] if it occurs by throwing exceptions into JS.
fn with_sys_result_ret<'scope, O: Serialize>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    run: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> SysCallResult<O>,
) -> FnRet<'scope> {
    match with_span(abi_call, scope, args, run) {
        Ok(ret) => serialize_to_js(scope, &ret),
        Err(err) => Err(handle_sys_call_error(abi_call, scope, err)),
    }
}

/// Wraps `run` in [`with_span`] and returns undefined to JS.
/// Handles [`SysCallError`] if it occurs by throwing exceptions into JS.
fn with_sys_result_noret<'scope>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    run: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> SysCallResult<()>,
) -> FnRet<'scope> {
    match with_span(abi_call, scope, args, run) {
        Ok(()) => Ok(v8::undefined(scope).into()),
        Err(err) => Err(handle_sys_call_error(abi_call, scope, err)),
    }
}

/// Wraps `run` in [`with_span`] and returns undefined to JS.
/// Handles [`SysCallError`] if it occurs by throwing exceptions into JS.
fn with_sys_result_value<'scope, O>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    run: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> SysCallResult<Local<'scope, O>>,
) -> FnRet<'scope>
where
    Local<'scope, O>: Into<Local<'scope, v8::Value>>,
{
    match with_span(abi_call, scope, args, run) {
        Ok(v) => Ok(v.into()),
        Err(err) => Err(handle_sys_call_error(abi_call, scope, err)),
    }
}

/// A higher order function conforming to the interface of [`with_sys_result`] and [`with_span`].
fn with_nothing<'scope>(
    (): (),
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    run: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> FnRet<'scope> {
    run(scope, args)
}

/// Tracks the span of `body` under the label `abi_call`.
fn with_span<'scope, T, E: From<ExceptionThrown>>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    body: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> Result<T, E>,
) -> Result<T, E> {
    // Start the span.
    let span_start = span::CallSpanStart::new(abi_call);

    // Call `fun` with `args` in `scope`.
    let result = body(scope, args);

    // Track the span of this call.
    let span = span_start.end();
    if let Some(env) = env_on_isolate(scope) {
        span::record_span(&mut env.call_times, span);
    }

    result
}

/// Module ABI that registers the functions called by the host.
///
/// # Signature
///
/// ```ignore
/// register_hooks(hooks: {
///     __describe_module__: () => u8[];
///     __call_reducer__: (
///         reducer_id: u32,
///         sender: u256,
///         conn_id: u128,
///         timestamp: i64,
///         args_buf: u8[]
///     ) => { tag: 'ok' } | { tag: 'err'; value: string };
/// }): void
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `i64` is `bigint` in JS restricted to signed 64-bit integers.
/// - `u128` is `bigint` in JS restricted to unsigned 128-bit integers.
/// - `u256` is `bigint` in JS restricted to unsigned 256-bit integers.
///
/// # Returns
///
/// Returns nothing.
///
/// # Throws
///
/// Throws a `TypeError` if:
/// - `hooks` is not an object that has functions `__describe_module__` and `__call_reducer__`.
fn register_hooks_v1_0<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'_>) -> FnRet<'scope> {
    // Convert `hooks` to an object.
    let hooks = cast!(scope, args.get(0), Object, "hooks object").map_err(|e| e.throw(scope))?;

    let describe_module = get_hook_function(scope, hooks, str_from_ident!(__describe_module__))?;
    let call_reducer = get_hook_function(scope, hooks, str_from_ident!(__call_reducer__))?;

    // Set the hooks.
    set_hook_slots(
        scope,
        AbiVersion::V1,
        &[
            (ModuleHookKey::DescribeModule, describe_module),
            (ModuleHookKey::CallReducer, call_reducer),
        ],
    )?;

    Ok(v8::undefined(scope).into())
}

/// Module ABI that registers the functions called by the host.
///
/// # Signature
///
/// ```ignore
/// register_hooks(hooks: {
///     __call_view__(view_id: u32, sender: u256, args: u8[]): u8[];
///     __call_view_anon__(view_id: u32, args: u8[]): u8[];
/// }): void
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u32` is `bigint` in JS restricted to unsigned 32-bit integers.
/// - `u256` is `bigint` in JS restricted to unsigned 256-bit integers.
///
/// # Returns
///
/// Returns nothing.
///
/// # Throws
///
/// Throws a `TypeError` if:
/// - `hooks` is not an object that has the correct functions.
fn register_hooks_v1_1<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'_>) -> FnRet<'scope> {
    // Convert `hooks` to an object.
    let hooks = cast!(scope, args.get(0), Object, "hooks object").map_err(|e| e.throw(scope))?;

    let call_view = get_hook_function(scope, hooks, str_from_ident!(__call_view__))?;
    let call_view_anon = get_hook_function(scope, hooks, str_from_ident!(__call_view_anon__))?;

    // Set the hooks.
    set_hook_slots(
        scope,
        AbiVersion::V1,
        &[
            (ModuleHookKey::CallView, call_view),
            (ModuleHookKey::CallAnonymousView, call_view_anon),
        ],
    )?;

    Ok(v8::undefined(scope).into())
}

/// Module ABI that registers the functions called by the host.
///
/// # Signature
///
/// ```ignore
/// export function register_hooks(hooks: {
///     __call_procedure__(
///         id: u32,
///         sender: u256,
///         connection_id: u128,
///         timestamp: u64,
///         args: Uint8Array
///     ): Uint8Array;
/// }): void;
/// ```
///
/// # Returns
///
/// Returns nothing.
///
/// # Throws
///
/// Throws a `TypeError` if:
/// - `hooks` is not an object that has the correct functions.
fn register_hooks_v1_2<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'_>) -> FnRet<'scope> {
    // Convert `hooks` to an object.
    let hooks = cast!(scope, args.get(0), Object, "hooks object").map_err(|e| e.throw(scope))?;

    let call_procedure = get_hook_function(scope, hooks, str_from_ident!(__call_procedure__))?;

    // Set the hooks.
    set_hook_slots(scope, AbiVersion::V1, &[(ModuleHookKey::CallProcedure, call_procedure)])?;

    Ok(v8::undefined(scope).into())
}

/// Calls the `__call_reducer__` function `fun`.
pub(super) fn call_call_reducer(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ReducerOp<'_>,
) -> ExcResult<ReducerResult> {
    let ReducerOp {
        id: ReducerId(reducer_id),
        name: _,
        caller_identity: sender,
        caller_connection_id: conn_id,
        timestamp,
        args: reducer_args,
    } = op;
    // Serialize the arguments.
    let reducer_id = serialize_to_js(scope, &reducer_id)?;
    let sender = serialize_to_js(scope, &sender.to_u256())?;
    let conn_id: v8::Local<'_, v8::Value> = serialize_to_js(scope, &conn_id.to_u128())?;
    let timestamp = serialize_to_js(scope, &timestamp.to_micros_since_unix_epoch())?;
    let reducer_args = serialize_to_js(scope, reducer_args.get_bsatn())?;
    let args = &[reducer_id, sender, conn_id, timestamp, reducer_args];

    // Call the function.
    let ret = call_free_fun(scope, hooks.call_reducer, args)?;

    // Deserialize the user result.
    let user_res = deserialize_js(scope, ret)?;

    Ok(user_res)
}

/// Calls the `__call_view__` function `fun`.
pub(super) fn call_call_view(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ViewOp<'_>,
) -> Result<ViewReturnData, ErrorOrException<ExceptionThrown>> {
    let fun = hooks.call_view.context("`__call_view__` was never defined")?;

    let ViewOp {
        fn_ptr: ViewFnPtr(view_id),
        view_id: _,
        table_id: _,
        name: _,
        sender,
        timestamp: _,
        args: view_args,
    } = op;
    // Serialize the arguments.
    let view_id = serialize_to_js(scope, &view_id)?;
    let sender = serialize_to_js(scope, &sender.to_u256())?;
    let view_args = serialize_to_js(scope, view_args.get_bsatn())?;
    let args = &[view_id, sender, view_args];

    // Call the function.
    let ret = call_free_fun(scope, fun, args)?;

    // The original version returned a byte array with the encoded rows.
    if ret.is_typed_array() && ret.is_uint8_array() {
        // This is the original format, which just returns the raw bytes.
        let ret =
            cast!(scope, ret, v8::Uint8Array, "bytes return from `__call_view_anon__`").map_err(|e| e.throw(scope))?;
        let bytes = ret.get_contents(&mut []);

        return Ok(ViewReturnData::Rows(Bytes::copy_from_slice(bytes)));
    };

    // The newer version returns an object with a `data` field containing the bytes.
    let ret = cast!(scope, ret, v8::Object, "object return from `__call_view_anon__`").map_err(|e| e.throw(scope))?;

    let Some(data_key) = v8::String::new(scope, "data") else {
        return Err(ErrorOrException::Err(anyhow::anyhow!("error creating a v8 string")));
    };
    let Some(data_val) = ret.get(scope, data_key.into()) else {
        return Err(ErrorOrException::Err(anyhow::anyhow!(
            "data key not found in return object"
        )));
    };

    let ret = cast!(
        scope,
        data_val,
        v8::Uint8Array,
        "bytes in the `data` field returned from `__call_view_anon__`"
    )
    .map_err(|e| e.throw(scope))?;
    let bytes = ret.get_contents(&mut []);

    Ok(ViewReturnData::HeaderFirst(Bytes::copy_from_slice(bytes)))
}

/// Calls the `__call_view_anon__` function `fun`.
pub(super) fn call_call_view_anon(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: AnonymousViewOp<'_>,
) -> Result<ViewReturnData, ErrorOrException<ExceptionThrown>> {
    let fun = hooks.call_view_anon.context("`__call_view_anon__` was never defined")?;

    let AnonymousViewOp {
        fn_ptr: ViewFnPtr(view_id),
        view_id: _,
        table_id: _,
        name: _,
        timestamp: _,
        args: view_args,
    } = op;
    // Serialize the arguments.
    let view_id = serialize_to_js(scope, &view_id)?;
    let view_args = serialize_to_js(scope, view_args.get_bsatn())?;
    let args = &[view_id, view_args];

    // Call the function.
    let ret = call_free_fun(scope, fun, args)?;

    if ret.is_typed_array() && ret.is_uint8_array() {
        // This is the original format, which just returns the raw bytes.
        let ret =
            cast!(scope, ret, v8::Uint8Array, "bytes return from `__call_view_anon__`").map_err(|e| e.throw(scope))?;
        let bytes = ret.get_contents(&mut []);

        // We are pretending this was sent with the new format.
        return Ok(ViewReturnData::Rows(Bytes::copy_from_slice(bytes)));
    };

    let ret = cast!(
        scope,
        ret,
        v8::Object,
        "bytes or object return from `__call_view_anon__`"
    )
    .map_err(|e| e.throw(scope))?;

    let Some(data_key) = v8::String::new(scope, "data") else {
        return Err(ErrorOrException::Err(anyhow::anyhow!("error creating a v8 string")));
    };
    let Some(data_val) = ret.get(scope, data_key.into()) else {
        return Err(ErrorOrException::Err(anyhow::anyhow!(
            "data key not found in return object"
        )));
    };

    let ret = cast!(
        scope,
        data_val,
        v8::Uint8Array,
        "bytes in the `data` field returned from `__call_view_anon__`"
    )
    .map_err(|e| e.throw(scope))?;
    let bytes = ret.get_contents(&mut []);

    Ok(ViewReturnData::HeaderFirst(Bytes::copy_from_slice(bytes)))
}

/// Module ABI that finds all rows in the index identified by `index_id`,
/// according to `prefix`, `rstart`, and `rend`.
///
/// The index itself has a schema/type.
/// The `prefix` is decoded to the initial `prefix_elems` `AlgebraicType`s
/// whereas `rstart` and `rend` are decoded to the `prefix_elems + 1` `AlgebraicType`
/// where the `AlgebraicValue`s are wrapped in `Bound`.
/// That is, `rstart, rend` are BSATN-encoded `Bound<AlgebraicValue>`s.
///
/// Matching is then defined by equating `prefix`
/// to the initial `prefix_elems` columns of the index
/// and then imposing `rstart` as the starting bound
/// and `rend` as the ending bound on the `prefix_elems + 1` column of the index.
/// Remaining columns of the index are then unbounded.
/// Note that the `prefix` in this case can be empty (`prefix_elems = 0`),
/// in which case this becomes a ranged index scan on a single-col index
/// or even a full table scan if `rstart` and `rend` are both unbounded.
///
/// The relevant table for the index is found implicitly via the `index_id`,
/// which is unique for the module.
///
/// On success, the iterator handle is returned.
/// This handle can be advanced by [`row_iter_bsatn_advance`].
///
/// # Non-obvious queries
///
/// For an index on columns `[a, b, c]`:
///
/// - `a = x, b = y` is encoded as a prefix `[x, y]`
///   and a range `Range::Unbounded`,
///   or as a  prefix `[x]` and a range `rstart = rend = Range::Inclusive(y)`.
/// - `a = x, b = y, c = z` is encoded as a prefix `[x, y]`
///   and a  range `rstart = rend = Range::Inclusive(z)`.
/// - A sorted full scan is encoded as an empty prefix
///   and a range `Range::Unbounded`.
///
/// # Signature
///
/// ```ignore
/// datastore_index_scan_range_bsatn(
///     index_id: u32,
///     prefix: u8[],
///     prefix_elems: u16,
///     rstart: u8[],
///     rend: u8[],
/// ) -> u32 throws {
///    __code_error__: NOT_IN_TRANSACTION | NO_SUCH_INDEX | BSATN_DECODE_ERROR
/// }
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
/// - `u64` is `bigint` in JS restricted to unsigned 64-bit integers.
///
/// # Returns
///
/// Returns a `u32` that is the iterator handle.
/// This handle can be advanced by [`row_iter_bsatn_advance`].
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_INDEX`]
///   when `index_id` is not a known ID of an index.
///
/// - [`spacetimedb_primitives::errno::BSATN_DECODE_ERROR`]
///   when `prefix` cannot be decoded to
///   a `prefix_elems` number of `AlgebraicValue`
///   typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
///   Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
///   where the inner `AlgebraicValue`s are
///   typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
///
/// Throws a `TypeError` if:
/// - `index_id` is not a `u32`.
/// - `prefix`, `rstart`, and `rend` are not arrays of `u8`s.
/// - `prefix_elems` is not a `u16`.
fn datastore_index_scan_range_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let index_id: IndexId = deserialize_js(scope, args.get(0))?;
    let prefix: &[u8] = deserialize_js(scope, args.get(1))?;
    let prefix_elems: ColId = deserialize_js(scope, args.get(2))?;
    let rstart: &[u8] = deserialize_js(scope, args.get(3))?;
    let rend: &[u8] = deserialize_js(scope, args.get(4))?;

    datastore_index_scan_range_bsatn_inner(scope, index_id, prefix, prefix_elems, rstart, rend)
}

/// Module ABI that reads rows from the given iterator registered under `iter`.
///
/// Takes rows from the iterator with id `iter`
/// and returns them encoded in the BSATN format.
///
/// The rows returned take up at most `buffer_max_len` bytes.
/// A row is never broken up between calls.
///
/// Aside from the BSATN,
/// the function also returns `true` when the iterator been exhausted
/// and there are no more rows to read.
/// This leads to the iterator being immediately destroyed.
/// Conversely, `false` is returned if there are more rows to read.
/// Note that the host is free to reuse allocations in a pool,
/// destroying the handle logically does not entail that memory is necessarily reclaimed.
///
/// # Signature
///
/// ```ignore
/// row_iter_bsatn_advance(iter: u32, buffer_max_len: u32) -> (boolean, u8[]) throws
///     { __code_error__: NO_SUCH_ITER } | { __buffer_too_small__: number }
/// ```
///
/// # Types
///
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns `(exhausted: boolean, rows_bsatn: u8[])` where:
/// - `exhausted` is `true` if there are no more rows to read,
/// - `rows_bsatn` are the BSATN-encoded row bytes, concatenated.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_ITER`]
///   when `iter` is not a valid iterator.
///
/// Throws `{ __buffer_too_small__: number }`
/// when there are rows left but they cannot fit in `buffer`.
/// When this occurs, `__buffer_too_small__` contains the size of the next item in the iterator.
/// To make progress, the caller should call `row_iter_bsatn_advance`
/// with `buffer_max_len >= __buffer_too_small__` and try again.
///
/// Throws a `TypeError` if:
/// - `iter` and `buffer_max_len` are not `u32`s.
fn row_iter_bsatn_advance<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<(bool, Vec<u8>)> {
    let row_iter_idx = deserialize_row_iter_idx(scope, args.get(0))?;
    let buffer_max_len: u32 = deserialize_js(scope, args.get(1))?;

    // Retrieve the iterator by `row_iter_idx`, or error.
    let env = get_env(scope)?;
    let Some(iter) = env.iters.get_mut(row_iter_idx) else {
        return Err(no_such_iter(scope));
    };

    // Allocate a buffer with `buffer_max_len` capacity.
    let mut buffer = vec![0; buffer_max_len as usize];
    // Fill the buffer as much as possible.
    let written = InstanceEnv::fill_buffer_from_iter(iter, &mut buffer, &mut env.chunk_pool);
    buffer.truncate(written);

    let next_buf_len = iter.as_slice().first().map(|v| v.len());
    let done = match (written, next_buf_len) {
        // Nothing was written and the iterator is not exhausted.
        (0, Some(min_len)) => {
            let min_len = min_len.try_into().unwrap();
            let exc = BufferTooSmall::from_requirement(scope, min_len)?;
            return Err(exc.throw(scope).into());
        }
        // The iterator is exhausted, destroy it, and tell the caller.
        (_, None) => {
            env.iters.take(row_iter_idx);
            true
        }
        // Something was written, but the iterator is not exhausted.
        (_, Some(_)) => false,
    };
    Ok((done, buffer))
}

/// Module ABI that inserts a row into the table identified by `table_id`,
/// where the `row` is an array of bytes.
///
/// The byte array `row` must be a BSATN-encoded `ProductValue`
/// typed at the table's `ProductType` row-schema.
///
/// To handle auto-incrementing columns,
/// when the call is successful,
/// the an array of bytes is returned, containing the generated sequence values.
/// These values are written as a BSATN-encoded `pv: ProductValue`.
/// Each `v: AlgebraicValue` in `pv` is typed at the sequence's column type.
/// The `v`s in `pv` are ordered by the order of the columns, in the schema of the table.
/// When the table has no sequences,
/// this implies that the `pv`, and thus `row`, will be empty.
///
/// # Signature
///
/// ```ignore
/// datastore_insert_bsatn(table_id: u32, row: u8[]) -> u8[] throws {
///     __code_error__:
///           NOT_IN_TRANSACTION
///         | NOT_SUCH_TABLE
///         | BSATN_DECODE_ERROR
///         | UNIQUE_ALREADY_EXISTS
///         | SCHEDULE_AT_DELAY_TOO_LONG
/// }
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns the generated sequence values encoded in BSATN (see above).
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
/// - [`spacetimedb_primitives::errno::NOT_SUCH_TABLE`]
///   when `table_id` is not a known ID of a table.
/// - [`spacetimedb_primitives::errno::`BSATN_DECODE_ERROR`]
///   when `row` cannot be decoded to a `ProductValue`.
///   typed at the `ProductType` the table's schema specifies.
/// - [`spacetimedb_primitives::errno::`UNIQUE_ALREADY_EXISTS`]
///   when inserting `row` would violate a unique constraint.
/// - [`spacetimedb_primitives::errno::`SCHEDULE_AT_DELAY_TOO_LONG`]
///   when the delay specified in the row was too long.
///
/// Throws a `TypeError` if:
/// - `table_id` is not a `u32`.
/// - `row` is not an array of `u8`s.
fn datastore_insert_bsatn(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<Vec<u8>> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;
    let mut row: Vec<u8> = deserialize_js(scope, args.get(1))?;

    // Insert the row into the DB and write back the generated column values.
    let row_len = get_env(scope)?.instance_env.insert(table_id, &mut row)?;
    row.truncate(row_len);

    Ok(row)
}

/// Module ABI that updates a row into the table identified by `table_id`,
/// where the `row` is an array of bytes.
///
/// The byte array `row` must be a BSATN-encoded `ProductValue`
/// typed at the table's `ProductType` row-schema.
///
/// The row to update is found by projecting `row`
/// to the type of the *unique* index identified by `index_id`.
/// If no row is found, the error `NO_SUCH_ROW` is returned.
///
/// To handle auto-incrementing columns,
/// when the call is successful,
/// the `row` is written back to with the generated sequence values.
/// These values are written as a BSATN-encoded `pv: ProductValue`.
/// Each `v: AlgebraicValue` in `pv` is typed at the sequence's column type.
/// The `v`s in `pv` are ordered by the order of the columns, in the schema of the table.
/// When the table has no sequences,
/// this implies that the `pv`, and thus `row`, will be empty.
///
/// # Signature
///
/// ```ignore
/// datastore_update_bsatn(table_id: u32, index_id: u32, row: u8[]) -> u8[] throws {
///     __code_error__:
///         NOT_IN_TRANSACTION
///       | NOT_SUCH_TABLE
///       | NO_SUCH_INDEX
///       | INDEX_NOT_UNIQUE
///       | BSATN_DECODE_ERROR
///       | NO_SUCH_ROW
///       | UNIQUE_ALREADY_EXISTS
///       | SCHEDULE_AT_DELAY_TOO_LONG
/// }
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns the generated sequence values encoded in BSATN (see above).
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
/// - [`spacetimedb_primitives::errno::NOT_SUCH_TABLE`]
///   when `table_id` is not a known ID of a table.
/// - [`spacetimedb_primitives::errno::NO_SUCH_INDEX`]
///   when `index_id` is not a known ID of an index.
/// - [`spacetimedb_primitives::errno::INDEX_NOT_UNIQUE`]
///   when the index was not unique.
/// - [`spacetimedb_primitives::errno::`BSATN_DECODE_ERROR`]
///   when `row` cannot be decoded to a `ProductValue`.
///   typed at the `ProductType` the table's schema specifies
///   or when it cannot be projected to the index identified by `index_id`.
/// - [`spacetimedb_primitives::errno::`NO_SUCH_ROW`]
///   when the row was not found in the unique index.
/// - [`spacetimedb_primitives::errno::`UNIQUE_ALREADY_EXISTS`]
///   when inserting `row` would violate a unique constraint.
/// - [`spacetimedb_primitives::errno::`SCHEDULE_AT_DELAY_TOO_LONG`]
///   when the delay specified in the row was too long.
///
/// Throws a `TypeError` if:
/// - `table_id` is not a `u32`.
/// - `row` is not an array of `u8`s.
fn datastore_update_bsatn(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<Vec<u8>> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;
    let index_id: IndexId = deserialize_js(scope, args.get(1))?;
    let mut row: Vec<u8> = deserialize_js(scope, args.get(2))?;

    // Insert the row into the DB and write back the generated column values.
    let row_len = get_env(scope)?.instance_env.update(table_id, index_id, &mut row)?;
    row.truncate(row_len);

    Ok(row)
}

/// Module ABI that deletes all rows found in the index identified by `index_id`,
/// according to `prefix`, `rstart`, and `rend`.
///
/// This syscall will delete all the rows found by
/// [`datastore_index_scan_range_bsatn`] with the same arguments passed,
/// including `prefix_elems`.
/// See `datastore_index_scan_range_bsatn` for details.
///
/// # Signature
///
/// ```ignore
/// datastore_index_scan_range_bsatn(
///     index_id: u32,
///     prefix: u8[],
///     prefix_elems: u16,
///     rstart: u8[],
///     rend: u8[],
/// ) -> u32 throws {
///    __code_error__: NOT_IN_TRANSACTION | NO_SUCH_INDEX | BSATN_DECODE_ERROR
/// }
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns a `u32` that is the number of rows deleted.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_INDEX`]
///   when `index_id` is not a known ID of an index.
///
/// - [`spacetimedb_primitives::errno::BSATN_DECODE_ERROR`]
///   when `prefix` cannot be decoded to
///   a `prefix_elems` number of `AlgebraicValue`
///   typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
///   Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
///   where the inner `AlgebraicValue`s are
///   typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
///
/// Throws a `TypeError` if:
/// - `table_id` is not a `u32`.
/// - `prefix`, `rstart`, and `rend` are not arrays of `u8`s.
/// - `prefix_elems` is not a `u16`.
fn datastore_delete_by_index_scan_range_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let index_id: IndexId = deserialize_js(scope, args.get(0))?;
    let mut prefix: &[u8] = deserialize_js(scope, args.get(1))?;
    let prefix_elems: ColId = deserialize_js(scope, args.get(2))?;
    let rstart: &[u8] = deserialize_js(scope, args.get(3))?;
    let rend: &[u8] = deserialize_js(scope, args.get(4))?;

    if prefix_elems.idx() == 0 {
        prefix = &[];
    }

    // Delete the relevant rows.
    let count = get_env(scope)?
        .instance_env
        .datastore_delete_by_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)?;
    Ok(count)
}

/// Module ABI that deletes those rows, in the table identified by `table_id`,
/// that match any row in `relation`.
///
/// Matching is defined by first BSATN-decoding
/// the array of bytes `relation` to a `Vec<ProductValue>`
/// according to the row schema of the table
/// and then using `Ord for AlgebraicValue`.
/// A match happens when `Ordering::Equal` is returned from `fn cmp`.
/// This occurs exactly when the row's BSATN-encoding is equal to the encoding of the `ProductValue`.
///
/// # Signature
///
/// ```ignore
/// datastore_delete_all_by_eq_bsatn(table_id: u32, relation: u8[]) -> u32 throws {
///    __code_error__: NOT_IN_TRANSACTION | NO_SUCH_INDEX | BSATN_DECODE_ERROR
/// }
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns a `u32` that is the number of rows deleted.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_TABLE`]
///   when `table_id` is not a known ID of a table.
///
/// - [`spacetimedb_primitives::errno::BSATN_DECODE_ERROR`]
///   when `relation` cannot be decoded to `Vec<ProductValue>`
///   where each `ProductValue` is typed at the `ProductType` the table's schema specifies.
///
/// Throws a `TypeError` if:
/// - `table_id` is not a `u32`.
/// - `relation` is not an array of `u8`s.
fn datastore_delete_all_by_eq_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;
    let relation: &[u8] = deserialize_js(scope, args.get(1))?;

    let count = get_env(scope)?
        .instance_env
        .datastore_delete_all_by_eq_bsatn(table_id, relation)?;
    Ok(count)
}

fn datastore_index_scan_point_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let index_id: IndexId = deserialize_js(scope, args.get(0))?;
    let point: &[u8] = deserialize_js(scope, args.get(1))?;

    let env = get_env(scope)?;

    // Find the relevant rows.
    let chunks = env
        .instance_env
        .datastore_index_scan_point_bsatn_chunks(&mut env.chunk_pool, index_id, point)?;

    // Insert the encoded + concatenated rows into a new buffer and return its id.
    Ok(env.iters.insert(chunks.into_iter()).0)
}

fn datastore_delete_by_index_scan_point_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let index_id: IndexId = deserialize_js(scope, args.get(0))?;
    let point: &[u8] = deserialize_js(scope, args.get(1))?;

    // Delete the relevant rows.
    let count = get_env(scope)?
        .instance_env
        .datastore_delete_by_index_scan_point_bsatn(index_id, point)?;
    Ok(count)
}
