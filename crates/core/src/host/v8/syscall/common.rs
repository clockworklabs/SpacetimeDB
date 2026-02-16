use super::super::{
    call_recv_fun,
    de::deserialize_js,
    de::scratch_buf,
    env_on_isolate,
    error::{
        exception_already_thrown, ErrorOrException, ExceptionThrown, JsStackTrace, SysCallError, SysCallResult,
        Throwable, TypeError,
    },
    from_value::cast,
    ser::serialize_to_js,
    util::make_uint8array,
    JsInstanceEnv,
};
use super::{call_call_view, call_call_view_anon, get_registered_hooks, HookFunctions};
use crate::database_logger::{LogLevel, Record};
use crate::error::NodesError;
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::module_host_actor::{
    deserialize_view_rows, run_query_for_view, AnonymousViewOp, ProcedureOp, ViewOp, ViewResult, ViewReturnData,
};
use crate::host::wasm_common::{RowIterIdx, TimingSpan, TimingSpanIdx};
use anyhow::Context;
use bytes::Bytes;
use spacetimedb_datastore::locking_tx_datastore::{FuncCallType, MutTxId, ViewCallInfo};
use spacetimedb_lib::{ConnectionId, Identity, RawModuleDef, Timestamp};
use spacetimedb_primitives::{ColId, IndexId, ProcedureId, TableId};
use spacetimedb_sats::bsatn;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::identifier::Identifier;
use v8::{FunctionCallbackArguments, Isolate, Local, PinScope, Value};

/// Calls the `__call_procedure__` function `fun`.
pub fn call_call_procedure(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    op: ProcedureOp,
) -> Result<Bytes, ErrorOrException<ExceptionThrown>> {
    let fun = hooks.call_procedure.context("`__call_procedure__` was never defined")?;

    let ProcedureOp {
        id: ProcedureId(procedure_id),
        name: _,
        caller_identity: sender,
        caller_connection_id: connection_id,
        timestamp,
        arg_bytes: procedure_args,
    } = op;
    // Serialize the arguments.
    let procedure_id = serialize_to_js(scope, &procedure_id)?;
    let sender = serialize_to_js(scope, &sender.to_u256())?;
    let connection_id = serialize_to_js(scope, &connection_id.to_u128())?;
    let timestamp = serialize_to_js(scope, &timestamp.to_micros_since_unix_epoch())?;
    let procedure_args = serialize_to_js(scope, &procedure_args)?;
    let args = &[procedure_id, sender, connection_id, timestamp, procedure_args];

    // Call the function.
    let ret = call_recv_fun(scope, fun, hooks.recv, args)?;

    // Deserialize the user result.
    let ret =
        cast!(scope, ret, v8::Uint8Array, "bytes return from `__call_procedure__`").map_err(|e| e.throw(scope))?;
    let bytes = ret.get_contents(&mut []);

    Ok(Bytes::copy_from_slice(bytes))
}

/// Calls the registered `__describe_module__` function hook.
pub fn call_describe_module(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
) -> Result<RawModuleDef, ErrorOrException<ExceptionThrown>> {
    // Call the function.
    let raw_mod_js = call_recv_fun(scope, hooks.describe_module, hooks.recv, &[])?;

    // Deserialize the raw module.
    let raw_mod = cast!(
        scope,
        raw_mod_js,
        v8::Uint8Array,
        "bytes return from `__describe_module__`"
    )
    .map_err(|e| e.throw(scope))?;

    let bytes = raw_mod.get_contents(&mut []);
    let module = bsatn::from_slice::<RawModuleDef>(bytes).context("invalid bsatn module def")?;
    Ok(module)
}

/// Returns the environment or errors.
pub fn get_env(isolate: &mut Isolate) -> SysCallResult<&mut JsInstanceEnv> {
    env_on_isolate(isolate).ok_or(SysCallError::NoEnv)
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
pub fn table_id_from_name(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<TableId> {
    let name: String = deserialize_js(scope, args.get(0))?;
    Ok(get_env(scope)?.instance_env.table_id_from_name(&name)?)
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
pub fn index_id_from_name(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<IndexId> {
    let name: String = deserialize_js(scope, args.get(0))?;
    Ok(get_env(scope)?.instance_env.index_id_from_name(&name)?)
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
pub fn datastore_table_row_count(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u64> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;
    Ok(get_env(scope)?.instance_env.datastore_table_row_count(table_id)?)
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
pub fn datastore_table_scan_bsatn(
    scope: &mut PinScope<'_, '_>,
    args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u32> {
    let table_id: TableId = deserialize_js(scope, args.get(0))?;

    let env = get_env(scope)?;
    // Collect the iterator chunks.
    let chunks = env
        .instance_env
        .datastore_table_scan_bsatn_chunks(&mut env.chunk_pool, table_id)?;

    // Register the iterator and get back the index to write to `out`.
    // Calls to the iterator are done through dynamic dispatch.
    Ok(env.iters.insert(chunks.into_iter()).0)
}

/// This is a helper function that is used by
/// `v1::datastore_index_scan_range_bsatn`
/// and `v2::datastore_index_scan_range_bsatn`.
/// See those for additional details.
pub fn datastore_index_scan_range_bsatn_inner(
    scope: &mut PinScope<'_, '_>,
    index_id: IndexId,
    mut prefix: &[u8],
    prefix_elems: ColId,
    rstart: &[u8],
    rend: &[u8],
) -> SysCallResult<u32> {
    if prefix_elems.idx() == 0 {
        prefix = &[];
    }

    let env = get_env(scope)?;

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

pub fn deserialize_row_iter_idx(scope: &mut PinScope<'_, '_>, value: Local<'_, Value>) -> SysCallResult<RowIterIdx> {
    deserialize_js(scope, value).map(RowIterIdx).map_err(Into::into)
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
pub fn row_iter_bsatn_close<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<()> {
    let row_iter_idx: u32 = deserialize_js(scope, args.get(0))?;
    let row_iter_idx = RowIterIdx(row_iter_idx);

    // Retrieve the iterator by `row_iter_idx`, or error.
    let env = get_env(scope)?;

    // Retrieve the iterator by `row_iter_idx`, or error.
    if env.iters.take(row_iter_idx).is_none() {
        return Err(SysCallError::NO_SUCH_ITER);
    } else {
        // TODO(Centril): consider putting these into a pool for reuse.
    }

    Ok(())
}

/// # Signature
///
/// ```ignore
/// volatile_nonatomic_schedule_immediate(reducer_name: string, args: u8[]) -> undefined
/// ```
pub fn volatile_nonatomic_schedule_immediate<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<()> {
    let name: String = deserialize_js(scope, args.get(0))?;
    let args: Vec<u8> = deserialize_js(scope, args.get(1))?;

    get_env(scope)?
        .instance_env
        .scheduler
        .volatile_nonatomic_schedule_immediate(name, crate::host::FunctionArgs::Bsatn(args.into()));

    Ok(())
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
pub fn console_log<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<()> {
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

    let env = get_env(scope).inspect_err(|_| {
        tracing::warn!(
            "{}:{} {msg}",
            filename.as_deref().unwrap_or("unknown"),
            frame.get_line_number()
        );
    })?;

    let function = env.log_record_function();
    let record = Record {
        // TODO: figure out whether to use walltime now or logical reducer now (env.reducer_start)
        ts: InstanceEnv::now_for_logging(),
        target: None,
        filename: filename.as_deref(),
        line_number: Some(frame.get_line_number() as u32),
        function,
        message: &msg,
    };

    env.instance_env.console_log(level, &record, &trace);

    Ok(())
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
pub fn console_timer_start<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<u32> {
    let name = args.get(0).cast::<v8::String>();
    let mut buf = scratch_buf::<128>();
    let name = name.to_rust_cow_lossy(scope, &mut buf).into_owned();

    let span_id = get_env(scope)?.timing_spans.insert(TimingSpan::new(name)).0;
    Ok(span_id)
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
pub fn console_timer_end<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<()> {
    let span_id: u32 = deserialize_js(scope, args.get(0))?;

    let env = get_env(scope)?;
    let span = env
        .timing_spans
        .take(TimingSpanIdx(span_id))
        .ok_or(SysCallError::NO_SUCH_CONSOLE_TIMER)?;
    let function = env.log_record_function();
    env.instance_env.console_timer_end(&span, function);

    Ok(())
}

/// Module ABI to read a JWT payload associated with a connection ID from the system tables.
///
/// # Signature
///
/// ```ignore
/// get_jwt_payload(connection_id: u128) -> u8[] throws {
///     __code_error__:
///         NOT_IN_TRANSACTION
/// }
/// ```
///
/// # Types
///
/// - `u128` is `bigint` in JS restricted to unsigned 128-bit integers.
///
/// # Returns
///
/// Returns a byte array encoding the JWT payload if one is found. If one is not found, an
/// empty byte array is returned.
///
/// # Throws
///
/// Throws `{ __code_error__: u16 }` where `__code_error__` is:
///
/// - [`spacetimedb_primitives::errno::NOT_IN_TRANSACTION`]
///   when called outside of a transaction.
pub fn get_jwt_payload(scope: &mut PinScope<'_, '_>, args: FunctionCallbackArguments<'_>) -> SysCallResult<Vec<u8>> {
    let connection_id: u128 = deserialize_js(scope, args.get(0))?;
    let connection_id = ConnectionId::from_u128(connection_id);
    let payload = get_env(scope)?
        .instance_env
        .get_jwt_payload(connection_id)?
        .map(String::into_bytes)
        .unwrap_or_default();
    Ok(payload)
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
pub fn identity<'scope>(
    scope: &mut PinScope<'scope, '_>,
    _: FunctionCallbackArguments<'scope>,
) -> SysCallResult<Identity> {
    Ok(*get_env(scope)?.instance_env.database_identity())
}

/// Execute an HTTP request in the context of a procedure.
///
/// # Signature
///
/// ```ignore
/// function procedure_http_request(
///     request: Uint8Array,
///     body: Uint8Array | string
/// ): [response: Uint8Array, body: Uint8Array];
/// ```
///
/// Accepts a BSATN-encoded [`spacetimedb_lib::http::Request`] and a request body, and
/// returns a BSATN-encoded [`spacetimedb_lib::http::Response`] and the response body.
pub fn procedure_http_request<'scope>(
    scope: &mut PinScope<'scope, '_>,
    args: FunctionCallbackArguments<'scope>,
) -> SysCallResult<Local<'scope, v8::Array>> {
    use spacetimedb_lib::http as st_http;

    let request =
        cast!(scope, args.get(0), v8::Uint8Array, "Uint8Array for procedure request").map_err(|e| e.throw(scope))?;

    let request = bsatn::from_slice::<st_http::Request>(request.get_contents(&mut []))
        .map_err(|e| TypeError(format!("failed to decode http request: {e}")).throw(scope))?;

    let request_body = args.get(1);
    let request_body = if let Ok(s) = request_body.try_cast::<v8::String>() {
        Bytes::from(s.to_rust_string_lossy(scope))
    } else {
        let bytes = cast!(
            scope,
            request_body,
            v8::Uint8Array,
            "Uint8Array or string for request body"
        )
        .map_err(|e| e.throw(scope))?;
        Bytes::copy_from_slice(bytes.get_contents(&mut []))
    };

    let env = get_env(scope)?;

    let fut = env.instance_env.http_request(request, request_body)?;

    let rt = tokio::runtime::Handle::current();
    let (response, response_body) = rt.block_on(fut)?;

    let response = bsatn::to_vec(&response).expect("failed to serialize `HttpResponse`");
    let response = make_uint8array(scope, response);

    let response_body = match response_body.try_into_mut() {
        Ok(bytes_mut) => make_uint8array(scope, Box::new(bytes_mut)),
        Err(bytes) => make_uint8array(scope, Vec::from(bytes)),
    };

    Ok(v8::Array::new_with_elements(
        scope,
        &[response.into(), response_body.into()],
    ))
}

pub fn procedure_start_mut_tx(
    scope: &mut PinScope<'_, '_>,
    _args: FunctionCallbackArguments<'_>,
) -> SysCallResult<u64> {
    let env = get_env(scope)?;

    env.instance_env.start_mutable_tx()?;

    let timestamp = Timestamp::now().to_micros_since_unix_epoch() as u64;

    Ok(timestamp)
}

pub fn procedure_abort_mut_tx(scope: &mut PinScope<'_, '_>, _args: FunctionCallbackArguments<'_>) -> SysCallResult<()> {
    let env = get_env(scope)?;

    env.instance_env.abort_mutable_tx()?;
    Ok(())
}

pub fn procedure_commit_mut_tx(
    scope: &mut PinScope<'_, '_>,
    _args: FunctionCallbackArguments<'_>,
) -> SysCallResult<()> {
    let tx = {
        let env = get_env(scope)?;
        env.instance_env.take_mutable_tx_for_commit()?
    };

    let hooks = get_registered_hooks(scope).ok_or_else(|| {
        SysCallError::Exception(
            TypeError("module hooks are unavailable while committing a procedure transaction").throw(scope),
        )
    })?;
    let module_def = get_env(scope)?.module_def().ok_or_else(|| {
        SysCallError::Exception(
            TypeError("module definition is unavailable while committing a procedure transaction").throw(scope),
        )
    })?;
    let tx = refresh_views(scope, tx, &hooks, &module_def)?;
    get_env(scope)?.instance_env.commit_procedure_tx(tx)?;

    Ok(())
}

/// Refresh all views made stale by a procedure `tx`.
///
/// This runs each pending view call in the same mutable transaction and writes the refreshed rows
/// into the corresponding backing view tables. If any step fails (missing metadata, view execution,
/// row decoding, SQL execution, or materialization), this method rolls back `tx` and returns an error.
///
/// On success, it returns the same transaction handle so the caller can commit it.
fn refresh_views(
    scope: &mut PinScope<'_, '_>,
    tx: MutTxId,
    hooks: &HookFunctions<'_>,
    module_def: &ModuleDef,
) -> SysCallResult<MutTxId> {
    let views_for_refresh = tx.views_for_refresh().cloned().collect::<Vec<_>>();
    let stdb = get_env(scope)?.instance_env.relational_db().clone();
    let database_identity = *get_env(scope)?.instance_env.database_identity();
    let mut tx_slot = get_env(scope)?.instance_env.tx.clone();
    let mut tx = Some(tx);

    for view_call in views_for_refresh {
        let res: SysCallResult<()> = (|| {
            let view_def = module_def
                .get_view_by_id(view_call.fn_ptr, view_call.sender.is_none())
                .ok_or_else(|| {
                    SysCallError::Exception(
                        TypeError(format!(
                            "view with fn_ptr `{}` not found while refreshing procedure transaction",
                            view_call.fn_ptr
                        ))
                        .throw(scope),
                    )
                })?;

            let current_tx = tx.take().expect("procedure tx missing during view refresh");
            let (next_tx, call_result) =
                tx_slot.set(current_tx, || call_view(scope, hooks, &view_call, &view_def.name));
            tx = Some(next_tx);
            let return_data = call_result?;

            let typespace = module_def.typespace();
            let row_product_type = typespace
                .resolve(view_def.product_type_ref)
                .resolve_refs()
                .map_err(|err| {
                    SysCallError::Exception(
                        TypeError(format!(
                            "failed resolving row type for refreshed view `{}`: {err}",
                            view_def.name
                        ))
                        .throw(scope),
                    )
                })?
                .into_product()
                .map_err(|_| {
                    SysCallError::Exception(
                        TypeError(format!(
                            "failed resolving row product type for refreshed view `{}`",
                            view_def.name
                        ))
                        .throw(scope),
                    )
                })?;

            let rows = match ViewResult::from_return_data(return_data).map_err(|err| {
                SysCallError::Exception(
                    TypeError(format!(
                        "failed parsing result for refreshed view `{}`: {err}",
                        view_def.name
                    ))
                    .throw(scope),
                )
            })? {
                ViewResult::Rows(bytes) => {
                    deserialize_view_rows(view_def.product_type_ref, bytes, typespace).map_err(NodesError::from)?
                }
                ViewResult::RawSql(query) => run_query_for_view(
                    tx.as_mut().expect("procedure tx missing while running view query"),
                    &query,
                    &row_product_type,
                    &view_call,
                    database_identity,
                )
                .map_err(|err| {
                    SysCallError::Exception(
                        TypeError(format!(
                            "failed running query for refreshed view `{}`: {err}",
                            view_def.name
                        ))
                        .throw(scope),
                    )
                })?,
            };

            match view_call.sender {
                Some(sender) => stdb
                    .materialize_view(
                        tx.as_mut()
                            .expect("procedure tx missing while materializing authenticated view"),
                        view_call.table_id,
                        sender,
                        rows,
                    )
                    .map_err(NodesError::from)?,
                None => stdb
                    .materialize_anonymous_view(
                        tx.as_mut()
                            .expect("procedure tx missing while materializing anonymous view"),
                        view_call.table_id,
                        rows,
                    )
                    .map_err(NodesError::from)?,
            }

            Ok(())
        })();

        if let Err(err) = res {
            let tx = tx.expect("procedure tx missing while rolling back failed view refresh");
            get_env(scope)?.instance_env.rollback_procedure_tx(tx);
            return Err(err);
        }
    }

    Ok(tx.expect("procedure tx missing after refreshing views"))
}

/// Execute a view and return its payload.
///
/// This helper is used by [`refresh_views`] while a procedure transaction is being committed.
/// It temporarily sets the active function type to the target view for dependency tracking,
/// invokes the applicable JS hook, restores the previous function type, and returns [`ViewReturnData`].
fn call_view(
    scope: &mut PinScope<'_, '_>,
    hooks: &HookFunctions<'_>,
    view_call: &ViewCallInfo,
    view_name: &Identifier,
) -> SysCallResult<ViewReturnData> {
    let prev_func_type = get_env(scope)?
        .instance_env
        .swap_func_type(FuncCallType::View(view_call.clone()));

    let result = {
        let args = crate::host::ArgsTuple::nullary();
        match view_call.sender {
            Some(sender) => call_call_view(
                scope,
                hooks,
                ViewOp {
                    name: view_name,
                    view_id: view_call.view_id,
                    table_id: view_call.table_id,
                    fn_ptr: view_call.fn_ptr,
                    args: &args,
                    sender: &sender,
                    timestamp: Timestamp::now(),
                },
            ),
            None => call_call_view_anon(
                scope,
                hooks,
                AnonymousViewOp {
                    name: view_name,
                    view_id: view_call.view_id,
                    table_id: view_call.table_id,
                    fn_ptr: view_call.fn_ptr,
                    args: &args,
                    timestamp: Timestamp::now(),
                },
            ),
        }
    };

    get_env(scope)?.instance_env.swap_func_type(prev_func_type);

    result.map_err(|err| match err {
        ErrorOrException::Err(err) => SysCallError::Exception(
            TypeError(format!(
                "failed executing refreshed view `{}` during procedure commit: {err}",
                view_name
            ))
            .throw(scope),
        ),
        ErrorOrException::Exception(exc) => SysCallError::Exception(exc),
    })
}
