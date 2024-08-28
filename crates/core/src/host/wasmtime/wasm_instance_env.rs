#![allow(clippy::too_many_arguments)]

use std::ops::DerefMut;
use std::time::Instant;

use crate::database_logger::{BacktraceFrame, BacktraceProvider, ModuleBacktrace, Record};
use crate::execution_context::ExecutionContext;
use crate::host::wasm_common::instrumentation;
use crate::host::wasm_common::module_host_actor::ExecutionTimings;
use crate::host::wasm_common::{
    err_to_errno, instrumentation::CallTimes, AbiRuntimeError, RowIterIdx, RowIters, TimingSpan, TimingSpanIdx,
    TimingSpanSet,
};
use crate::host::AbiCall;
use anyhow::Context as _;
use spacetimedb_primitives::{errno, ColId};
use wasmtime::{AsContext, Caller, StoreContextMut};

use crate::host::instance_env::InstanceEnv;

use super::{Mem, MemView, NullableMemOp, WasmError, WasmPointee, WasmPtr};

#[cfg(not(feature = "spacetimedb-wasm-instance-env-times"))]
use instrumentation::noop as span;
#[cfg(feature = "spacetimedb-wasm-instance-env-times")]
use instrumentation::op as span;

/// A `WasmInstanceEnv` provides the connection between a module
/// and the database.
///
/// A `WasmInstanceEnv` associates an `InstanceEnv` (responsible for
/// the database instance and its associated state) with a wasm
/// `Mem`. It also contains the resources (`Buffers` and
/// `BufferIters`) needed to manage the ABI contract between modules
/// and the host.
///
/// Once created, a `WasmInstanceEnv` must be instantiated with a `Mem`
/// exactly once.
///
/// Some of the state associated to a `WasmInstanceEnv` is per reducer invocation.
/// For instance, module-defined timing spans are per reducer.
pub(super) struct WasmInstanceEnv {
    /// The database `InstanceEnv` associated to this instance.
    instance_env: InstanceEnv,

    /// The `Mem` associated to this instance. At construction time,
    /// this is always `None`. The `Mem` instance is extracted from the
    /// instance exports, and after instantiation is complete, this will
    /// always be `Some`.
    mem: Option<Mem>,

    /// The arguments being passed to a reducer
    /// that it can read via [`Self::bytes_source_read`].
    call_reducer_args: Option<(bytes::Bytes, usize)>,

    /// The standard sink used for [`Self::bytes_sink_write`].
    standard_bytes_sink: Option<Vec<u8>>,

    /// The slab of `BufferIters` created for this instance.
    iters: RowIters,

    /// Track time spent in module-defined spans.
    timing_spans: TimingSpanSet,

    /// The point in time the last reducer call started at.
    reducer_start: Instant,

    /// Track time spent in all wasm instance env calls (aka syscall time).
    ///
    /// Each function, like `insert`, will add the `Duration` spent in it
    /// to this tracker.
    call_times: CallTimes,

    /// The last, including current, reducer to be executed by this environment.
    reducer_name: String,
}

const CALL_REDUCER_ARGS_SOURCE: u32 = 1;
const STANDARD_BYTES_SINK: u32 = 1;

type WasmResult<T> = Result<T, WasmError>;
type RtResult<T> = anyhow::Result<T>;

/// Wraps an `InstanceEnv` with the magic necessary to push
/// and pull bytes from webassembly memory.
impl WasmInstanceEnv {
    /// Create a new `WasmEnstanceEnv` from the given `InstanceEnv`.
    pub fn new(instance_env: InstanceEnv) -> Self {
        let reducer_start = Instant::now();
        Self {
            instance_env,
            mem: None,
            call_reducer_args: None,
            standard_bytes_sink: None,
            iters: Default::default(),
            timing_spans: Default::default(),
            reducer_start,
            call_times: CallTimes::new(),
            reducer_name: String::from(""),
        }
    }

    /// Finish the instantiation of this instance with the provided `Mem`.
    pub fn instantiate(&mut self, mem: Mem) {
        assert!(self.mem.is_none());
        self.mem = Some(mem);
    }

    /// Returns a reference to the memory, assumed to be initialized.
    pub fn get_mem(&self) -> Mem {
        self.mem.expect("Initialized memory")
    }
    fn mem_env<'a>(ctx: impl Into<StoreContextMut<'a, Self>>) -> (&'a mut MemView, &'a mut Self) {
        let ctx = ctx.into();
        let mem = ctx.data().get_mem();
        mem.view_and_store_mut(ctx)
    }

    /// Return a reference to the `InstanceEnv`,
    /// which is responsible for DB instance and associated state.
    pub fn instance_env(&self) -> &InstanceEnv {
        &self.instance_env
    }

    /// Setup the standard bytes sink and return a handle to it for writing.
    pub fn setup_standard_bytes_sink(&mut self) -> u32 {
        self.standard_bytes_sink = Some(Vec::new());
        STANDARD_BYTES_SINK
    }

    /// Extract all the bytes written to the standard bytes sink
    /// and prevent further writes to it.
    pub fn take_standard_bytes_sink(&mut self) -> Vec<u8> {
        self.standard_bytes_sink.take().unwrap_or_default()
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is beginning.
    ///
    /// Returns the handle used by reducers to read from `args`
    /// as well as the handle used to write the error message, if any.
    pub fn start_reducer(&mut self, name: &str, args: bytes::Bytes) -> (u32, u32) {
        let errors = self.setup_standard_bytes_sink();

        // Pass an invalid source when the reducer args were empty.
        // This allows the module to avoid allocating and make a system call in those cases.
        self.call_reducer_args = (!args.is_empty()).then_some((args, 0));
        let args = if self.call_reducer_args.is_some() {
            CALL_REDUCER_ARGS_SOURCE
        } else {
            0
        };

        self.reducer_start = Instant::now();
        name.clone_into(&mut self.reducer_name);

        (args, errors)
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is over.
    /// This resets all of the state associated to a single reducer call,
    /// and returns instrumentation records.
    pub fn finish_reducer(&mut self) -> (ExecutionTimings, Vec<u8>) {
        // For the moment,
        // we only explicitly clear the source/sink buffers and the "syscall" times.
        // TODO: should we be clearing `iters` and/or `timing_spans`?

        let total_duration = self.reducer_start.elapsed();

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        let timings = ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        };

        self.call_reducer_args = None;
        (timings, self.take_standard_bytes_sink())
    }

    /// Returns an execution context for a reducer call.
    fn reducer_context(&self) -> Result<impl DerefMut<Target = ExecutionContext> + '_, WasmError> {
        self.instance_env().get_ctx().map_err(|err| WasmError::Db(err.into()))
    }

    fn with_span<R>(mut caller: Caller<'_, Self>, func: AbiCall, run: impl FnOnce(&mut Caller<'_, Self>) -> R) -> R {
        let span_start = span::CallSpanStart::new(func);

        // Call `run` with the caller and a handle to the memory.
        let result = run(&mut caller);

        // Track the span of this call.
        let span = span_start.end();
        span::record_span(&mut caller.data_mut().call_times, span);

        result
    }

    fn convert_wasm_result<T: From<u16>>(func: AbiCall, err: WasmError) -> RtResult<T> {
        Err(match err {
            WasmError::Db(err) => match err_to_errno(&err) {
                Some(errno) => {
                    log::debug!(
                        "abi call to {func} returned an errno: {errno} ({})",
                        errno::strerror(errno).unwrap_or("<unknown>")
                    );
                    return Ok(errno.get().into());
                }
                None => anyhow::Error::from(AbiRuntimeError { func, err }),
            },
            WasmError::BufferTooSmall => return Ok(errno::BUFFER_TOO_SMALL.get().into()),
            WasmError::Wasm(err) => err,
        })
    }

    /// Call the function `run` with the name `func`.
    /// The function `run` is provided with the callers environment and the host's memory.
    ///
    /// One of `cvt_custom`, `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    ///
    /// Some database errors are logged but are otherwise regarded as `Ok(_)`.
    /// See `err_to_errno` for a list.
    ///
    /// This variant should be used when more control is needed over the success value.
    fn cvt_custom<T: From<u16>>(
        caller: Caller<'_, Self>,
        func: AbiCall,
        run: impl FnOnce(&mut Caller<'_, Self>) -> WasmResult<T>,
    ) -> RtResult<T> {
        Self::with_span(caller, func, run).or_else(|err| Self::convert_wasm_result(func, err))
    }

    /// Call the function `run` with the name `func`.
    /// The function `run` is provided with the callers environment and the host's memory.
    ///
    /// One of `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    ///
    /// Some database errors are logged but are otherwise regarded as `Ok(_)`.
    /// See `err_to_errno` for a list.
    fn cvt<T: From<u16>>(
        caller: Caller<'_, Self>,
        func: AbiCall,
        run: impl FnOnce(&mut Caller<'_, Self>) -> WasmResult<()>,
    ) -> RtResult<T> {
        Self::cvt_custom(caller, func, |c| run(c).map(|()| 0u16.into()))
    }

    /// Call the function `f` with any return value being written to the pointer `out`.
    ///
    /// Otherwise, `cvt_ret` (this function) behaves as `cvt`.
    ///
    /// One of `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    ///
    /// This method should be used as opposed to a manual implementation,
    /// as it helps with upholding the safety invariants of [`bindings_sys::call`].
    ///
    /// Returns an error if writing `T` to `out` errors.
    fn cvt_ret<O: WasmPointee>(
        caller: Caller<'_, Self>,
        call: AbiCall,
        out: WasmPtr<O>,
        f: impl FnOnce(&mut Caller<'_, Self>) -> WasmResult<O>,
    ) -> RtResult<u32> {
        Self::cvt(caller, call, |caller| {
            f(caller).and_then(|ret| {
                let (mem, _) = Self::mem_env(caller);
                ret.write_to(mem, out).map_err(|e| e.into())
            })
        })
    }

    /// Call the function `f`.
    ///
    /// This is the version of `cvt` or `cvt_ret` for functions with no return value.
    /// One of `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    fn cvt_noret(caller: Caller<'_, Self>, call: AbiCall, f: impl FnOnce(&mut Caller<'_, Self>)) {
        Self::with_span(caller, call, f)
    }

    fn convert_u32_to_col_id(col_id: u32) -> WasmResult<ColId> {
        let col_id: u16 = col_id
            .try_into()
            .context("ABI violation, a `ColId` must be a `u16`")
            .map_err(WasmError::Wasm)?;
        Ok(col_id.into())
    }

    /// Log at `level` a `message` message occuring in `filename:line_number`
    /// with [`target`] being the module path at the `log!` invocation site.
    ///
    /// These various pointers are interpreted lossily as UTF-8 strings with a corresponding `_len`.
    ///
    /// The `target` and `filename` pointers are ignored by passing `NULL`.
    /// The line number is ignored if `line_number == u32::MAX`.
    ///
    /// No message is logged if
    /// - `target != NULL && target + target_len > u64::MAX`
    /// - `filename != NULL && filename + filename_len > u64::MAX`
    /// - `message + message_len > u64::MAX`
    ///
    /// [`target`]: https://docs.rs/log/latest/log/struct.Record.html#method.target
    #[tracing::instrument(skip_all)]
    pub fn console_log(
        caller: Caller<'_, Self>,
        level: u32,
        target: WasmPtr<u8>,
        target_len: u32,
        filename: WasmPtr<u8>,
        filename_len: u32,
        line_number: u32,
        message: WasmPtr<u8>,
        message_len: u32,
    ) {
        let do_console_log = |caller: &mut Caller<'_, Self>| -> WasmResult<()> {
            let env = caller.data();
            let mem = env.get_mem().view(&caller);

            // Read the `target`, `filename`, and `message` strings from WASM memory.
            let target = mem.deref_str_lossy(target, target_len).check_nullptr()?;
            let filename = mem.deref_str_lossy(filename, filename_len).check_nullptr()?;
            let message = mem.deref_str_lossy(message, message_len)?;

            // The line number cannot be `u32::MAX` as this represents `Option::None`.
            let line_number = (line_number != u32::MAX).then_some(line_number);

            let record = Record {
                // TODO: figure out whether to use walltime now or logical reducer now (env.reducer_start)
                ts: chrono::Utc::now(),
                target: target.as_deref(),
                filename: filename.as_deref(),
                line_number,
                message: &message,
            };

            // Write the log record to the `DatabaseLogger` in the database instance context (dbic).
            env.instance_env
                .console_log((level as u8).into(), &record, &caller.as_context());
            Ok(())
        };
        Self::cvt_noret(caller, AbiCall::ConsoleLog, |caller| {
            let _ = do_console_log(caller);
        })
    }

    /// Inserts a row into the table identified by `table_id`,
    /// where the row is read from the byte slice `row` in WASM memory,
    /// lasting `row_len` bytes.
    ///
    /// The `(row, row_len)` slice must be a BSATN-encoded `ProductValue`
    /// matching the table's `ProductType` row-schema.
    /// The `row` pointer is written to with the inserted row re-encoded.
    /// This is due to auto-incrementing columns.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - there were unique constraint violations
    /// - `row + row_len` overflows a 64-bit integer
    /// - `(row, row_len)` doesn't decode from BSATN to a `ProductValue`
    ///   according to the `ProductType` that the table's schema specifies.
    #[tracing::instrument(skip_all)]
    pub fn insert(caller: Caller<'_, Self>, table_id: u32, row: WasmPtr<u8>, row_len: u32) -> RtResult<u32> {
        Self::cvt(caller, AbiCall::Insert, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Read the row from WASM memory into a buffer.
            let row_buffer = mem.deref_slice_mut(row, row_len)?;

            // Insert the row into the DB. We get back the decoded version.
            // Then re-encode and write that back into WASM memory at `row`.
            // We're doing this because of autoinc.
            let ctx = env.reducer_context()?;
            let new_row = env.instance_env.insert(&ctx, table_id.into(), row_buffer)?;
            new_row.encode(&mut { row_buffer });
            Ok(())
        })
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the column identified by `cols` matches the byte string,
    /// in WASM memory, pointed to at by `value`.
    ///
    /// Matching is defined by BSATN-decoding `value` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// The number of rows deleted is written to the WASM pointer `out`.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - no columns were deleted
    /// - `col_id` does not identify a column of the table,
    /// - `(value, value_len)` doesn't decode from BSATN to an `AlgebraicValue`
    ///   according to the `AlgebraicType` that the table's schema specifies for `col_id`.
    /// - `value + value_len` overflows a 64-bit integer
    /// - writing to `out` would overflow a 32-bit integer
    pub fn delete_by_col_eq(
        caller: Caller<'_, Self>,
        table_id: u32,
        col_id: u32,
        value: WasmPtr<u8>,
        value_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DeleteByColEq, out, |caller| {
            let col_id = Self::convert_u32_to_col_id(col_id)?;

            let (mem, env) = Self::mem_env(caller);
            let ctx = env.reducer_context()?;
            let value = mem.deref_slice(value, value_len)?;
            let count = env
                .instance_env
                .delete_by_col_eq(&ctx, table_id.into(), col_id, value)?;
            Ok(count)
        })
    }

    /// Deletes those rows, in the table identified by `table_id`,
    /// that match any row in `relation`.
    ///
    /// Matching is defined by first BSATN-decoding
    /// the byte string pointed to at by `relation` to a `Vec<ProductValue>`
    /// according to the row schema of the table
    /// and then using `Ord for AlgebraicValue`.
    ///
    /// The number of rows deleted is written to the WASM pointer `out`.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - `(relation, relation_len)` doesn't decode from BSATN to a `Vec<ProductValue>`
    ///   according to the `ProductValue` that the table's schema specifies for rows.
    /// - `relation + relation_len` overflows a 64-bit integer
    /// - writing to `out` would overflow a 32-bit integer
    #[tracing::instrument(skip_all)]
    pub fn delete_by_rel(
        caller: Caller<'_, Self>,
        table_id: u32,
        relation: WasmPtr<u8>,
        relation_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DeleteByRel, out, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let relation = mem.deref_slice(relation, relation_len)?;
            Ok(env.instance_env.delete_by_rel(table_id.into(), relation)?)
        })
    }

    /// Queries the `table_id` associated with the given (table) `name`
    /// where `name` is the UTF-8 slice in WASM memory at `name_ptr[..name_len]`.
    ///
    /// The table id is written into the `out` pointer.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `name_ptr` is NULL or `name` is not in bounds of WASM memory.
    /// - `name` is not valid UTF-8.
    /// - `out` is NULL or `out[..size_of::<TableId>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `name` is not the name of a table.
    #[tracing::instrument(skip_all)]
    pub fn table_id_from_name(
        caller: Caller<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret::<u32>(caller, AbiCall::TableIdFromName, out, |caller| {
            let (mem, env) = Self::mem_env(caller);
            // Read the table name from WASM memory.
            let name = mem.deref_str(name, name_len)?;

            // Query the table id.
            Ok(env.instance_env.table_id_from_name(name)?.into())
        })
    }

    /// Writes the number of rows currently in table identified by `table_id` to `out`.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `out` is NULL or `out[..size_of::<u64>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `table_id` is not a known ID of a table.
    #[tracing::instrument(skip_all)]
    pub fn datastore_table_row_count(caller: Caller<'_, Self>, table_id: u32, out: WasmPtr<u64>) -> RtResult<u32> {
        Self::cvt_ret::<u64>(caller, AbiCall::DatastoreTableRowCount, out, |caller| {
            let (_, env) = Self::mem_env(caller);
            Ok(env.instance_env.datastore_table_row_count(table_id.into())?)
        })
    }

    /// Finds all rows in the table identified by `table_id`,
    /// where the row has a column, identified by `cols`,
    /// with data matching the byte string, in WASM memory, pointed to at by `val`.
    ///
    /// Matching is defined BSATN-decoding `val` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// The rows found are BSATN-encoded and then concatenated.
    /// The resulting byte string from the concatenation is written
    /// to a fresh buffer with the buffer's identifier written to the WASM pointer `out`.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - `col_id` does not identify a column of the table,
    /// - `(val, val_len)` cannot be decoded to an `AlgebraicValue`
    ///   typed at the `AlgebraicType` of the column,
    /// - `val + val_len` overflows a 64-bit integer
    pub fn iter_by_col_eq(
        caller: Caller<'_, Self>,
        table_id: u32,
        col_id: u32,
        val: WasmPtr<u8>,
        val_len: u32,
        out: WasmPtr<RowIterIdx>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::IterByColEq, out, |caller| {
            let col_id = Self::convert_u32_to_col_id(col_id)?;

            let (mem, env) = Self::mem_env(caller);
            // Read the test value from WASM memory.
            let value = mem.deref_slice(val, val_len)?;

            // Retrieve the execution context for the current reducer.
            let ctx = env.reducer_context()?;

            // Find the relevant rows.
            let chunks = env
                .instance_env
                .iter_by_col_eq_chunks(&ctx, table_id.into(), col_id, value)?;

            // Release the immutable borrow of `env.buffers` by dropping `ctx`.
            drop(ctx);

            // Insert the encoded + concatenated rows into a new buffer and return its id.
            Ok(env.iters.insert(chunks.into_iter()))
        })
    }

    /// Start iteration on each row, as bytes, of a table identified by `table_id`.
    ///
    /// The iterator is registered in the host environment
    /// under an assigned index which is written to the `out` pointer provided.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    // #[tracing::instrument(skip_all)]
    pub fn iter_start(caller: Caller<'_, Self>, table_id: u32, out: WasmPtr<RowIterIdx>) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::IterStart, out, |caller| {
            let env = caller.data_mut();
            // Retrieve the execution context for the current reducer.
            let ctx = env.reducer_context()?;
            // Collect the iterator chunks.
            let chunks = env.instance_env.iter_chunks(&ctx, table_id.into())?;
            drop(ctx);
            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(env.iters.insert(chunks.into_iter()))
        })
    }

    /// Like [`WasmInstanceEnv::iter_start`], start iteration on each row,
    /// as bytes, of a table identified by `table_id`.
    ///
    /// The rows are filtered through `filter`, which is read from WASM memory
    /// and is encoded in the embedded language defined by `spacetimedb_lib::filter::Expr`.
    ///
    /// The iterator is registered in the host environment
    /// under an assigned index which is written to the `out` pointer provided.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - `(filter, filter_len)` doesn't decode to a filter expression
    /// - `filter + filter_len` overflows a 64-bit integer
    pub fn iter_start_filtered(
        caller: Caller<'_, Self>,
        table_id: u32,
        filter: WasmPtr<u8>,
        filter_len: u32,
        out: WasmPtr<RowIterIdx>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::IterStartFiltered, out, |caller| {
            let (mem, env) = Self::mem_env(caller);
            // Retrieve the execution context for the current reducer.
            let ctx = env.reducer_context()?;

            // Read the slice `(filter, filter_len)`.
            let filter = mem.deref_slice(filter, filter_len)?;

            // Construct the iterator.
            let chunks = env.instance_env.iter_filtered_chunks(&ctx, table_id.into(), filter)?;
            drop(ctx);
            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(env.iters.insert(chunks.into_iter()))
        })
    }

    /// Reads rows from the given iterator registered under `iter`.
    ///
    /// Takes rows from the iterator
    /// and stores them in the memory pointed to by `buffer = buffer_ptr[..buffer_len]`,
    /// encoded in BSATN format.
    ///
    /// The `buffer_len = buffer_len_ptr[..size_of::<usize>()]` stores the capacity of `buffer`.
    /// On success (`0` or `-1` is returned),
    /// `buffer_len` is set to the combined length of the encoded rows.
    /// When `-1` is returned, the iterator has been exhausted
    /// and there are no more rows to read,
    /// leading to the iterator being immediately destroyed.
    /// Note that the host is free to reuse allocations in a pool,
    /// destroying the handle logically does not entail that memory is necessarily reclaimed.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - `buffer_len_ptr` is NULL or `buffer_len` is not in bounds of WASM memory.
    /// - `buffer_ptr` is NULL or `buffer` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NO_SUCH_ITER`, when `iter` is not a valid iterator.
    /// - `BUFFER_TOO_SMALL`, when there are rows left but they cannot fit in `buffer`.
    ///   When this occurs, `buffer_len` is set to the size of the next item in the iterator.
    ///   To make progress, the caller should reallocate the buffer to at least that size and try again.
    // #[tracing::instrument(skip_all)]
    pub fn row_iter_bsatn_advance(
        caller: Caller<'_, Self>,
        iter: u32,
        buffer_ptr: WasmPtr<u8>,
        buffer_len_ptr: WasmPtr<u32>,
    ) -> RtResult<i32> {
        let row_iter_idx = RowIterIdx(iter);
        Self::cvt_custom(caller, AbiCall::RowIterBsatnAdvance, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Retrieve the iterator by `row_iter_idx`, or error.
            let Some(iter) = env.iters.get_mut(row_iter_idx) else {
                return Ok(errno::NO_SUCH_ITER.get().into());
            };

            // Read `buffer_len`, i.e., the capacity of `buffer` pointed to by `buffer_ptr`.
            let buffer_len = u32::read_from(mem, buffer_len_ptr)?;
            let write_buffer_len = |mem, len| u32::try_from(len).unwrap().write_to(mem, buffer_len_ptr);
            // Get a mutable view to the `buffer`.
            let mut buffer = mem.deref_slice_mut(buffer_ptr, buffer_len)?;

            let mut written = 0;
            // Fill the buffer as much as possible.
            while let Some(chunk) = iter.as_slice().first() {
                // TODO(Centril): refactor using `split_at_mut_checked`.
                let Some(buf_chunk) = buffer.get_mut(..chunk.len()) else {
                    // Cannot fit chunk into the buffer,
                    // either because we already filled it too much,
                    // or because it is too small.
                    break;
                };
                buf_chunk.copy_from_slice(chunk);
                written += chunk.len();
                buffer = &mut buffer[chunk.len()..];

                // Advance the iterator, as we used a chunk.
                // TODO(Centril): consider putting these into a pool for reuse
                // by the next `ChunkedWriter::collect_iter`, `span_start`, and `bytes_sink_write`.
                // Although we need to shrink these chunks to fit due to `Box<[u8]>`,
                // in practice, `realloc` will in practice not move the data to a new heap allocation.
                iter.next();
            }

            let ret = match (written, iter.as_slice().first()) {
                // Nothing was written and the iterator is not exhausted.
                (0, Some(chunk)) => {
                    write_buffer_len(mem, chunk.len())?;
                    return Ok(errno::BUFFER_TOO_SMALL.get().into());
                }
                // The iterator is exhausted, destroy it, and tell the caller.
                (_, None) => {
                    env.iters.take(row_iter_idx);
                    -1
                }
                // Something was written, but the iterator is not exhausted.
                (_, Some(_)) => 0,
            };
            write_buffer_len(mem, written)?;
            Ok(ret)
        })
    }

    /// Destroys the iterator registered under `iter`.
    ///
    /// Once `row_iter_bsatn_close` is called on `iter`, the `iter` is invalid.
    /// That is, `row_iter_bsatn_close(iter)` the second time will yield `NO_SUCH_ITER`.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NO_SUCH_ITER`, when `iter` is not a valid iterator.
    // #[tracing::instrument(skip_all)]
    pub fn row_iter_bsatn_close(caller: Caller<'_, Self>, iter: u32) -> RtResult<u32> {
        let row_iter_idx = RowIterIdx(iter);
        Self::cvt_custom(caller, AbiCall::RowIterBsatnClose, |caller| {
            let (_, env) = Self::mem_env(caller);

            // Retrieve the iterator by `row_iter_idx`, or error.
            Ok(match env.iters.take(row_iter_idx) {
                None => errno::NO_SUCH_ITER.get().into(),
                // TODO(Centril): consider putting these into a pool for reuse.
                Some(_) => 0,
            })
        })
    }

    pub fn volatile_nonatomic_schedule_immediate(
        mut caller: Caller<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        args: WasmPtr<u8>,
        args_len: u32,
    ) -> RtResult<()> {
        let (mem, env) = Self::mem_env(&mut caller);
        let name = mem.deref_str(name, name_len)?;
        let args = mem.deref_slice(args, args_len)?;
        env.instance_env.scheduler.volatile_nonatomic_schedule_immediate(
            name.to_owned(),
            crate::host::ReducerArgs::Bsatn(args.to_vec().into()),
        );

        Ok(())
    }

    /// Reads bytes from `source`, registered in the host environment,
    /// and stores them in the memory pointed to by `buffer = buffer_ptr[..buffer_len]`.
    ///
    /// The `buffer_len = buffer_len_ptr[..size_of::<usize>()]` stores the capacity of `buffer`.
    /// On success (`0` or `-1` is returned),
    /// `buffer_len` is set to the number of bytes written to `buffer`.
    /// When `-1` is returned, the resource has been exhausted
    /// and there are no more bytes to read,
    /// leading to the resource being immediately destroyed.
    /// Note that the host is free to reuse allocations in a pool,
    /// destroying the handle logically does not entail that memory is necessarily reclaimed.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - `buffer_len_ptr` is NULL or `buffer_len` is not in bounds of WASM memory.
    /// - `buffer_ptr` is NULL or `buffer` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NO_SUCH_BYTES`, when `source` is not a valid bytes source.
    ///
    /// # Example
    ///
    /// The typical use case for this ABI is in `__call_reducer__`,
    /// to read and deserialize the `args`.
    /// An example definition, dealing with `args` might be:
    /// ```rust,ignore
    /// /// #[no_mangle]
    /// extern "C" fn __call_reducer__(..., args: BytesSource, ...) -> i16 {
    ///     // ...
    ///
    ///     let mut buf = Vec::<u8>::with_capacity(1024);
    ///     loop {
    ///         // Write into the spare capacity of the buffer.
    ///         let buf_ptr = buf.spare_capacity_mut();
    ///         let spare_len = buf_ptr.len();
    ///         let mut buf_len = buf_ptr.len();
    ///         let buf_ptr = buf_ptr.as_mut_ptr().cast();
    ///         let ret = unsafe { bytes_source_read(args, buf_ptr, &mut buf_len) };
    ///         // SAFETY: `bytes_source_read` just appended `spare_len` bytes to `buf`.
    ///         unsafe { buf.set_len(buf.len() + spare_len) };
    ///         match ret {
    ///             // Host side source exhausted, we're done.
    ///             -1 => break,
    ///             // Wrote the entire spare capacity.
    ///             // Need to reserve more space in the buffer.
    ///             0 if spare_len == buf_len => buf.reserve(1024),
    ///             // Host didn't write as much as possible.
    ///             // Try to read some more.
    ///             // The host will likely not trigger this branch,
    ///             // but a module should be prepared for it.
    ///             0 => {}
    ///             _ => unreachable!(),
    ///         }
    ///     }
    ///
    ///     // ...
    /// }
    /// ```
    pub fn bytes_source_read(
        caller: Caller<'_, Self>,
        source: u32,
        buffer_ptr: WasmPtr<u8>,
        buffer_len_ptr: WasmPtr<u32>,
    ) -> RtResult<i32> {
        Self::cvt_custom(caller, AbiCall::BytesSourceRead, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Retrieve the reducer args if available and requested, or error.
            let Some((reducer_args, cursor)) = env
                .call_reducer_args
                .as_mut()
                .filter(|_| source == CALL_REDUCER_ARGS_SOURCE)
            else {
                return Ok(errno::NO_SUCH_BYTES.get().into());
            };

            // Read `buffer_len`, i.e., the capacity of `buffer` pointed to by `buffer_ptr`.
            let buffer_len = u32::read_from(mem, buffer_len_ptr)?;
            // Get a mutable view to the `buffer`.
            let buffer = mem.deref_slice_mut(buffer_ptr, buffer_len)?;
            let buffer_len = buffer_len as usize;

            // Derive the portion that we can read and what remains,
            // based on what is left to read and the capacity.
            let left_to_read = &reducer_args[*cursor..];
            let can_read_len = buffer_len.min(left_to_read.len());
            let (can_read, remainder) = left_to_read.split_at(can_read_len);
            // Copy to the `buffer` and write written bytes count to `buffer_len`.
            buffer[..can_read_len].copy_from_slice(can_read);
            (can_read_len as u32).write_to(mem, buffer_len_ptr)?;

            // Destroy the source if exhausted, or advance `cursor`.
            if remainder.is_empty() {
                env.call_reducer_args = None;
                Ok(-1i32)
            } else {
                *cursor += can_read_len;
                Ok(0)
            }
        })
    }

    /// Writes up to `buffer_len` bytes from `buffer = buffer_ptr[..buffer_len]`,
    /// to the `sink`, registered in the host environment.
    ///
    /// The `buffer_len = buffer_len_ptr[..size_of::<usize>()]` stores the capacity of `buffer`.
    /// On success (`0` is returned),
    /// `buffer_len` is set to the number of bytes written to `sink`.
    ///
    /// # Traps
    ///
    /// - `buffer_len_ptr` is NULL or `buffer_len` is not in bounds of WASM memory.
    /// - `buffer_ptr` is NULL or `buffer` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NO_SUCH_BYTES`, when `sink` is not a valid bytes sink.
    /// - `NO_SPACE`, when there is no room for more bytes in `sink`.
    ///    (Doesn't currently happen.)
    pub fn bytes_sink_write(
        caller: Caller<'_, Self>,
        sink: u32,
        buffer_ptr: WasmPtr<u8>,
        buffer_len_ptr: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_custom(caller, AbiCall::BytesSinkWrite, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Retrieve the reducer args if available and requested, or error.
            let Some(sink) = env.standard_bytes_sink.as_mut().filter(|_| sink == STANDARD_BYTES_SINK) else {
                return Ok(errno::NO_SUCH_BYTES.get().into());
            };

            // Read `buffer_len`, i.e., the capacity of `buffer` pointed to by `buffer_ptr`.
            let buffer_len = u32::read_from(mem, buffer_len_ptr)?;
            // Write `buffer` to `sink`.
            let buffer = mem.deref_slice(buffer_ptr, buffer_len)?;
            sink.extend(buffer);

            Ok(0)
        })
    }

    pub fn span_start(mut caller: Caller<'_, Self>, name: WasmPtr<u8>, name_len: u32) -> RtResult<u32> {
        let (mem, env) = Self::mem_env(&mut caller);
        let name = mem.deref_slice(name, name_len)?.to_vec();
        Ok(env.timing_spans.insert(TimingSpan::new(name)).0)
    }

    pub fn span_end(mut caller: Caller<'_, Self>, span_id: u32) -> RtResult<()> {
        let span = caller
            .data_mut()
            .timing_spans
            .take(TimingSpanIdx(span_id))
            .context("no such timing span")?;

        let elapsed = span.start.elapsed();

        let name = String::from_utf8_lossy(&span.name);
        let message = format!("Timing span {:?}: {:?}", name, elapsed);

        let record = Record {
            ts: chrono::Utc::now(),
            target: None,
            filename: None,
            line_number: None,
            message: &message,
        };
        caller
            .data()
            .instance_env
            .console_log(crate::database_logger::LogLevel::Info, &record, &caller.as_context());
        Ok(())
    }
}

impl<T> BacktraceProvider for wasmtime::StoreContext<'_, T> {
    fn capture(&self) -> Box<dyn ModuleBacktrace> {
        Box::new(wasmtime::WasmBacktrace::capture(self))
    }
}

impl ModuleBacktrace for wasmtime::WasmBacktrace {
    fn frames(&self) -> Vec<BacktraceFrame<'_>> {
        self.frames()
            .iter()
            .map(|f| BacktraceFrame {
                module_name: None,
                func_name: f.func_name(),
            })
            .collect()
    }
}
