#![allow(clippy::too_many_arguments)]

use std::time::Instant;

use crate::database_logger::{BacktraceFrame, BacktraceProvider, ModuleBacktrace, Record};
use crate::db::db_metrics::DB_METRICS;
use crate::execution_context::ExecutionContext;
use crate::host::scheduler::{ScheduleError, ScheduledReducerId};
use crate::host::timestamp::Timestamp;
use crate::host::wasm_common::instrumentation;
use crate::host::wasm_common::module_host_actor::ExecutionTimings;
use crate::host::wasm_common::{
    err_to_errno, instrumentation::CallTimes, AbiRuntimeError, BufferIdx, BufferIterIdx, BufferIters, Buffers,
    TimingSpan, TimingSpanIdx, TimingSpanSet,
};
use crate::host::AbiCall;
use wasmer::{FunctionEnvMut, MemoryAccessError, RuntimeError, ValueType, WasmPtr};

use crate::host::instance_env::InstanceEnv;

use super::{Mem, WasmError};

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

    /// The slab of `Buffers` created for this instance.
    buffers: Buffers,

    /// The slab of `BufferIters` created for this instance.
    iters: BufferIters,

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

type WasmResult<T> = Result<T, WasmError>;
type RtResult<T> = Result<T, RuntimeError>;

fn mem_err(err: MemoryAccessError) -> RuntimeError {
    match err {
        MemoryAccessError::HeapOutOfBounds | MemoryAccessError::Overflow => {
            RuntimeError::from(wasmer_vm::Trap::lib(wasmer_vm::TrapCode::HeapAccessOutOfBounds))
        }
        _ => RuntimeError::user(err.into()),
    }
}

/// Wraps an `InstanceEnv` with the magic necessary to push
/// and pull bytes from webassembly memory.
impl WasmInstanceEnv {
    /// Create a new `WasmEnstanceEnv` from the given `InstanceEnv`.
    pub fn new(instance_env: InstanceEnv) -> Self {
        let reducer_start = Instant::now();
        Self {
            instance_env,
            mem: None,
            buffers: Default::default(),
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
    pub fn mem(&self) -> Mem {
        self.mem.clone().expect("Initialized memory")
    }

    /// Return a reference to the `InstanceEnv`,
    /// which is responsible for DB instance and associated state.
    pub fn instance_env(&self) -> &InstanceEnv {
        &self.instance_env
    }

    /// Take ownership of a particular `Buffer` from this instance.
    pub fn take_buffer(&mut self, idx: BufferIdx) -> Option<bytes::Bytes> {
        self.buffers.take(idx)
    }

    /// Take ownership of the given `data` and give back a `BufferIdx`
    /// as a handle to that data.
    pub fn insert_buffer(&mut self, data: bytes::Bytes) -> BufferIdx {
        self.buffers.insert(data)
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is beginning.
    pub fn start_reducer(&mut self, name: &str) {
        self.reducer_start = Instant::now();
        self.reducer_name = name.to_owned();
    }

    /// Signal to this `WasmInstanceEnv` that a reducer call is over.
    /// This resets all of the state associated to a single reducer call,
    /// and returns instrumentation records.
    pub fn finish_reducer(&mut self) -> ExecutionTimings {
        // For the moment, we only explicitly clear the set of buffers and the
        // "syscall" times.
        // TODO: should we be clearing `iters` and/or `timing_spans`?
        self.buffers.clear();

        let total_duration = self.reducer_start.elapsed();

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        }
    }

    /// Returns an execution context for a reducer call.
    fn reducer_context(&self) -> ExecutionContext {
        ExecutionContext::reducer(self.instance_env().dbic.address, self.reducer_name.as_str())
    }

    /// Call the function `f` with the name `func`.
    /// The function `f` is provided with the callers environment and the host's memory.
    ///
    /// One of `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    ///
    /// Some database errors are logged but are otherwise regarded as `Ok(_)`.
    /// See `err_to_errno` for a list.
    fn cvt(
        mut caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        call: AbiCall,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> WasmResult<()>,
    ) -> RtResult<u16> {
        let span_start = span::CallSpanStart::new(call);

        // Call `f` with the caller and a handle to the memory.
        let mem = caller.data().mem();
        let result = f(caller.as_mut(), &mem);

        // Track the span of this call.
        let span = span_start.end();
        span::record_span(&mut caller.data_mut().call_times, span);

        // Bail if there were no errors.
        let Err(err) = result else {
            return Ok(0);
        };

        // Handle any errors.
        Err(match err {
            WasmError::Db(err) => match err_to_errno(&err) {
                Some(errno) => {
                    log::debug!("abi call to {func} returned a normal error: {err:#}");
                    return Ok(errno);
                }
                None => RuntimeError::user(Box::new(AbiRuntimeError { func, err })),
            },
            WasmError::Mem(err) => mem_err(err),
            WasmError::Wasm(err) => err,
        })
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
    fn cvt_ret<T: ValueType>(
        caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        call: AbiCall,
        out: WasmPtr<T>,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> WasmResult<T>,
    ) -> RtResult<u16> {
        Self::cvt(caller, func, call, |mut caller, mem| {
            f(caller.as_mut(), mem).and_then(|ret| out.write(&mem.view(&caller), ret).map_err(Into::into))
        })
    }

    /// Call the function `f`.
    ///
    /// This is the version of `cvt` or `cvt_ret` for functions with no return value.
    /// One of `cvt`, `cvt_ret`, or `cvt_noret` should be used in the implementation of any
    /// host call, to provide consistent error handling and instrumentation.
    fn cvt_noret(mut caller: FunctionEnvMut<'_, Self>, call: AbiCall, f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem)) {
        let span_start = span::CallSpanStart::new(call);

        // Call `f` with the caller and a handle to the memory.
        let mem = caller.data().mem();
        f(caller.as_mut(), &mem);

        let span = span_start.end();
        span::record_span(&mut caller.data_mut().call_times, span);
    }

    /// Reads a string from WASM memory starting at `ptr` and lasting `len` bytes.
    ///
    /// Returns an error if:
    /// - `ptr + len` overflows a 64-bit address.
    /// - the string was not valid UTF-8
    fn read_string(caller: &FunctionEnvMut<'_, Self>, mem: &Mem, ptr: WasmPtr<u8>, len: u32) -> RtResult<String> {
        let bytes = mem.read_bytes(&caller, ptr, len)?;
        String::from_utf8(bytes).map_err(|_| RuntimeError::new("name must be utf8"))
    }

    /// Schedules a reducer to be called asynchronously at `time`.
    ///
    /// The reducer is named as the valid UTF-8 slice `(name, name_len)`,
    /// and is passed the slice `(args, args_len)` as its argument.
    ///
    /// A generated schedule id is assigned to the reducer.
    /// This id is written to the pointer `out`.
    ///
    /// Returns an error if
    /// - the `time` delay exceeds `64^6 - 1` milliseconds from now
    /// - `name` does not point to valid UTF-8
    /// - `name + name_len` or `args + args_len` overflow a 64-bit integer
    /// - writing to `out` overflows a 64-bit integer
    #[tracing::instrument(skip_all)]
    pub fn schedule_reducer(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        args: WasmPtr<u8>,
        args_len: u32,
        time: u64,
        out: WasmPtr<u64>,
    ) -> RtResult<()> {
        Self::cvt_ret(
            caller,
            "schedule_reducer",
            AbiCall::ScheduleReducer,
            out,
            |caller, mem| {
                // Read the index name as a string from `(name, name_len)`.
                let name = Self::read_string(&caller, mem, name, name_len)?;

                // Read the reducer's arguments as a byte slice.
                let args = mem.read_bytes(&caller, args, args_len)?;

                // Schedule it!
                // TODO: Be more strict re. types by avoiding newtype unwrapping here? (impl ValueType?)
                // Noa: This would be nice but I think the eventual goal/desire is to switch to wasmtime,
                //      which doesn't allow user types to impl ValueType.
                //      Probably the correct API choice, but makes things a bit less ergonomic sometimes.
                let ScheduledReducerId(id) = caller
                    .data()
                    .instance_env
                    .schedule(name, args, Timestamp(time))
                    .map_err(|e| match e {
                        ScheduleError::DelayTooLong(_) => RuntimeError::new("requested delay is too long"),
                        ScheduleError::IdTransactionError(_) => {
                            RuntimeError::new("transaction to acquire ScheduleReducerId failed")
                        }
                    })?;
                Ok(id)
            },
        )
        .map(|_| ())
    }

    /// Unschedule a reducer using the same `id` generated as when it was scheduled.
    ///
    /// This assumes that the reducer hasn't already been executed.
    #[tracing::instrument(skip_all)]
    pub fn cancel_reducer(caller: FunctionEnvMut<'_, Self>, id: u64) {
        Self::cvt_noret(caller, AbiCall::CancelReducer, |caller, _mem| {
            caller.data().instance_env.cancel_reducer(ScheduledReducerId(id))
        })
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
        caller: FunctionEnvMut<'_, Self>,
        level: u8,
        target: WasmPtr<u8>,
        target_len: u32,
        filename: WasmPtr<u8>,
        filename_len: u32,
        line_number: u32,
        message: WasmPtr<u8>,
        message_len: u32,
    ) {
        Self::cvt_noret(caller, AbiCall::ConsoleLog, |caller, mem| {
            // Reads a string lossily from the slice `(ptr, len)` in WASM memory.
            let read_str = |ptr, len| {
                mem.read_bytes(&caller, ptr, len)
                    .map(crate::util::string_from_utf8_lossy_owned)
            };

            // Reads as string optionally, unless `ptr.is_null()`.
            let read_opt_str = |ptr: WasmPtr<_>, len| (!ptr.is_null()).then(|| read_str(ptr, len)).transpose();

            let _ = (|| -> Result<_, MemoryAccessError> {
                // Read the `target`, `filename`, and `message` strings from WASM memory.
                let target = read_opt_str(target, target_len)?;
                let filename = read_opt_str(filename, filename_len)?;
                let message = read_str(message, message_len)?;

                // The line number cannot be `u32::MAX` as this represents `Option::None`.
                let line_number = (line_number != u32::MAX).then_some(line_number);

                let record = Record {
                    target: target.as_deref(),
                    filename: filename.as_deref(),
                    line_number,
                    message: &message,
                };

                // Write the log record to the `DatabaseLogger` in the database instance context (dbic).
                caller
                    .data()
                    .instance_env
                    .console_log(level.into(), &record, &WasmerBacktraceProvider);
                Ok(())
            })();
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
    pub fn insert(caller: FunctionEnvMut<'_, Self>, table_id: u32, row: WasmPtr<u8>, row_len: u32) -> RtResult<u16> {
        let ctx = caller.data().reducer_context();
        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default().to_owned();
        let syscall = &AbiCall::Insert;

        // TODO: Instead of writing this metric on every insert call,
        // we should aggregate and write at the end of the transaction.
        let _guard = DB_METRICS
            .wasm_abi_call_duration_sec
            .with_label_values(txn_type, db, reducer, syscall)
            .start_timer();

        Self::cvt(caller, "insert", AbiCall::Insert, |caller, mem| {
            // Read the row from WASM memory into a buffer.
            let mut row_buffer = mem.read_bytes(&caller, row, row_len)?;

            // Insert the row into the DB. We get back the decoded version.
            // Then re-encode and write that back into WASM memory at `row`.
            // We're doing this because of autoinc.
            let ctx = caller.data().reducer_context();
            let new_row = caller.data().instance_env.insert(&ctx, table_id.into(), &row_buffer)?;
            row_buffer.clear();
            new_row.encode(&mut row_buffer);
            assert_eq!(
                row_buffer.len(),
                row_len as usize,
                "autoinc'd row is different encoded size from original row"
            );
            mem.set_bytes(&caller, row, row_len, &row_buffer)?;
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
    #[tracing::instrument(skip_all)]
    pub fn delete_by_col_eq(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        value: WasmPtr<u8>,
        value_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        let ctx = caller.data().reducer_context();
        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default().to_owned();
        let syscall = &AbiCall::DeleteByColEq;

        let _guard = DB_METRICS
            .wasm_abi_call_duration_sec
            .with_label_values(txn_type, db, reducer, syscall)
            .start_timer();

        Self::cvt_ret(
            caller,
            "delete_by_col_eq",
            AbiCall::DeleteByColEq,
            out,
            |caller, mem| {
                let ctx = caller.data().reducer_context();
                let value = mem.read_bytes(&caller, value, value_len)?;
                let count =
                    caller
                        .data()
                        .instance_env
                        .delete_by_col_eq(&ctx, table_id.into(), col_id.into(), &value)?;
                Ok(count)
            },
        )
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
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        relation: WasmPtr<u8>,
        relation_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "delete_by_rel", AbiCall::DeleteByRel, out, |caller, mem| {
            let relation = mem.read_bytes(&caller, relation, relation_len)?;
            Ok(caller.data().instance_env.delete_by_rel(table_id.into(), &relation)?)
        })
    }

    /// Queries the `table_id` associated with the given (table) `name`
    /// where `name` points to a UTF-8 slice in WASM memory of `name_len` bytes.
    ///
    /// The table id is written into the `out` pointer.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - the slice `(name, name_len)` is not valid UTF-8
    /// - `name + name_len` overflows a 64-bit address.
    /// - writing to `out` overflows a 32-bit integer
    #[tracing::instrument(skip_all)]
    pub fn get_table_id(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "get_table_id", AbiCall::GetTableId, out, |caller, mem| {
            // Read the table name from WASM memory.
            let name = Self::read_string(&caller, mem, name, name_len)?;

            // Query the table id.
            Ok(caller.data().instance_env.get_table_id(name)?.into())
        })
    }

    /// Creates an index with the name `index_name` and type `index_type`,
    /// on a product of the given columns in `col_ids`
    /// in the table identified by `table_id`.
    ///
    /// Here `index_name` points to a UTF-8 slice in WASM memory
    /// and `col_ids` points to a byte slice in WASM memory with each element being a column.
    ///
    /// Currently only single-column-indices are supported
    /// and they may only be of the btree index type.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    /// - the slice `(index_name, index_name_len)` is not valid UTF-8
    /// - `index_name + index_name_len` or `col_ids + col_len` overflow a 64-bit integer
    /// - `index_type > 1`
    ///
    /// Panics if `index_type == 1` or `col_ids.len() != 1`.
    #[tracing::instrument(skip_all)]
    pub fn create_index(
        caller: FunctionEnvMut<'_, Self>,
        index_name: WasmPtr<u8>,
        index_name_len: u32,
        table_id: u32,
        index_type: u8,
        col_ids: WasmPtr<u8>,
        col_len: u32,
    ) -> RtResult<u16> {
        Self::cvt(caller, "create_index", AbiCall::CreateIndex, |caller, mem| {
            // Read the index name from WASM memory.
            let index_name = Self::read_string(&caller, mem, index_name, index_name_len)?;

            // Read the column ids on which to create an index from WASM memory.
            // This may be one column or an index on several columns.
            let cols = mem.read_bytes(&caller, col_ids, col_len)?;

            caller
                .data()
                .instance_env
                .create_index(index_name, table_id.into(), index_type, cols)?;
            Ok(())
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
    #[tracing::instrument(skip_all)]
    pub fn iter_by_col_eq(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        val: WasmPtr<u8>,
        val_len: u32,
        out: WasmPtr<BufferIdx>,
    ) -> RtResult<u16> {
        let ctx = caller.data().reducer_context();
        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default().to_owned();
        let syscall = &AbiCall::IterByColEq;

        let _guard = DB_METRICS
            .wasm_abi_call_duration_sec
            .with_label_values(txn_type, db, reducer, syscall)
            .start_timer();

        Self::cvt_ret(
            caller,
            "iter_by_col_eq",
            AbiCall::IterByColEq,
            out,
            |mut caller, mem| {
                // Read the test value from WASM memory.
                let value = mem.read_bytes(&caller, val, val_len)?;

                // Retrieve the execution context for the current reducer.
                let ctx = caller.data().reducer_context();

                // Find the relevant rows.
                let data = caller
                    .data()
                    .instance_env
                    .iter_by_col_eq(&ctx, table_id.into(), col_id.into(), &value)?;

                // Insert the encoded + concatenated rows into a new buffer and return its id.
                Ok(caller.data_mut().buffers.insert(data.into()))
            },
        )
    }

    /// Start iteration on each row, as bytes, of a table identified by `table_id`.
    ///
    /// The iterator is registered in the host environment
    /// under an assigned index which is written to the `out` pointer provided.
    ///
    /// Returns an error if
    /// - a table with the provided `table_id` doesn't exist
    // #[tracing::instrument(skip_all)]
    pub fn iter_start(caller: FunctionEnvMut<'_, Self>, table_id: u32, out: WasmPtr<BufferIterIdx>) -> RtResult<u16> {
        let ctx = caller.data().reducer_context();
        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default().to_owned();
        let syscall = &AbiCall::IterStart;

        let _guard = DB_METRICS
            .wasm_abi_call_duration_sec
            .with_label_values(txn_type, db, reducer, syscall)
            .start_timer();

        Self::cvt_ret(caller, "iter_start", AbiCall::IterStart, out, |mut caller, _mem| {
            // Retrieve the execution context for the current reducer.
            let ctx = caller.data().reducer_context();

            // Collect the iterator chunks.
            let chunks = caller.data().instance_env.iter_chunks(&ctx, table_id.into())?;

            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(caller.data_mut().iters.insert(chunks.into_iter()))
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
    // #[tracing::instrument(skip_all)]
    pub fn iter_start_filtered(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        filter: WasmPtr<u8>,
        filter_len: u32,
        out: WasmPtr<BufferIterIdx>,
    ) -> RtResult<u16> {
        let ctx = caller.data().reducer_context();
        let txn_type = &ctx.txn_type();
        let db = &ctx.database();
        let reducer = &ctx.reducer_name().unwrap_or_default().to_owned();
        let syscall = &AbiCall::IterStartFiltered;

        let _guard = DB_METRICS
            .wasm_abi_call_duration_sec
            .with_label_values(txn_type, db, reducer, syscall)
            .start_timer();

        Self::cvt_ret(
            caller,
            "iter_start_filtered",
            AbiCall::IterStartFiltered,
            out,
            |mut caller, _mem| {
                // Read the slice `(filter, filter_len)`.
                let filter = caller.data().mem().read_bytes(&caller, filter, filter_len)?;

                // Retrieve the execution context for the current reducer.
                let ctx = caller.data().reducer_context();

                // Construct the iterator.
                let chunks = caller
                    .data()
                    .instance_env
                    .iter_filtered_chunks(&ctx, table_id.into(), &filter)?;

                // Register the iterator and get back the index to write to `out`.
                // Calls to the iterator are done through dynamic dispatch.
                Ok(caller.data_mut().iters.insert(chunks.into_iter()))
            },
        )
    }

    /// Advances the registered iterator with the index given by `iter_key`.
    ///
    /// On success, the next element (the row as bytes) is written to a buffer.
    /// The buffer's index is returned and written to the `out` pointer.
    /// If there are no elements left, an invalid buffer index is written to `out`.
    /// On failure however, the error is returned.
    ///
    /// Returns an error if
    /// - `iter` does not identify a registered `BufferIter`
    /// - writing to `out` would overflow a 32-bit integer
    /// - advancing the iterator resulted in an error
    // #[tracing::instrument(skip_all)]
    pub fn iter_next(caller: FunctionEnvMut<'_, Self>, iter_key: u32, out: WasmPtr<BufferIdx>) -> RtResult<u16> {
        Self::cvt_ret(caller, "iter_next", AbiCall::IterNext, out, |mut caller, _mem| {
            let data_mut = caller.data_mut();

            // Retrieve the iterator by `iter_key`.
            let iter = data_mut
                .iters
                .get_mut(BufferIterIdx(iter_key))
                .ok_or_else(|| RuntimeError::new("no such iterator"))?;

            // Advance the iterator.
            Ok(iter
                .next()
                .map_or(BufferIdx::INVALID, |buf| data_mut.buffers.insert(buf.into())))
        })
    }

    /// Drops the entire registered iterator with the index given by `iter_key`.
    /// The iterator is effectively de-registered.
    ///
    /// Returns an error if the iterator does not exist.
    // #[tracing::instrument(skip_all)]
    pub fn iter_drop(caller: FunctionEnvMut<'_, Self>, iter_key: u32) -> RtResult<u16> {
        Self::cvt(caller, "iter_drop", AbiCall::IterDrop, |mut caller, _mem| {
            caller
                .data_mut()
                .iters
                .take(BufferIterIdx(iter_key))
                .ok_or_else(|| RuntimeError::new("no such iterator").into())
                .map(drop)
        })
    }

    /// Returns the length (number of bytes) of buffer `bufh` without
    /// transferring ownership of the data into the function.
    ///
    /// The `bufh` must have previously been allocating using `_buffer_alloc`.
    ///
    /// Returns an error if the buffer does not exist.
    // #[tracing::instrument(skip_all)]
    pub fn buffer_len(caller: FunctionEnvMut<'_, Self>, buffer: u32) -> RtResult<u32> {
        caller
            .data()
            .buffers
            .get(BufferIdx(buffer))
            .map(|b| b.len() as u32)
            .ok_or_else(|| RuntimeError::new("no such buffer"))
    }

    /// Consumes the `buffer`,
    /// moving its contents to the slice `(dst, dst_len)`.
    ///
    /// Returns an error if
    /// - the buffer does not exist
    /// - `dst + dst_len` overflows a 64-bit integer
    // #[tracing::instrument(skip_all)]
    pub fn buffer_consume(
        mut caller: FunctionEnvMut<'_, Self>,
        buffer: u32,
        dst: WasmPtr<u8>,
        dst_len: u32,
    ) -> RtResult<()> {
        let buf = caller
            .data_mut()
            .buffers
            .take(BufferIdx(buffer))
            .ok_or_else(|| RuntimeError::new("no such buffer"))?;
        dst.slice(&caller.data().mem().view(&caller), dst_len)
            .and_then(|slice| slice.write_slice(&buf))
            .map_err(mem_err)
    }

    /// Creates a buffer of size `data_len` in the host environment.
    ///
    /// The contents of the byte slice pointed to by `data`
    /// and lasting `data_len` bytes
    /// is written into the newly initialized buffer.
    ///
    /// The buffer is registered in the host environment and is indexed by the returned `u32`.
    ///
    /// Returns an error if `data + data_len` overflows a 64-bit integer.
    // #[tracing::instrument(skip_all)]
    pub fn buffer_alloc(mut caller: FunctionEnvMut<'_, Self>, data: WasmPtr<u8>, data_len: u32) -> RtResult<u32> {
        let buf = caller
            .data()
            .mem()
            .read_bytes(&caller, data, data_len)
            .map_err(mem_err)?;
        Ok(caller.data_mut().buffers.insert(buf.into()).0)
    }

    pub fn span_start(mut caller: FunctionEnvMut<'_, Self>, name: WasmPtr<u8>, name_len: u32) -> RtResult<u32> {
        let name = caller
            .data()
            .mem()
            .read_bytes(&caller, name, name_len)
            .map_err(mem_err)?;
        Ok(caller.data_mut().timing_spans.insert(TimingSpan::new(name)).0)
    }

    pub fn span_end(mut caller: FunctionEnvMut<'_, Self>, span_id: u32) -> RtResult<()> {
        let span = caller
            .data_mut()
            .timing_spans
            .take(TimingSpanIdx(span_id))
            .ok_or_else(|| RuntimeError::new("no such timing span"))?;

        let elapsed = span.start.elapsed();

        let name = String::from_utf8_lossy(&span.name);
        let message = format!("Timing span {:?}: {:?}", name, elapsed);

        let record = Record {
            target: None,
            filename: None,
            line_number: None,
            message: &message,
        };
        caller.data().instance_env.console_log(
            crate::database_logger::LogLevel::Info,
            &record,
            &WasmerBacktraceProvider,
        );
        Ok(())
    }
}

struct WasmerBacktraceProvider;
impl BacktraceProvider for WasmerBacktraceProvider {
    fn capture(&self) -> Box<dyn ModuleBacktrace> {
        Box::new(RuntimeError::new(""))
    }
}

impl ModuleBacktrace for RuntimeError {
    fn frames(&self) -> Vec<BacktraceFrame<'_>> {
        self.trace()
            .iter()
            .map(|f| {
                let module = f.module_name();
                BacktraceFrame {
                    module_name: (module != "<module>").then_some(module),
                    func_name: f.function_name(),
                }
            })
            .collect()
    }
}
