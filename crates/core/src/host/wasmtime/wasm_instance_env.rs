#![allow(clippy::too_many_arguments)]

use super::{Mem, MemView, NullableMemOp, WasmError, WasmPointee, WasmPtr};
use crate::database_logger::{BacktraceFrame, BacktraceProvider, ModuleBacktrace, Record};
use crate::host::instance_env::{ChunkPool, InstanceEnv};
use crate::host::wasm_common::instrumentation::{span, CallTimes};
use crate::host::wasm_common::module_host_actor::ExecutionTimings;
use crate::host::wasm_common::{err_to_errno_and_log, RowIterIdx, RowIters, TimingSpan, TimingSpanIdx, TimingSpanSet};
use crate::host::AbiCall;
use anyhow::Context as _;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::{ConnectionId, Timestamp};
use spacetimedb_primitives::{errno, ColId};
use std::future::Future;
use std::num::NonZeroU32;
use std::time::Instant;
use wasmtime::{AsContext, Caller, StoreContextMut};

/// A stream of bytes which the WASM module can read from
/// using [`WasmInstanceEnv::bytes_source_read`].
///
/// These are managed in the `bytes_sources` of [`WasmInstanceEnv`],
/// where each one is paired with an integer ID.
/// This is basically a massively-simplified version of Unix read files and file descriptors.
///
/// Unlike Unix read files, we implicitly close `BytesSource`s once they are read to the end.
/// This is sensible because we don't provide a seek operation,
/// so the `BytesSource` becomes useless once read to the end.
struct BytesSource {
    /// The actual bytes which will be returned by calls to `byte_source_read`.
    ///
    /// When this becomes empty, this `ByteSource` is expended and should be discarded.
    bytes: bytes::Bytes,
}

/// Identifier for a [`BytesSource`] stored in the `bytes_sources` of a [`WasmInstanceEnv`].
///
/// The special sentinel [`Self::INVALID`] (zero) is used for a never-readable [`BytesSource`].
/// We pass this to guests for a [`BytesSource`] with a length of zero
/// so that they can avoid host calls.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) struct BytesSourceId(pub(super) u32);

// `nohash_hasher` recommends impling `Hash` explicitly rather than using the derive macro,
// as the derive macro is not technically guaranteed to only call `hasher.write_{int}` for an integer newtype,
// even though any other behavior would be deranged.
impl std::hash::Hash for BytesSourceId {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u32(self.0)
    }
}

impl nohash_hasher::IsEnabled for BytesSourceId {}

impl BytesSourceId {
    const INVALID: Self = Self(0);
}

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

    /// `File`-like [`BytesSource`]s which guest code can read via [`Self::bytes_source_read`].
    ///
    /// These are essentially simplified versions of Unix read files,
    /// with [`BytesSourceId`] being file descriptors.
    ///
    /// Unlike Unix files, we implicitly close a [`BytesSource`] when it is read to the end.
    /// This is because we don't provide a seek operation and a [`BytesSource`] never grows after initialization.
    bytes_sources: IntMap<BytesSourceId, BytesSource>,

    /// Counter as a source of [`BytesSourceId`] values.
    ///
    /// Recall that zero is [`BytesSourceId::INVALID`], so we have to start at 1.
    next_bytes_source_id: NonZeroU32,

    /// The standard sink used for [`Self::bytes_sink_write`].
    standard_bytes_sink: Option<Vec<u8>>,

    /// The slab of `BufferIters` created for this instance.
    iters: RowIters,

    /// Track time spent in module-defined spans.
    timing_spans: TimingSpanSet,

    /// The point in time the last, or current, reducer or procedure call started at.
    funcall_start: Instant,

    /// Track time spent in all wasm instance env calls (aka syscall time).
    ///
    /// Each function, like `insert`, will add the `Duration` spent in it
    /// to this tracker.
    call_times: CallTimes,

    /// The name of the last, including current, reducer or procedure to be executed by this environment.
    funcall_name: String,

    /// A pool of unused allocated chunks that can be reused.
    // TODO(Centril): consider using this pool for `console_timer_start` and `bytes_sink_write`.
    chunk_pool: ChunkPool,
}

const STANDARD_BYTES_SINK: u32 = 1;

type WasmResult<T> = Result<T, WasmError>;
type RtResult<T> = anyhow::Result<T>;

/// Wraps an `InstanceEnv` with the magic necessary to push
/// and pull bytes from webassembly memory.
impl WasmInstanceEnv {
    /// Create a new `WasmEnstanceEnv` from the given `InstanceEnv`.
    pub fn new(instance_env: InstanceEnv) -> Self {
        let funcall_start = Instant::now();
        Self {
            instance_env,
            mem: None,
            bytes_sources: IntMap::default(),
            next_bytes_source_id: NonZeroU32::new(1).unwrap(),
            standard_bytes_sink: None,
            iters: Default::default(),
            timing_spans: Default::default(),
            funcall_start,
            call_times: CallTimes::new(),
            funcall_name: String::from("<initializing>"),
            chunk_pool: <_>::default(),
        }
    }

    fn alloc_bytes_source_id(&mut self) -> RtResult<BytesSourceId> {
        let id = self.next_bytes_source_id;
        self.next_bytes_source_id = id
            .checked_add(1)
            .context("Allocating next `BytesSourceId` overflowed `u32`")?;
        Ok(BytesSourceId(id.into()))
    }

    /// Binds `bytes` to the environment and assigns it an ID.
    ///
    /// If `bytes` is empty, `BytesSourceId::INVALID` is returned.
    fn create_bytes_source(&mut self, bytes: bytes::Bytes) -> RtResult<BytesSourceId> {
        // Pass an invalid source when the bytes were empty.
        // This allows the module to avoid allocating and make a system call in those cases.
        if bytes.is_empty() {
            Ok(BytesSourceId::INVALID)
        } else if bytes.len() > u32::MAX as usize {
            // There's no inherent reason we need to error here,
            // other than that it makes it impossible to report the length in `bytes_source_remaining_length`
            // and that all of our usage of `BytesSource`s as of writing (pgoldman 2025-09-26)
            // are to immediately slurp the whole thing into a buffer in guest memory,
            // which can't hold buffers this big because it's WASM32.
            Err(anyhow::anyhow!(
                "`create_bytes_source`: `Bytes` has length {}, which is greater than `u32::MAX` {}",
                bytes.len(),
                u32::MAX,
            ))
        } else {
            let id = self.alloc_bytes_source_id()?;
            self.bytes_sources.insert(id, BytesSource { bytes });
            Ok(id)
        }
    }

    fn free_bytes_source(&mut self, id: BytesSourceId) {
        if self.bytes_sources.remove(&id).is_none() {
            log::warn!("`free_bytes_source` on non-existent source {id:?}");
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

    /// Signal to this `WasmInstanceEnv` that a reducer or procedure call is beginning.
    ///
    /// Returns the handle used by reducers and procedures to read from `args`
    /// as well as the handle used to write the reducer error message or procedure return value.
    pub fn start_funcall(&mut self, name: &str, args: bytes::Bytes, ts: Timestamp) -> (BytesSourceId, u32) {
        // Create the output sink.
        // Reducers which fail will write their error message here.
        // Procedures will write their result here.
        let errors = self.setup_standard_bytes_sink();

        let args = self.create_bytes_source(args).unwrap();

        self.funcall_start = Instant::now();
        name.clone_into(&mut self.funcall_name);
        self.instance_env.start_funcall(ts);

        (args, errors)
    }

    /// Returns the name of the most recent reducer or procedure to be run in this environment.
    pub fn funcall_name(&self) -> &str {
        &self.funcall_name
    }

    /// Returns the name of the most recent reducer or procedure to be run in this environment,
    /// or `None` if no reducer or procedure is actively being invoked.
    fn log_record_function(&self) -> Option<&str> {
        let function = self.funcall_name();
        (!function.is_empty()).then_some(function)
    }

    /// Returns the start time of the most recent reducer or procedure to be run in this environment.
    pub fn funcall_start(&self) -> Instant {
        self.funcall_start
    }

    /// Signal to this `WasmInstanceEnv` that a reducer or procedure call is over.
    ///
    /// Returns time measurements which can be recorded as metrics,
    /// and the errors written by the WASM code to the standard error sink.
    ///
    /// This resets the call times and clears the arguments source and error sink.
    pub fn finish_funcall(&mut self) -> (ExecutionTimings, Vec<u8>) {
        // For the moment,
        // we only explicitly clear the source/sink buffers and the "syscall" times.
        // TODO: should we be clearing `iters` and/or `timing_spans`?

        let total_duration = self.funcall_start.elapsed();

        // Taking the call times record also resets timings to 0s for the next call.
        let wasm_instance_env_call_times = self.call_times.take();

        let timings = ExecutionTimings {
            total_duration,
            wasm_instance_env_call_times,
        };

        // Drop any outstanding bytes sources and reset the ID counter,
        // so that we don't leak either the IDs or the buffers themselves.
        self.bytes_sources = IntMap::default();
        self.next_bytes_source_id = NonZeroU32::new(1).unwrap();

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
        match err {
            WasmError::Db(err) => err_to_errno_and_log(func, err),
            WasmError::BufferTooSmall => Ok(errno::BUFFER_TOO_SMALL.get().into()),
            WasmError::Wasm(err) => Err(err),
        }
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
    ///   and (`prefix_ptr` is NULL or `prefix` is not in bounds of WASM memory).
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
    ///   a `prefix_elems` number of `AlgebraicValue`
    ///   typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
    ///   Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
    ///   where the inner `AlgebraicValue`s are
    ///   typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
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
            let buffer = mem.deref_slice_mut(buffer_ptr, buffer_len)?;

            // Fill the buffer as much as possible.
            let written = InstanceEnv::fill_buffer_from_iter(iter, buffer, &mut env.chunk_pool);

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
    ///   and (`prefix_ptr` is NULL or `prefix` is not in bounds of WASM memory).
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
    ///   a `prefix_elems` number of `AlgebraicValue`
    ///   typed at the initial `prefix_elems` `AlgebraicType`s of the index's key type.
    ///   Or when `rstart` or `rend` cannot be decoded to an `Bound<AlgebraicValue>`
    ///   where the inner `AlgebraicValue`s are
    ///   typed at the `prefix_elems + 1` `AlgebraicType` of the index's key type.
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
                crate::host::FunctionArgs::Bsatn(args.to_vec().into()),
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

            let source = BytesSourceId(source);

            // Retrieve the reducer args if available and requested, or error.
            let Some(bytes_source) = env.bytes_sources.get_mut(&source) else {
                return Ok(errno::NO_SUCH_BYTES.get().into());
            };

            // Read `buffer_len`, i.e., the capacity of `buffer` pointed to by `buffer_ptr`.
            let buffer_len = u32::read_from(mem, buffer_len_ptr)?;
            // Get a mutable view to the `buffer`.
            let buffer = mem.deref_slice_mut(buffer_ptr, buffer_len)?;
            let buffer_len = buffer_len as usize;

            // Derive the portion that we can read and what remains,
            // based on what is left to read and the capacity.
            let can_read_len = buffer_len.min(bytes_source.bytes.len());
            let can_read = bytes_source.bytes.split_to(can_read_len);
            // Copy to the `buffer` and write written bytes count to `buffer_len`.
            buffer[..can_read_len].copy_from_slice(&can_read);
            (can_read_len as u32).write_to(mem, buffer_len_ptr)?;

            // Destroy the source if exhausted, or advance `cursor`.
            if bytes_source.bytes.is_empty() {
                env.free_bytes_source(source);
                Ok(-1i32)
            } else {
                Ok(0)
            }
        })
    }

    /// Read the remaining length of a [`BytesSource`] and write it to `out`.
    ///
    /// Note that the host automatically frees byte sources which are exhausted.
    /// Such sources are invalid, and this method will return an error when passed one.
    /// Callers of [`Self::bytes_source_read`] should check for a return of -1
    /// before invoking this function on the same `source`.
    ///
    /// Also note that the special [`BytesSourceId::INVALID`] (zero) is always invalid.
    /// Callers should check for that value before invoking this function.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - `out` is NULL or `out` is not in bounds of WASM memory.
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NO_SUCH_BYTES`, when `source` is not a valid bytes source.
    ///
    /// If this function returns an error, `out` is not written.
    pub fn bytes_source_remaining_length(caller: Caller<'_, Self>, source: u32, out: WasmPtr<u32>) -> RtResult<i32> {
        Self::cvt_custom(caller, AbiCall::BytesSourceRemainingLength, |caller| {
            let (mem, env) = Self::mem_env(caller);

            let Some(bytes_source) = env.bytes_sources.get(&BytesSourceId(source)) else {
                return Ok(errno::NO_SUCH_BYTES.get().into());
            };

            let remaining: u32 = bytes_source
                .bytes
                .len()
                .try_into()
                // TODO: Change this into an `errno::BYTES_SOURCE_LENGTH_UNKNOWN` rather than a trap,
                // so that we can support very large `BytesSource`s, streams, and other file-like things that aren't just `Bytes`.
                // This is not currently (pgoldman 2025-09-26) a useful thing to do,
                // as all of our uses of `BytesSource` are to slurp the whole source into a single buffer in guest memory,
                // `File::read_to_end`-style, and we don't have any use for large or streaming `BytesSource`s.
                .context("Bytes object in `BytesSource` had length greater than range of u32")?;

            u32::write_to(remaining, mem, out)
                .context("Failed to write output from `bytes_source_remaining_length`")?;

            Ok(0)
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
    ///   (Doesn't currently happen.)
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

    /// Logs at `level` a `message` message occurring in `filename:line_number`
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

            let function = env.log_record_function();

            let record = Record {
                ts: InstanceEnv::now_for_logging(),
                target: target.as_deref(),
                filename: filename.as_deref(),
                line_number,
                function,
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
            let function = caller.data().log_record_function();
            caller.data().instance_env.console_timer_end(&span, function);
            Ok(0)
        })
    }

    /// Finds the JWT payload associated with `connection_id`.
    /// A `[ByteSourceId]` for the payload will be written to `target_ptr`.
    /// If nothing is found for the connection, `[ByteSourceId::INVALID]` (zero) is written to `target_ptr`.
    ///
    /// This must be called inside a transaction (because it reads from a system table).
    ///
    /// # Errors
    ///
    /// Returns an error:
    ///
    /// - `NOT_IN_TRANSACTION`, when called outside of a transaction.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - `connection_id` does not point to a valid little-endian `ConnectionId`.
    /// - `target_ptr` is NULL or `target_ptr[..size_of::<u32>()]` is not in bounds of WASM memory.
    ///  - The `ByteSourceId` to be written to `target_ptr` would overflow [`u32::MAX`].
    pub fn get_jwt(
        caller: Caller<'_, Self>,
        connection_id: WasmPtr<ConnectionId>,
        target_ptr: WasmPtr<u32>,
    ) -> RtResult<u32> {
        Self::cvt_ret(caller, AbiCall::GetJwt, target_ptr, |caller| {
            let (mem, env) = Self::mem_env(caller);
            let cid = ConnectionId::read_from(mem, connection_id)?;
            let jwt = env.instance_env.get_jwt_payload(cid)?;
            let jwt = match jwt {
                None => {
                    // We should consider logging a warning here, since we don't expect any
                    // connection ids to not have a JWT after we migrate.
                    return Ok(0u32);
                }
                Some(jwt) => jwt,
            };
            let b = bytes::Bytes::from(jwt);
            let source_id = env.create_bytes_source(b)?;
            Ok(source_id.0)
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
            let identity = env.instance_env.database_identity();
            // We're implicitly casting `out_ptr` to `WasmPtr<Identity>` here.
            // (Both types are actually `u32`.)
            // This works because `Identity::write_to` does not require an aligned pointer,
            // as it gets a `&mut [u8]` from WASM memory and does `copy_from_slice` with it.
            identity.write_to(mem, out_ptr)?;
            Ok(())
        })
    }

    /// Suspends execution of this WASM instance until approximately `wake_at_micros_since_unix_epoch`.
    ///
    /// Returns immediately if `wake_at_micros_since_unix_epoch` is in the past.
    ///
    /// Upon resuming, returns the current timestamp as microseconds since the Unix epoch.
    ///
    /// Not particularly useful, except for testing SpacetimeDB internals related to suspending procedure execution.
    ///
    /// In our public module-facing interfaces, this function is marked as unstable.
    ///
    /// # Traps
    ///
    /// Traps if:
    ///
    /// - The calling WASM instance is holding open a transaction.
    /// - The calling WASM instance is not executing a procedure.
    // TODO(procedure-sleep-until): remove this
    pub fn procedure_sleep_until<'caller>(
        mut caller: Caller<'caller, Self>,
        (wake_at_micros_since_unix_epoch,): (i64,),
    ) -> Box<dyn Future<Output = i64> + Send + 'caller> {
        Box::new(async move {
            use std::time::SystemTime;
            let span_start = span::CallSpanStart::new(AbiCall::ProcedureSleepUntil);

            let get_current_time = || Timestamp::now().to_micros_since_unix_epoch();

            if wake_at_micros_since_unix_epoch < 0 {
                return get_current_time();
            }

            let wake_at = Timestamp::from_micros_since_unix_epoch(wake_at_micros_since_unix_epoch);
            let Ok(duration) = SystemTime::from(wake_at).duration_since(SystemTime::now()) else {
                return get_current_time();
            };

            tokio::time::sleep(duration).await;

            let res = get_current_time();

            let span = span_start.end();
            span::record_span(&mut caller.data_mut().call_times, span);

            res
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
                module_name: None,
                func_name: f.func_name(),
            })
            .collect()
    }
}
