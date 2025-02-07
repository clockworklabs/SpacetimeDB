#![allow(clippy::too_many_arguments)]

use std::time::Instant;

use crate::database_logger::{BacktraceFrame, BacktraceFrameSymbol, BacktraceProvider, ModuleBacktrace, Record};
use crate::host::instance_env::{ChunkPool, InstanceEnv};
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
    /// A pool of unused allocated chunks that can be reused.
    // TODO(Centril): consider using this pool for `console_timer_start` and `bytes_sink_write`.
    chunk_pool: ChunkPool,
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
            chunk_pool: <_>::default(),
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
    #[tracing::instrument(level = "trace", skip_all)]
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

    /// Queries the `index_id` associated with the given (index) `name`
    /// where `name` is the UTF-8 slice in WASM memory at `name_ptr[..name_len]`.
    ///
    /// The index id is written into the `out` pointer.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `name_ptr` is NULL or `name` is not in bounds of WASM memory.
    /// - `name` is not valid UTF-8.
    /// - `out` is NULL or `out[..size_of::<IndexId>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_INDEX`, when `name` is not the name of an index.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn index_id_from_name(
        caller: Caller<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret::<u32>(caller, AbiCall::IndexIdFromName, out, |caller| {
            let (mem, env) = Self::mem_env(caller);
            // Read the index name from WASM memory.
            let name = mem.deref_str(name, name_len)?;

            // Query the index id.
            Ok(env.instance_env.index_id_from_name(name)?.into())
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
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_table_row_count(caller: Caller<'_, Self>, table_id: u32, out: WasmPtr<u64>) -> RtResult<u32> {
        Self::cvt_ret::<u64>(caller, AbiCall::DatastoreTableRowCount, out, |caller| {
            let (_, env) = Self::mem_env(caller);
            Ok(env.instance_env.datastore_table_row_count(table_id.into())?)
        })
    }

    /// Starts iteration on each row, as BSATN-encoded, of a table identified by `table_id`.
    ///
    /// On success, the iterator handle is written to the `out` pointer.
    /// This handle can be advanced by [`row_iter_bsatn_advance`].
    ///
    /// # Traps
    ///
    /// This function does not trap.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `table_id` is not a known ID of a table.
    // #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_table_scan_bsatn(
        caller: Caller<'_, Self>,
        table_id: u32,
        out: WasmPtr<RowIterIdx>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DatastoreTableScanBsatn, out, |caller| {
            let env = caller.data_mut();
            // Collect the iterator chunks.
            let chunks = env
                .instance_env
                .datastore_table_scan_bsatn_chunks(&mut env.chunk_pool, table_id.into())?;
            // Register the iterator and get back the index to write to `out`.
            // Calls to the iterator are done through dynamic dispatch.
            Ok(env.iters.insert(chunks.into_iter()))
        })
    }

    /// Finds all rows in the index identified by `index_id`,
    /// according to the:
    /// - `prefix = prefix_ptr[..prefix_len]`,
    /// - `rstart = rstart_ptr[..rstart_len]`,
    /// - `rend = rend_ptr[..rend_len]`,
    ///
    /// in WASM memory.
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
    /// # Traps
    ///
    /// Traps if:
    /// - `prefix_elems > 0`
    ///    and (`prefix_ptr` is NULL or `prefix` is not in bounds of WASM memory).
    /// - `rstart` is NULL or `rstart` is not in bounds of WASM memory.
    /// - `rend` is NULL or `rend` is not in bounds of WASM memory.
    /// - `out` is NULL or `out[..size_of::<RowIter>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_INDEX`, when `index_id` is not a known ID of an index.
    /// - `WRONG_INDEX_ALGO` if the index is not a range-scan compatible index.
    /// - `BSATN_DECODE_ERROR`, when `prefix` cannot be decoded to
    ///    a `prefix_elems` number of `AlgebraicValue`
    ///    typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
    ///    Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
    ///    where the inner `AlgebraicValue`s are
    ///    typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
    pub fn datastore_index_scan_range_bsatn(
        caller: Caller<'_, Self>,
        index_id: u32,
        prefix_ptr: WasmPtr<u8>,
        prefix_len: u32,
        prefix_elems: u32,
        rstart_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rstart_len: u32,
        rend_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rend_len: u32,
        out: WasmPtr<RowIterIdx>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DatastoreIndexScanRangeBsatn, out, |caller| {
            let prefix_elems = Self::convert_u32_to_col_id(prefix_elems)?;

            let (mem, env) = Self::mem_env(caller);
            // Read the prefix and range start & end from WASM memory.
            let prefix = if prefix_elems.idx() == 0 {
                &[]
            } else {
                mem.deref_slice(prefix_ptr, prefix_len)?
            };
            let rstart = mem.deref_slice(rstart_ptr, rstart_len)?;
            let rend = mem.deref_slice(rend_ptr, rend_len)?;

            // Find the relevant rows.
            let chunks = env.instance_env.datastore_index_scan_range_bsatn_chunks(
                &mut env.chunk_pool,
                index_id.into(),
                prefix,
                prefix_elems,
                rstart,
                rend,
            )?;

            // Insert the encoded + concatenated rows into a new buffer and return its id.
            Ok(env.iters.insert(chunks.into_iter()))
        })
    }

    /// Deprecated name for [`Self::datastore_index_scan_range_bsatn`].
    #[deprecated = "use `datastore_index_scan_range_bsatn` instead"]
    pub fn datastore_btree_scan_bsatn(
        caller: Caller<'_, Self>,
        index_id: u32,
        prefix_ptr: WasmPtr<u8>,
        prefix_len: u32,
        prefix_elems: u32,
        rstart_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rstart_len: u32,
        rend_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rend_len: u32,
        out: WasmPtr<RowIterIdx>,
    ) -> RtResult<u32> {
        Self::datastore_index_scan_range_bsatn(
            caller,
            index_id,
            prefix_ptr,
            prefix_len,
            prefix_elems,
            rstart_ptr,
            rstart_len,
            rend_ptr,
            rend_len,
            out,
        )
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
    // #[tracing::instrument(level = "trace", skip_all)]
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
                let Some((buf_chunk, rest)) = buffer.split_at_mut_checked(chunk.len()) else {
                    // Cannot fit chunk into the buffer,
                    // either because we already filled it too much,
                    // or because it is too small.
                    break;
                };
                buf_chunk.copy_from_slice(chunk);
                written += chunk.len();
                buffer = rest;

                // Advance the iterator, as we used a chunk.
                // SAFETY: We peeked one `chunk`, so there must be one at least.
                let chunk = unsafe { iter.next().unwrap_unchecked() };
                env.chunk_pool.put(chunk);
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
    // #[tracing::instrument(level = "trace", skip_all)]
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

    /// Inserts a row into the table identified by `table_id`,
    /// where the row is read from the byte string `row = row_ptr[..row_len]` in WASM memory
    /// where `row_len = row_len_ptr[..size_of::<usize>()]` stores the capacity of `row`.
    ///
    /// The byte string `row` must be a BSATN-encoded `ProductValue`
    /// typed at the table's `ProductType` row-schema.
    ///
    /// To handle auto-incrementing columns,
    /// when the call is successful,
    /// the `row` is written back to with the generated sequence values.
    /// These values are written as a BSATN-encoded `pv: ProductValue`.
    /// Each `v: AlgebraicValue` in `pv` is typed at the sequence's column type.
    /// The `v`s in `pv` are ordered by the order of the columns, in the schema of the table.
    /// When the table has no sequences,
    /// this implies that the `pv`, and thus `row`, will be empty.
    /// The `row_len` is set to the length of `bsatn(pv)`.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `row_len_ptr` is NULL or `row_len` is not in bounds of WASM memory.
    /// - `row_ptr` is NULL or `row` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `table_id` is not a known ID of a table.
    /// - `BSATN_DECODE_ERROR`, when `row` cannot be decoded to a `ProductValue`.
    ///   typed at the `ProductType` the table's schema specifies.
    /// - `UNIQUE_ALREADY_EXISTS`, when inserting `row` would violate a unique constraint.
    /// - `SCHEDULE_AT_DELAY_TOO_LONG`, when the delay specified in the row was too long.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_insert_bsatn(
        caller: Caller<'_, Self>,
        table_id: u32,
        row_ptr: WasmPtr<u8>,
        row_len_ptr: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt(caller, AbiCall::DatastoreInsertBsatn, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Read `row-len`, i.e., the capacity of `row` pointed to by `row_ptr`.
            let row_len = u32::read_from(mem, row_len_ptr)?;
            // Get a mutable view to the `row`.
            let row = mem.deref_slice_mut(row_ptr, row_len)?;

            // Insert the row into the DB and write back the generated column values.
            let row_len = env.instance_env.insert(table_id.into(), row)?;
            u32::try_from(row_len).unwrap().write_to(mem, row_len_ptr)?;
            Ok(())
        })
    }

    /// Updates a row in the table identified by `table_id` to `row`
    /// where the row is read from the byte string `row = row_ptr[..row_len]` in WASM memory
    /// where `row_len = row_len_ptr[..size_of::<usize>()]` stores the capacity of `row`.
    ///
    /// The byte string `row` must be a BSATN-encoded `ProductValue`
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
    /// The `row_len` is set to the length of `bsatn(pv)`.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `row_len_ptr` is NULL or `row_len` is not in bounds of WASM memory.
    /// - `row_ptr` is NULL or `row` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `table_id` is not a known ID of a table.
    /// - `NO_SUCH_INDEX`, when `index_id` is not a known ID of an index.
    /// - `INDEX_NOT_UNIQUE`, when the index was not unique.
    /// - `BSATN_DECODE_ERROR`, when `row` cannot be decoded to a `ProductValue`
    ///    typed at the `ProductType` the table's schema specifies
    ///    or when it cannot be projected to the index identified by `index_id`.
    /// - `NO_SUCH_ROW`, when the row was not found in the unique index.
    /// - `UNIQUE_ALREADY_EXISTS`, when inserting `row` would violate a unique constraint.
    /// - `SCHEDULE_AT_DELAY_TOO_LONG`, when the delay specified in the row was too long.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_update_bsatn(
        caller: Caller<'_, Self>,
        table_id: u32,
        index_id: u32,
        row_ptr: WasmPtr<u8>,
        row_len_ptr: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt(caller, AbiCall::DatastoreUpdateBsatn, |caller| {
            let (mem, env) = Self::mem_env(caller);

            // Read `row-len`, i.e., the capacity of `row` pointed to by `row_ptr`.
            let row_len = u32::read_from(mem, row_len_ptr)?;
            // Get a mutable view to the `row`.
            let row = mem.deref_slice_mut(row_ptr, row_len)?;

            // Update the row in the DB and write back the generated column values.
            let row_len = env.instance_env.update(table_id.into(), index_id.into(), row)?;
            u32::try_from(row_len).unwrap().write_to(mem, row_len_ptr)?;
            Ok(())
        })
    }

    /// Deletes all rows found in the index identified by `index_id`,
    /// according to the:
    /// - `prefix = prefix_ptr[..prefix_len]`,
    /// - `rstart = rstart_ptr[..rstart_len]`,
    /// - `rend = rend_ptr[..rend_len]`,
    ///
    /// in WASM memory.
    ///
    /// This syscall will delete all the rows found by
    /// [`datastore_index_scan_range_bsatn`] with the same arguments passed,
    /// including `prefix_elems`.
    /// See `datastore_index_scan_range_bsatn` for details.
    ///
    /// The number of rows deleted is written to the WASM pointer `out`.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `prefix_elems > 0`
    ///    and (`prefix_ptr` is NULL or `prefix` is not in bounds of WASM memory).
    /// - `rstart` is NULL or `rstart` is not in bounds of WASM memory.
    /// - `rend` is NULL or `rend` is not in bounds of WASM memory.
    /// - `out` is NULL or `out[..size_of::<u32>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_INDEX`, when `index_id` is not a known ID of an index.
    /// - `WRONG_INDEX_ALGO` if the index is not a range-compatible index.
    /// - `BSATN_DECODE_ERROR`, when `prefix` cannot be decoded to
    ///    a `prefix_elems` number of `AlgebraicValue`
    ///    typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
    ///    Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
    ///    where the inner `AlgebraicValue`s are
    ///    typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
    pub fn datastore_delete_by_index_scan_range_bsatn(
        caller: Caller<'_, Self>,
        index_id: u32,
        prefix_ptr: WasmPtr<u8>,
        prefix_len: u32,
        prefix_elems: u32,
        rstart_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rstart_len: u32,
        rend_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rend_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DatastoreDeleteByIndexScanRangeBsatn, out, |caller| {
            let prefix_elems = Self::convert_u32_to_col_id(prefix_elems)?;

            let (mem, env) = Self::mem_env(caller);
            // Read the prefix and range start & end from WASM memory.
            let prefix = if prefix_elems.idx() == 0 {
                &[]
            } else {
                mem.deref_slice(prefix_ptr, prefix_len)?
            };
            let rstart = mem.deref_slice(rstart_ptr, rstart_len)?;
            let rend = mem.deref_slice(rend_ptr, rend_len)?;

            // Delete the relevant rows.
            Ok(env.instance_env.datastore_delete_by_index_scan_range_bsatn(
                index_id.into(),
                prefix,
                prefix_elems,
                rstart,
                rend,
            )?)
        })
    }

    /// Deprecated name for [`Self::datastore_delete_by_index_scan_range_bsatn`].
    #[deprecated = "use `datastore_delete_by_index_scan_range_bsatn` instead"]
    pub fn datastore_delete_by_btree_scan_bsatn(
        caller: Caller<'_, Self>,
        index_id: u32,
        prefix_ptr: WasmPtr<u8>,
        prefix_len: u32,
        prefix_elems: u32,
        rstart_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rstart_len: u32,
        rend_ptr: WasmPtr<u8>, // Bound<AlgebraicValue>
        rend_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::datastore_delete_by_index_scan_range_bsatn(
            caller,
            index_id,
            prefix_ptr,
            prefix_len,
            prefix_elems,
            rstart_ptr,
            rstart_len,
            rend_ptr,
            rend_len,
            out,
        )
    }

    /// Deletes those rows, in the table identified by `table_id`,
    /// that match any row in the byte string `rel = rel_ptr[..rel_len]` in WASM memory.
    ///
    /// Matching is defined by first BSATN-decoding
    /// the byte string pointed to at by `relation` to a `Vec<ProductValue>`
    /// according to the row schema of the table
    /// and then using `Ord for AlgebraicValue`.
    /// A match happens when `Ordering::Equal` is returned from `fn cmp`.
    /// This occurs exactly when the row's BSATN-encoding is equal to the encoding of the `ProductValue`.
    ///
    /// The number of rows deleted is written to the WASM pointer `out`.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `rel_ptr` is NULL or `rel` is not in bounds of WASM memory.
    /// - `out` is NULL or `out[..size_of::<u32>()]` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    /// - `NO_SUCH_TABLE`, when `table_id` is not a known ID of a table.
    /// - `BSATN_DECODE_ERROR`, when `rel` cannot be decoded to `Vec<ProductValue>`
    ///   where each `ProductValue` is typed at the `ProductType` the table's schema specifies.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_delete_all_by_eq_bsatn(
        caller: Caller<'_, Self>,
        table_id: u32,
        rel_ptr: WasmPtr<u8>,
        rel_len: u32,
        out: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::DatastoreDeleteAllByEqBsatn, out, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let relation = mem.deref_slice(rel_ptr, rel_len)?;
            Ok(env
                .instance_env
                .datastore_delete_all_by_eq_bsatn(table_id.into(), relation)?)
        })
    }

    pub fn volatile_nonatomic_schedule_immediate(
        caller: Caller<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        args: WasmPtr<u8>,
        args_len: u32,
    ) -> RtResult<()> {
        Self::with_span(caller, AbiCall::VolatileNonatomicScheduleImmediate, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let name = mem.deref_str(name, name_len)?;
            let args = mem.deref_slice(args, args_len)?;
            env.instance_env.scheduler.volatile_nonatomic_schedule_immediate(
                name.to_owned(),
                crate::host::ReducerArgs::Bsatn(args.to_vec().into()),
            );

            Ok(())
        })
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

    /// Logs at `level` a `message` message occuring in `filename:line_number`
    /// with [`target`](target) being the module path at the `log!` invocation site.
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
    /// # Traps
    ///
    /// Traps if:
    /// - `target` is not NULL and `target_ptr[..target_len]` is not in bounds of WASM memory.
    /// - `filename` is not NULL and `filename_ptr[..filename_len]` is not in bounds of WASM memory.
    /// - `message` is not NULL and `message_ptr[..message_len]` is not in bounds of WASM memory.
    ///
    /// [target]: https://docs.rs/log/latest/log/struct.Record.html#method.target
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn console_log(
        caller: Caller<'_, Self>,
        level: u32,
        target_ptr: WasmPtr<u8>,
        target_len: u32,
        filename_ptr: WasmPtr<u8>,
        filename_len: u32,
        line_number: u32,
        message_ptr: WasmPtr<u8>,
        message_len: u32,
    ) {
        let do_console_log = |caller: &mut Caller<'_, Self>| -> WasmResult<()> {
            let env = caller.data();
            let mem = env.get_mem().view(&caller);

            // Read the `target`, `filename`, and `message` strings from WASM memory.
            let target = mem.deref_str_lossy(target_ptr, target_len).check_nullptr()?;
            let filename = mem.deref_str_lossy(filename_ptr, filename_len).check_nullptr()?;
            let message = mem.deref_str_lossy(message_ptr, message_len)?;

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

            // Write the log record to the `DatabaseLogger` in the database instance context (replica_ctx).
            env.instance_env
                .console_log((level as u8).into(), &record, &caller.as_context());
            Ok(())
        };
        Self::cvt_noret(caller, AbiCall::ConsoleLog, |caller| {
            let _ = do_console_log(caller);
        })
    }

    /// Begins a timing span with `name = name_ptr[..name_len]`.
    ///
    /// When the returned `ConsoleTimerId` is passed to [`console_timer_end`],
    /// the duration between the calls will be printed to the module's logs.
    ///
    /// The `name` is interpreted lossily as a UTF-8 string.
    ///
    /// # Traps
    ///
    /// Traps if:
    /// - `name_ptr` is NULL or `name` is not in bounds of WASM memory.
    pub fn console_timer_start(caller: Caller<'_, Self>, name_ptr: WasmPtr<u8>, name_len: u32) -> RtResult<u32> {
        Self::with_span(caller, AbiCall::ConsoleTimerStart, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let name = mem.deref_str_lossy(name_ptr, name_len)?.into_owned();
            Ok(env.timing_spans.insert(TimingSpan::new(name)).0)
        })
    }

    pub fn console_timer_end(caller: Caller<'_, Self>, span_id: u32) -> RtResult<u32> {
        Self::cvt_custom(caller, AbiCall::ConsoleTimerEnd, |caller| {
            let Some(span) = caller.data_mut().timing_spans.take(TimingSpanIdx(span_id)) else {
                return Ok(errno::NO_SUCH_CONSOLE_TIMER.get().into());
            };

            let elapsed = span.start.elapsed();
            let message = format!("Timing span {:?}: {:?}", &span.name, elapsed);

            let record = Record {
                ts: chrono::Utc::now(),
                target: None,
                filename: None,
                line_number: None,
                message: &message,
            };
            caller.data().instance_env.console_log(
                crate::database_logger::LogLevel::Info,
                &record,
                &caller.as_context(),
            );
            Ok(0)
        })
    }

    /// Writes the identity of the module into `out = out_ptr[..32]`.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - `out_ptr` is NULL or `out` is not in bounds of WASM memory.
    pub fn identity(caller: Caller<'_, Self>, out_ptr: WasmPtr<u8>) -> RtResult<()> {
        // Use `with_span` rather than one of the `cvt_*` functions,
        // as we want to possibly trap, but not to return an error code.
        Self::with_span(caller, AbiCall::Identity, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let identity = env.instance_env.replica_ctx.database.database_identity;
            // We're implicitly casting `out_ptr` to `WasmPtr<Identity>` here.
            // (Both types are actually `u32`.)
            // This works because `Identity::write_to` does not require an aligned pointer,
            // as it gets a `&mut [u8]` from WASM memory and does `copy_from_slice` with it.
            identity.write_to(mem, out_ptr)?;
            Ok(())
        })
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
                module_name: f.module().name(),
                func_name: f.func_name(),
                symbols: f.symbols().iter().map(BacktraceFrameSymbol::from).collect(),
            })
            .collect()
    }
}

impl<'a> From<&'a wasmtime::FrameSymbol> for BacktraceFrameSymbol<'a> {
    fn from(sym: &'a wasmtime::FrameSymbol) -> Self {
        Self {
            name: sym.name(),
            file: sym.file(),
            line: sym.line(),
            column: sym.column(),
        }
    }
}
