use super::de::{deserialize_js, scratch_buf};
use super::error::{module_exception, ExcResult, ExceptionThrown};
use super::ser::serialize_to_js;
use super::string::{str_from_ident, StringConst};
use super::{
    env_on_isolate, exception_already_thrown, BufferTooSmall, CodeError, JsInstanceEnv, JsStackTrace, TerminationError,
    Throwable,
};
use crate::database_logger::{LogLevel, Record};
use crate::error::NodesError;
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::instrumentation::span;
use crate::host::wasm_common::{err_to_errno_and_log, RowIterIdx, TimingSpan, TimingSpanIdx};
use crate::host::AbiCall;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::{errno, ColId, IndexId, TableId};
use spacetimedb_sats::Serialize;
use v8::{
    callback_scope, ConstructorBehavior, Context, FixedArray, Function, FunctionCallbackArguments, Local, Module,
    PinCallbackScope, PinScope, Value,
};

/// A dependency resolver for the user's module
/// that will resolve `spacetimedb_sys` to a module that exposes the ABI.
pub(super) fn resolve_sys_module<'s>(
    context: Local<'s, Context>,
    spec: Local<'s, v8::String>,
    _attrs: Local<'s, FixedArray>,
    _referrer: Local<'s, Module>,
) -> Option<Local<'s, Module>> {
    callback_scope!(unsafe scope, context);

    if spec == SYS_MODULE_NAME.string(scope) {
        Some(register_sys_module(scope))
    } else {
        module_exception(scope, spec).throw(scope);
        None
    }
}

/// Registers all module -> host syscalls in the JS module `spacetimedb_sys`.
fn register_sys_module<'scope>(scope: &mut PinScope<'scope, '_>) -> Local<'scope, Module> {
    let module_name = SYS_MODULE_NAME.string(scope);

    macro_rules! create_synthetic_module {
        ($(($wrapper:ident, $abi_call:expr, $fun:ident),)*) => {{
            let export_names = &[$(str_from_ident!($fun).string(scope)),*];
            let eval_steps = |context, module| {
                callback_scope!(unsafe scope, context);
                $(
                    register_module_fun(scope, &module, str_from_ident!($fun), |s, a| {
                        $wrapper($abi_call, s, a, $fun)
                    })?;
                )*

                Some(v8::undefined(scope).into())
            };

            Module::create_synthetic_module(scope, module_name, export_names, eval_steps)
        }}
    }

    create_synthetic_module!(
        (with_sys_result, AbiCall::TableIdFromName, table_id_from_name),
        (with_sys_result, AbiCall::IndexIdFromName, index_id_from_name),
        (
            with_sys_result,
            AbiCall::DatastoreTableRowCount,
            datastore_table_row_count
        ),
        (
            with_sys_result,
            AbiCall::DatastoreTableScanBsatn,
            datastore_table_scan_bsatn
        ),
        (
            with_sys_result,
            AbiCall::DatastoreIndexScanRangeBsatn,
            datastore_index_scan_range_bsatn
        ),
        (with_sys_result, AbiCall::RowIterBsatnAdvance, row_iter_bsatn_advance),
        (with_span, AbiCall::RowIterBsatnClose, row_iter_bsatn_close),
        (with_sys_result, AbiCall::DatastoreInsertBsatn, datastore_insert_bsatn),
        (with_sys_result, AbiCall::DatastoreUpdateBsatn, datastore_update_bsatn),
        (
            with_sys_result,
            AbiCall::DatastoreDeleteByIndexScanRangeBsatn,
            datastore_delete_by_index_scan_range_bsatn
        ),
        (
            with_sys_result,
            AbiCall::DatastoreDeleteAllByEqBsatn,
            datastore_delete_all_by_eq_bsatn
        ),
        (
            with_span,
            AbiCall::VolatileNonatomicScheduleImmediate,
            volatile_nonatomic_schedule_immediate
        ),
        (with_span, AbiCall::ConsoleLog, console_log),
        (with_span, AbiCall::ConsoleTimerStart, console_timer_start),
        (with_span, AbiCall::ConsoleTimerEnd, console_timer_end),
        (with_sys_result, AbiCall::Identity, identity),
    )
}

const SYS_MODULE_NAME: &StringConst = str_from_ident!(spacetimedb_sys);

/// The return type of a module -> host syscall.
pub(super) type FnRet<'scope> = ExcResult<Local<'scope, Value>>;

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

/// A flag set in [`handle_nodes_error`].
/// The flag should be checked in every module -> host ABI.
/// If the flag is set, the call is prevented.
struct TerminationFlag;

/// Adapts `fun`, which returns a [`Value`] to one that works on [`v8::ReturnValue`].
fn adapt_fun(
    fun: impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> FnRet<'scope>,
) -> impl Copy + for<'scope> Fn(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>, v8::ReturnValue) {
    move |scope, args, mut rv| {
        // If the flag was set in `handle_nodes_error`,
        // we need to block all module -> host ABI calls.
        if scope.get_slot::<TerminationFlag>().is_some() {
            let err = anyhow::anyhow!("execution is being terminated");
            if let Ok(exception) = TerminationError::from_error(scope, &err) {
                exception.throw(scope);
            }
            return;
        }

        // Set the result `value` on success.
        if let Ok(value) = fun(scope, args) {
            rv.set(value);
        }
    }
}

/// Either an exception, already thrown, or [`NodesError`] arising from [`InstanceEnv`].
#[derive(derive_more::From)]
enum SysCallError {
    Error(NodesError),
    Exception(ExceptionThrown),
}

type SysCallResult<T> = Result<T, SysCallError>;

/// Wraps `run` in [`with_span`] and returns the return value of `run` to JS.
/// Handles [`SysCallError`] if it occurs by throwing exceptions into JS.
fn with_sys_result<'scope, O: Serialize>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    run: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> SysCallResult<O>,
) -> FnRet<'scope> {
    match with_span(abi_call, scope, args, run) {
        Ok(ret) => serialize_to_js(scope, &ret),
        Err(SysCallError::Exception(exc)) => Err(exc),
        Err(SysCallError::Error(error)) => Err(throw_nodes_error(abi_call, scope, error)),
    }
}

/// Turns a [`NodesError`] into a thrown exception.
fn throw_nodes_error(abi_call: AbiCall, scope: &mut PinScope<'_, '_>, error: NodesError) -> ExceptionThrown {
    let res = match err_to_errno_and_log::<u16>(abi_call, error) {
        Ok(code) => CodeError::from_code(scope, code),
        Err(err) => {
            // Terminate execution ASAP and throw a catchable exception (`TerminationError`).
            // Unfortunately, JS execution won't be terminated once the callback returns,
            // so we set a slot that all callbacks immediately check
            // to ensure that the module won't be able to do anything to the host
            // while it's being terminated (eventually).
            scope.terminate_execution();
            scope.set_slot(TerminationFlag);
            TerminationError::from_error(scope, &err)
        }
    };
    collapse_exc_thrown(scope, res)
}

/// Collapses `res` where the `Ok(x)` where `x` is throwable.
fn collapse_exc_thrown<'scope>(
    scope: &PinScope<'scope, '_>,
    res: ExcResult<impl Throwable<'scope>>,
) -> ExceptionThrown {
    let (Ok(thrown) | Err(thrown)) = res.map(|ev| ev.throw(scope));
    thrown
}

/// Tracks the span of `body` under the label `abi_call`.
fn with_span<'scope, R>(
    abi_call: AbiCall,
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
    body: impl FnOnce(&mut PinScope<'scope, '_>, FunctionCallbackArguments<'scope>) -> R,
) -> R {
    // Start the span.
    let span_start = span::CallSpanStart::new(abi_call);

    // Call `fun` with `args` in `scope`.
    let result = body(scope, args);

    // Track the span of this call.
    let span = span_start.end();
    span::record_span(&mut env_on_isolate(scope).call_times, span);

    result
}

/// Module ABI that finds the `TableId` for a table name.
///
/// # Signature
///
/// ```ignore
/// table_id_from_name(name: string) -> u32 throws {
///     __code_error__: NOT_IN_TRANSACTION | NO_SUCH_TABLE
/// }
/// ```
///
/// # Types
///
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns an `u32` containing the id of the table.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_TABLE`]
///   when `name` is not the name of a table.
///
/// Throws a `TypeError` if:
/// - `name` is not `string`.
fn table_id_from_name(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<TableId> {
    let name: &str = deserialize_js(scope, args.get(0))?;
    Ok(env_on_isolate(scope).instance_env.table_id_from_name(name)?)
}

/// Module ABI that finds the `IndexId` for an index name.
///
/// # Signature
///
/// ```ignore
/// index_id_from_name(name: string) -> u32 throws {
///     __code_error__: NOT_IN_TRANSACTION | NO_SUCH_INDEX
/// }
/// ```
///
/// # Types
///
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns an `u32` containing the id of the index.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_INDEX`]
///   when `name` is not the name of an index.
///
/// Throws a `TypeError`:
/// - if `name` is not `string`.
fn index_id_from_name(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<IndexId> {
    let name: &str = deserialize_js(scope, args.get(0))?;
    Ok(env_on_isolate(scope).instance_env.index_id_from_name(name)?)
}

/// Module ABI that returns the number of rows currently in table identified by `table_id`.
///
/// # Signature
///
/// ```ignore
/// datastore_table_row_count(table_id: u32) -> u64 throws {
///     __code_error__: NOT_IN_TRANSACTION | NO_SUCH_TABLE
/// }
/// ```
///
/// # Types
///
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
/// - `u64` is `bigint` in JS restricted to unsigned 64-bit integers.
///
/// # Returns
///
/// Returns a `u64` containing the number of rows in the table.
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
/// Throws a `TypeError` if:
/// - `table_id` is not a `u32`.
fn datastore_table_row_count(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<u64> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;
    Ok(env_on_isolate(scope).instance_env.datastore_table_row_count(table_id)?)
}

/// Module ABI that starts iteration on each row, as BSATN-encoded,
/// of a table identified by `table_id`.
///
/// # Signature
///
/// ```ignore
/// datastore_table_scan_bsatn(table_id: u32) -> u32 throws {
///     __code_error__: NOT_IN_TRANSACTION | NO_SUCH_TABLE
/// }
/// ```
///
/// # Types
///
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
/// - [`spacetimedb_primitives::errno::NO_SUCH_TABLE`]
///   when `table_id` is not a known ID of a table.
///
/// Throws a `TypeError`:
/// - if `table_id` is not a `u32`.
fn datastore_table_scan_bsatn(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<u32> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;

    let env = env_on_isolate(scope);
    // Collect the iterator chunks.
    let chunks = env
        .instance_env
        .datastore_table_scan_bsatn_chunks(&mut env.chunk_pool, table_id)?;

    // Register the iterator and get back the index to write to `out`.
    // Calls to the iterator are done through dynamic dispatch.
    Ok(env.iters.insert(chunks.into_iter()).0)
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
/// On success, the iterator handle is written to the `out` pointer.
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
/// - `table_id` is not a `u32`.
/// - `prefix`, `rstart`, and `rend` are not arrays of `u8`s.
/// - `prefix_elems` is not a `u16`.
fn datastore_index_scan_range_bsatn(
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

    let env = env_on_isolate(scope);

    // Find the relevant rows.
    let chunks = env.instance_env.datastore_index_scan_range_bsatn_chunks(
        &mut env.chunk_pool,
        index_id,
        prefix,
        prefix_elems,
        rstart,
        rend,
    )?;

    // Insert the encoded + concatenated rows into a new buffer and return its id.
    Ok(env.iters.insert(chunks.into_iter()).0)
}

/// Throws `{ __code_error__: NO_SUCH_ITER }`.
fn no_such_iter(scope: &PinScope<'_, '_>) -> ExceptionThrown {
    let res = CodeError::from_code(scope, errno::NO_SUCH_ITER.get());
    collapse_exc_thrown(scope, res)
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
    let row_iter_idx: u32 = deserialize_js(scope, args.get(0))?;
    let row_iter_idx = RowIterIdx(row_iter_idx);
    let buffer_max_len: u32 = deserialize_js(scope, args.get(1))?;

    // Retrieve the iterator by `row_iter_idx`, or error.
    let env = env_on_isolate(scope);
    let Some(iter) = env.iters.get_mut(row_iter_idx) else {
        return Err(no_such_iter(scope).into());
    };

    // Allocate a buffer with `buffer_max_len` capacity.
    let mut buffer = vec![0; buffer_max_len as usize];
    // Fill the buffer as much as possible.
    let written = InstanceEnv::fill_buffer_from_iter(iter, &mut buffer, &mut env.chunk_pool);
    buffer.truncate(written);

    match (written, iter.as_slice().first().map(|c| c.len().try_into().unwrap())) {
        // Nothing was written and the iterator is not exhausted.
        (0, Some(min_len)) => {
            let exc = BufferTooSmall::from_requirement(scope, min_len)?;
            Err(exc.throw(scope).into())
        }
        // The iterator is exhausted, destroy it, and tell the caller.
        (_, None) => {
            env.iters.take(row_iter_idx);
            Ok((true, buffer))
        }
        // Something was written, but the iterator is not exhausted.
        (_, Some(_)) => Ok((false, buffer)),
    }
}

/// Module ABI that destroys the iterator registered under `iter`.
///
/// Once `row_iter_bsatn_close` is called on `iter`, the `iter` is invalid.
/// That is, `row_iter_bsatn_close(iter)` the second time will yield `NO_SUCH_ITER`.
///
/// # Signature
///
/// ```ignore
/// row_iter_bsatn_close(iter: u32) -> undefined throws {
///     __code_error__: NO_SUCH_ITER
/// }
/// ```
///
/// # Types
///
/// - `u16` is `number` in JS restricted to unsigned 16-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns nothing.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_ITER`]
///   when `iter` is not a valid iterator.
///
/// Throws a `TypeError` if:
/// - `iter` is not a `u32`.
fn row_iter_bsatn_close<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> FnRet<'scope> {
    let row_iter_idx: u32 = deserialize_js(scope, args.get(0))?;
    let row_iter_idx = RowIterIdx(row_iter_idx);

    // Retrieve the iterator by `row_iter_idx`, or error.
    let env = env_on_isolate(scope);

    // Retrieve the iterator by `row_iter_idx`, or error.
    if env.iters.take(row_iter_idx).is_none() {
        return Err(no_such_iter(scope));
    } else {
        // TODO(Centril): consider putting these into a pool for reuse.
    }

    Ok(v8::undefined(scope).into())
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
    let env: &mut JsInstanceEnv = env_on_isolate(scope);
    let row_len = env.instance_env.insert(table_id, &mut row)?;
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
    let env: &mut JsInstanceEnv = env_on_isolate(scope);
    let row_len = env.instance_env.update(table_id, index_id, &mut row)?;
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

    let env = env_on_isolate(scope);

    // Delete the relevant rows.
    Ok(env
        .instance_env
        .datastore_delete_by_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)?)
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

    let env = env_on_isolate(scope);
    Ok(env.instance_env.datastore_delete_all_by_eq_bsatn(table_id, relation)?)
}

/// # Signature
///
/// ```ignore
/// volatile_nonatomic_schedule_immediate(reducer_name: string, args: u8[]) -> undefined
/// ```
fn volatile_nonatomic_schedule_immediate<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> FnRet<'scope> {
    let name: String = deserialize_js(scope, args.get(0))?;
    let args: Vec<u8> = deserialize_js(scope, args.get(1))?;

    let env = env_on_isolate(scope);
    env.instance_env
        .scheduler
        .volatile_nonatomic_schedule_immediate(name, crate::host::ReducerArgs::Bsatn(args.into()));

    Ok(v8::undefined(scope).into())
}

/// Module ABI that logs at `level` a `message` message occurring
/// at the parent stack frame.
///
/// The `message` is interpreted lossily as a UTF-8 string.
///
/// # Signature
///
/// ```ignore
/// console_log(level: u8, message: string) -> u32
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns nothing.
fn console_log<'scope>(scope: &mut PinScope<'scope, '_>, args: FunctionCallbackArguments<'scope>) -> FnRet<'scope> {
    let level: u32 = deserialize_js(scope, args.get(0))?;

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

    let level = (level as u8).into();
    let trace = if level == LogLevel::Panic {
        JsStackTrace::from_current_stack_trace(scope)?
    } else {
        <_>::default()
    };

    let env = env_on_isolate(scope);

    let function = env.log_record_function();
    let record = Record {
        // TODO: figure out whether to use walltime now or logical reducer now (env.reducer_start)
        ts: chrono::Utc::now(),
        target: None,
        filename: filename.as_deref(),
        line_number: Some(frame.get_line_number() as u32),
        function,
        message: &msg,
    };

    env.instance_env.console_log(level, &record, &trace);

    Ok(v8::undefined(scope).into())
}

/// Module ABI that begins a timing span with `name`.
///
/// When the returned `ConsoleTimerId` is passed to [`console_timer_end`],
/// the duration between the calls will be printed to the module's logs.
///
/// The `name` is interpreted lossily as a UTF-8 string.
///
/// # Signature
///
/// ```ignore
/// console_timer_start(name: string) -> u32
/// ```
///
/// # Types
///
/// - `u8` is `number` in JS restricted to unsigned 8-bit integers.
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns a `u32` that is the `ConsoleTimerId`.
///
/// # Throws
///
/// Throws a `TypeError` if:
/// - `name` is not a `string`.
fn console_timer_start<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> FnRet<'scope> {
    let name = args.get(0).cast::<v8::String>();
    let mut buf = scratch_buf::<128>();
    let name = name.to_rust_cow_lossy(scope, &mut buf).into_owned();

    let env = env_on_isolate(scope);
    let span_id = env.timing_spans.insert(TimingSpan::new(name)).0;
    serialize_to_js(scope, &span_id)
}

/// Module ABI that ends a timing span with `span_id`.
///
/// # Signature
///
/// ```ignore
/// console_timer_end(span_id: u32) -> undefined throws {
///     __code_error__: NO_SUCH_CONSOLE_TIMER
/// }
/// ```
///
/// # Types
///
/// - `u32` is `number` in JS restricted to unsigned 32-bit integers.
///
/// # Returns
///
/// Returns nothing.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NO_SUCH_CONSOLE_TIMER`]
///   when `span_id` doesn't refer to an active timing span.
///
/// Throws a `TypeError` if:
/// - `span_id` is not a `u32`.
fn console_timer_end<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> FnRet<'scope> {
    let span_id: u32 = deserialize_js(scope, args.get(0))?;

    let env = env_on_isolate(scope);
    let Some(span) = env.timing_spans.take(TimingSpanIdx(span_id)) else {
        let exc = CodeError::from_code(scope, errno::NO_SUCH_CONSOLE_TIMER.get())?;
        return Err(exc.throw(scope));
    };
    let function = env.log_record_function();
    env.instance_env.console_timer_end(&span, function);

    Ok(v8::undefined(scope).into())
}

/// Module ABI that returns the module identity.
///
/// # Signature
///
/// ```ignore
/// identity() -> { __identity__: u256 }
/// ```
///
/// # Types
///
/// - `u256` is `bigint` in JS restricted to unsigned 256-bit integers.
///
/// # Returns
///
/// Returns the module identity.
fn identity<'scope>(scope: &mut PinScope<'scope, '_>, _: FunctionCallbackArguments<'scope>) -> SysCallResult<Identity> {
    Ok(*env_on_isolate(scope).instance_env.database_identity())
}
