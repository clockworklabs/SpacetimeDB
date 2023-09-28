#![allow(clippy::too_many_arguments)]

use crate::database_logger::{BacktraceFrame, BacktraceProvider, ModuleBacktrace, Record};
use crate::db::datastore::traits::ColId;
use crate::host::scheduler::{ScheduleError, ScheduledReducerId};
use crate::host::timestamp::Timestamp;
use crate::host::wasm_common::{err_to_errno, AbiRuntimeError, BufferIdx, BufferIterIdx, BufferIters, Buffers};
use bytes::Bytes;
use itertools::Itertools;
use nonempty::NonEmpty;
use spacetimedb_sats::slim_slice::LenTooLong;
use spacetimedb_sats::SatsString;
use wasmer::{FunctionEnvMut, MemoryAccessError, RuntimeError, ValueType, WasmPtr};

use crate::host::instance_env::InstanceEnv;

use super::{Mem, WasmError};

pub(super) struct WasmInstanceEnv {
    pub instance_env: InstanceEnv,
    pub mem: Option<Mem>,
    pub buffers: Buffers,
    pub iters: BufferIters,
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
    /// Returns a reference to the memory, assumed to be initialized.
    pub fn mem(&self) -> Mem {
        self.mem.clone().expect("Initialized memory")
    }

    /// Call the function `f` with the name `func`.
    /// The function `f` is provided with the callers environment and the host's memory.
    ///
    /// Some database errors are logged but are otherwise regarded as `Ok(_)`.
    /// See `err_to_errno` for a list.
    fn cvt(
        mut caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> WasmResult<()>,
    ) -> RtResult<u16> {
        // Call `f` with the caller and a handle to the memory.
        // Bail if there were no errors.
        let mem = caller.data().mem();
        let Err(err) = f(caller.as_mut(), &mem) else {
            return Ok(0);
        };

        // Handle any errors.
        Err(match err {
            WasmError::Db(err) => match err_to_errno(&err) {
                Some(errno) => {
                    log::info!("abi call to {func} returned a normal error: {err:#}");
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
    /// This method should be used as opposed to a manual implementation,
    /// as it helps with upholding the safety invariants of [`bindings_sys::call`].
    fn cvt_ret<T: ValueType>(
        caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        out: WasmPtr<T>,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> WasmResult<T>,
    ) -> RtResult<u16> {
        Self::cvt(caller, func, |mut caller, mem| {
            f(caller.as_mut(), mem).and_then(|ret| out.write(&mem.view(&caller), ret).map_err(Into::into))
        })
    }

    /// Reads a string from WASM memory starting at `ptr` and lasting `len` bytes.
    ///
    /// Returns an error if there were memory access issues
    /// or if the string was not valid UTF-8 or if `string.len() > u32::MAX`.
    fn read_sats_string(
        caller: &FunctionEnvMut<'_, Self>,
        mem: &Mem,
        ptr: WasmPtr<u8>,
        len: u32,
    ) -> RtResult<SatsString> {
        let bytes = mem.read_bytes(&caller, ptr, len)?;
        let string = String::from_utf8(bytes).map_err(|_| RuntimeError::new("name must be utf8"))?;
        string
            .try_into()
            .map_err(|e: LenTooLong<_>| RuntimeError::new(format!("string too long: `{}`", e.len)))
    }

    /// Schedule the reducer `(name, name_len)` to be executed asynchronously,
    /// passing it `(args, args_len)`, at the given `time`.
    ///
    /// This can be thought of as `setTimeout` in JS.
    /// Note that `time = 0` can still mean that `cancel_reducer` has an effect due to threading.
    ///
    /// The scheduled reducer is assigned a generated `id`, which is written to the pointer `out`.
    /// Note that `name` must point to valid UTF-8 or a `RuntimeError` will occur.
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
        Self::cvt_ret(caller, "schedule_reducer", out, |caller, mem| {
            // Read the index name as a string from `(name, name_len)`.
            let name = Self::read_sats_string(&caller, mem, name, name_len)?;

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
        })
        .map(|_| ())
    }

    /// Cancel a reducer that was scheduled with `id`.
    ///
    /// This assumes that the reducer hasn't already been executed.
    #[tracing::instrument(skip_all)]
    pub fn cancel_reducer(caller: FunctionEnvMut<'_, Self>, id: u64) {
        caller.data().instance_env.cancel_reducer(ScheduledReducerId(id))
    }

    /// Log at `level` a `message` occuring in `filename:line_number` with `target`.
    ///
    /// These various pointers are interpreted lossily as UTF-8 strings with a corresponding `_len`.
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
        let mem = caller.data().mem();

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
    }

    /// Insert a row, into the table identified by `table_id`,
    /// where the row is read from the byte slice `row_ptr` in WASM memory,
    /// lasting `row_len` bytes.
    #[tracing::instrument(skip_all)]
    pub fn insert(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        row_ptr: WasmPtr<u8>,
        row_len: u32,
    ) -> RtResult<u16> {
        Self::cvt(caller, "insert", |caller, mem| {
            // Read the row from WASM memory into a buffer.
            let mut row_buffer = mem.read_bytes(&caller, row_ptr, row_len)?;

            // Insert the row into the DB. We get back the decoded version.
            // Then re-encode and write that back into WASM memory at `row_ptr`.
            // We're doing this because of autoinc.
            let new_row = caller.data().instance_env.insert(table_id, &row_buffer)?;
            row_buffer.clear();
            new_row.encode(&mut row_buffer);
            assert_eq!(
                row_buffer.len(),
                row_len as usize,
                "autoinc'd row is different encoded size from original row"
            );
            mem.set_bytes(&caller, row_ptr, row_len, &row_buffer)?;
            Ok(())
        })
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the column identified by `cols` matches the byte string,
    /// in WASM memory, pointed to at by `value`.
    ///
    /// Matching is defined by decoding of `value` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// The number of rows deleted is written to the WASM pointer `out`.
    ///
    /// Returns an error if no columns were deleted or if the column wasn't found.
    #[tracing::instrument(skip_all)]
    pub fn delete_by_col_eq(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        value: WasmPtr<u8>,
        value_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "delete_by_col_eq", out, |caller, mem| {
            let value = mem.read_bytes(&caller, value, value_len)?;
            Ok(caller.data().instance_env.delete_by_col_eq(table_id, col_id, &value)?)
        })
    }

    /*
    /// Deletes the primary key pointed to at by `pk` in the table identified by `table_id`.
    #[tracing::instrument(skip_all)]
    pub fn delete_pk(caller: FunctionEnvMut<'_, Self>, table_id: u32, pk: WasmPtr<u8>, pk_len: u32) -> RtResult<u16> {
        Self::cvt(caller, "delete_pk", |caller, mem| {
            // Read the primary key from WASM memory.
            let pk = mem.read_bytes(&caller, pk, pk_len)?;

            // Delete it.
            caller.data().instance_env.delete_pk(table_id, &pk)?;
            Ok(())
        })
    }

    /// Deletes the row pointed to at by `row` in the table identified by `table_id`.
    #[tracing::instrument(skip_all)]
    pub fn delete_value(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        row: WasmPtr<u8>,
        row_len: u32,
    ) -> RtResult<u16> {
        Self::cvt(caller, "delete_value", |caller, mem| {
            // Read the row from WASM memory.
            let row = mem.read_bytes(&caller, row, row_len)?;
            caller.data().instance_env.delete_value(table_id, &row)?;
            Ok(())
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn delete_range(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        cols: u32,
        range_start: WasmPtr<u8>,
        range_start_len: u32,
        range_end: WasmPtr<u8>,
        range_end_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "delete_range", out, |caller, mem| {
            let start = mem.read_bytes(&caller, range_start, range_start_len)?;
            let end = mem.read_bytes(&caller, range_end, range_end_len)?;
            let n_deleted = caller
                .data()
                .instance_env
                .delete_range(table_id, cols, &start, &end)?;
            Ok(n_deleted)
        })
    }

    /// Create a table with `name`, a UTF-8 slice in WASM memory lasting `name_len` bytes,
    /// and with the table's `schema` in a slice in WASM memory lasting `schema_len` bytes.
    ///
    /// Writes the table id of the new table into the WASM pointer `out`.
    #[tracing::instrument(skip_all)]
    pub fn create_table(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        schema: WasmPtr<u8>,
        schema_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "create_table", out, |caller, mem| {
            // Read `name` from WASM memory, requiring UTF-8 encoding.
            let name = Self::read_string(&caller, mem, name, name_len)?;

            // Read the schema from WASM memory.
            let schema = mem.read_bytes(&caller, schema, schema_len)?;

            // Create the table.
            Ok(caller.data().instance_env.create_table(&name, &schema)?)
        })
    }
    */

    /// Queries the `table_id` associated with the given (table) `name`
    /// where `name` points to a UTF-8 slice in WASM memory of `name_len` bytes.
    ///
    /// The table id is written into the `out` pointer.
    ///
    /// Errors if the table does not exist.
    #[tracing::instrument(skip_all)]
    pub fn get_table_id(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "get_table_id", out, |caller, mem| {
            // Read the table name from WASM memory.
            let name = Self::read_sats_string(&caller, mem, name, name_len)?;

            // Query the table id.
            Ok(caller.data().instance_env.get_table_id(name)?)
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
    /// In the former case, the function will panic,
    /// and in latter, an error is returned.
    ///
    /// Returns an error when a table with the provided `table_id` doesn't exist.
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
        Self::cvt(caller, "create_index", |caller, mem| {
            // Read the index name from WASM memory.
            let index_name = Self::read_sats_string(&caller, mem, index_name, index_name_len)?;

            // Read the column ids on which to create an index from WASM memory.
            // This may be one column or an index on several columns.
            let cols = mem.read_bytes(&caller, col_ids, col_len)?;

            let cols = NonEmpty::from_vec(cols)
                .expect("Attempt to create an index with zero columns")
                .map(|x| ColId(x as u32));
            let cols = cols
                .try_into()
                .expect("The number of columns in the index exceeded `u32::MAX`");

            caller
                .data()
                .instance_env
                .create_index(index_name, table_id, index_type, cols)?;
            Ok(())
        })
    }

    /// Finds all rows in the table identified by `table_id`,
    /// where the row has a column, identified by `cols`,
    /// with data matching the byte string, in WASM memory, pointed to at by `val`.
    ///
    /// Matching is defined by decoding of `value` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// The rows found are bsatn encoded and then concatenated.
    /// The resulting byte string from the concatenation is written
    /// to a fresh buffer with the buffer's identifier written to the WASM pointer `out`.
    #[tracing::instrument(skip_all)]
    pub fn iter_by_col_eq(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        val: WasmPtr<u8>,
        val_len: u32,
        out: WasmPtr<BufferIdx>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "iter_by_col_eq", out, |mut caller, mem| {
            // Read the test value from WASM memory.
            let value = mem.read_bytes(&caller, val, val_len)?;

            // Find the relevant rows.
            let data = caller.data().instance_env.iter_by_col_eq(table_id, col_id, &value)?;

            // Insert the encoded + concatenated rows into a new buffer and return its id.
            Ok(caller.data_mut().buffers.insert(data.into()))
        })
    }

    /// Start iteration on each row, as bytes, of a table identified by `table_id`.
    ///
    /// The iterator is registered in the host environment
    /// under an assigned index which is written to the `out` pointer provided.
    // #[tracing::instrument(skip_all)]
    pub fn iter_start(caller: FunctionEnvMut<'_, Self>, table_id: u32, out: WasmPtr<BufferIterIdx>) -> RtResult<u16> {
        Self::cvt_ret(caller, "iter_start", out, |mut caller, _mem| {
            // Construct the iterator.
            let iter = caller.data().instance_env.iter(table_id);
            // TODO: make it so the above iterator doesn't lock the database for its whole lifetime
            let iter = iter.map_ok(Bytes::from).collect::<Vec<_>>().into_iter();

            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(caller.data_mut().iters.insert(Box::new(iter)))
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
    // #[tracing::instrument(skip_all)]
    pub fn iter_start_filtered(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        filter: WasmPtr<u8>,
        filter_len: u32,
        out: WasmPtr<BufferIterIdx>,
    ) -> RtResult<u16> {
        Self::cvt_ret(caller, "iter_start_filtered", out, |mut caller, _mem| {
            // Read the slice `(filter, filter_len)`.
            let filter = caller.data().mem().read_bytes(&caller, filter, filter_len)?;

            // Construct the iterator.
            let iter = caller.data().instance_env.iter_filtered(table_id, &filter)?;
            // TODO: make it so the above iterator doesn't lock the database for its whole lifetime
            let iter = iter.map(Bytes::from).map(Ok).collect::<Vec<_>>().into_iter();

            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(caller.data_mut().iters.insert(Box::new(iter)))
        })
    }

    /// Advances the registered iterator with the index given by `iter_key`.
    ///
    /// On success, the next element (the row as bytes) is written to a buffer.
    /// The buffer's index is returned and written to the `out` pointer.
    /// If there are no elements left, an invalid buffer index is written to `out`.
    /// On failure however, the error is returned.
    // #[tracing::instrument(skip_all)]
    pub fn iter_next(caller: FunctionEnvMut<'_, Self>, iter_key: u32, out: WasmPtr<BufferIdx>) -> RtResult<u16> {
        Self::cvt_ret(caller, "iter_next", out, |mut caller, _mem| {
            let data_mut = caller.data_mut();

            // Retrieve the iterator by `iter_key`.
            let iter = data_mut
                .iters
                .get_mut(BufferIterIdx(iter_key))
                .ok_or_else(|| RuntimeError::new("no such iterator"))?;

            // Advance the iterator.
            match iter.next() {
                Some(Ok(buf)) => Ok(data_mut.buffers.insert(buf)),
                Some(Err(err)) => Err(err.into()),
                None => Ok(BufferIdx::INVALID),
            }
        })
    }

    /// Drops the entire registered iterator with the index given by `iter_key`.
    /// The iterator is effectively de-registered.
    ///
    /// Returns an error if the iterator does not exist.
    // #[tracing::instrument(skip_all)]
    pub fn iter_drop(caller: FunctionEnvMut<'_, Self>, iter_key: u32) -> RtResult<u16> {
        Self::cvt(caller, "iter_drop", |mut caller, _mem| {
            caller
                .data_mut()
                .iters
                .take(BufferIterIdx(iter_key))
                .ok_or_else(|| RuntimeError::new("no such iterator").into())
                .map(drop)
        })
    }

    /// Returns the length (number of bytes) of the `buffer`.
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

    /// Consumes the `buffer` and moves its contents into the slice `(ptr, len)`.
    ///
    /// Returns an error if the buffer does not exist.
    // #[tracing::instrument(skip_all)]
    pub fn buffer_consume(
        mut caller: FunctionEnvMut<'_, Self>,
        buffer: u32,
        ptr: WasmPtr<u8>,
        len: u32,
    ) -> RtResult<()> {
        let buf = caller
            .data_mut()
            .buffers
            .take(BufferIdx(buffer))
            .ok_or_else(|| RuntimeError::new("no such buffer"))?;
        ptr.slice(&caller.data().mem().view(&caller), len)
            .and_then(|slice| slice.write_slice(&buf))
            .map_err(mem_err)
    }

    /// Creates a buffer of size `data_len`.
    /// The buffer is initialized with the contents at the `data` WASM pointer.
    ///
    /// The buffer is registered in the host environment and is indexed by the returned `u32`.
    // #[tracing::instrument(skip_all)]
    pub fn buffer_alloc(mut caller: FunctionEnvMut<'_, Self>, data: WasmPtr<u8>, data_len: u32) -> RtResult<u32> {
        let buf = caller
            .data()
            .mem()
            .read_bytes(&caller, data, data_len)
            .map_err(mem_err)?;
        Ok(caller.data_mut().buffers.insert(buf.into()).0)
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
