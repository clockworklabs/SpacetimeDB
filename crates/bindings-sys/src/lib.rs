//! Defines sys calls to interact with SpacetimeDB.
//! This forms an ABI of sorts that modules written in Rust can use.

extern crate alloc;

use core::fmt;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;
use std::ptr;

use spacetimedb_primitives::{errno, errnos, ColId, TableId};

/// Provides a raw set of sys calls which abstractions can be built atop of.
pub mod raw {
    use spacetimedb_primitives::{ColId, TableId};

    // this module identifier determines the abi version that modules built with this crate depend
    // on. Any non-breaking additions to the abi surface should be put in a new `extern {}` block
    // with a module identifier with a minor version 1 above the previous highest minor version.
    // For breaking changes, all functions should be moved into one new `spacetime_X.0` block.
    #[link(wasm_import_module = "spacetime_10.0")]
    extern "C" {
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
        pub fn _table_id_from_name(name: *const u8, name_len: usize, out: *mut TableId) -> u16;

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
        pub fn _datastore_table_row_count(table_id: TableId, out: *mut u64) -> u16;

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
        pub fn _datastore_table_scan_bsatn(table_id: TableId, out: *mut RowIter) -> u16;

        /// Finds all rows in the table identified by `table_id`,
        /// where the row has a column, identified by `col_id`,
        /// with data matching the byte string, in WASM memory, pointed to at by `val`.
        ///
        /// Matching is defined BSATN-decoding `val` to an `AlgebraicValue`
        /// according to the column's schema and then `Ord for AlgebraicValue`.
        ///
        /// On success, the iterator handle is written to the `out` pointer.
        ///
        /// Returns an error if
        /// - a table with the provided `table_id` doesn't exist
        /// - `col_id` does not identify a column of the table,
        /// - `(val, val_len)` cannot be decoded to an `AlgebraicValue`
        ///   typed at the `AlgebraicType` of the column,
        /// - `val + val_len` overflows a 64-bit integer
        pub fn _iter_by_col_eq(
            table_id: TableId,
            col_id: ColId,
            val: *const u8,
            val_len: usize,
            out: *mut RowIter,
        ) -> u16;

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
        pub fn _insert(table_id: TableId, row: *mut u8, row_len: usize) -> u16;

        /// Deletes all rows in the table identified by `table_id`
        /// where the column identified by `col_id` matches the byte string,
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
        pub fn _delete_by_col_eq(
            table_id: TableId,
            col_id: ColId,
            value: *const u8,
            value_len: usize,
            out: *mut u32,
        ) -> u16;

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
        pub fn _delete_by_rel(table_id: TableId, relation: *const u8, relation_len: usize, out: *mut u32) -> u16;

        /// Like [`_datastore_table_scan_bsatn`], start iteration on each row,
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
        pub fn _iter_start_filtered(table_id: TableId, filter: *const u8, filter_len: usize, out: *mut RowIter) -> u16;

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
        pub fn _row_iter_bsatn_advance(iter: RowIter, buffer: *mut u8, buffer_len: *mut usize) -> i16;

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
        pub fn _row_iter_bsatn_close(iter: RowIter) -> u16;

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
        pub fn _console_log(
            level: u8,
            target: *const u8,
            target_len: usize,
            filename: *const u8,
            filename_len: usize,
            line_number: u32,
            message: *const u8,
            message_len: usize,
        );

        /// Schedules a reducer to be called asynchronously, nonatomically,
        /// and immediately on a best effort basis.
        ///
        /// The reducer is named as the valid UTF-8 slice `(name, name_len)`,
        /// and is passed the slice `(args, args_len)` as its argument.
        ///
        /// Traps if
        /// - `name` does not point to valid UTF-8
        /// - `name + name_len` or `args + args_len` overflow a 64-bit integer
        #[cfg(feature = "unstable_abi")]
        pub fn _volatile_nonatomic_schedule_immediate(
            name: *const u8,
            name_len: usize,
            args: *const u8,
            args_len: usize,
        );

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
        pub fn _bytes_sink_write(sink: BytesSink, buffer_ptr: *const u8, buffer_len_ptr: *mut usize) -> u16;

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
        pub fn _bytes_source_read(source: BytesSource, buffer_ptr: *mut u8, buffer_len_ptr: *mut usize) -> i16;

        /// Begin a timing span.
        ///
        /// When the returned `u32` span ID is passed to [`_span_end`],
        /// the duration between the calls will be printed to the module's logs.
        ///
        /// The slice (`name`, `name_len`) must be valid UTF-8 bytes.
        pub fn _span_start(name: *const u8, name_len: usize) -> u32;

        /// End a timing span.
        ///
        /// The `span_id` must be the result of a call to `_span_start`.
        /// The duration between the two calls will be computed and printed to the module's logs.
        ///
        /// Behavior is unspecified
        /// if `_span_end` is called on a `span_id` which is not the result of a call to `_span_start`,
        /// or if `_span_end` is called multiple times with the same `span_id`.
        pub fn _span_end(span_id: u32);
    }

    /// What strategy does the database index use?
    ///
    /// See also: https://www.postgresql.org/docs/current/sql-createindex.html
    #[repr(u8)]
    #[non_exhaustive]
    pub enum IndexType {
        /// Indexing works by putting the index key into a b-tree.
        BTree = 0,
        /// Indexing works by hashing the index key.
        Hash = 1,
    }

    /// The error log level. See [`_console_log`].
    pub const LOG_LEVEL_ERROR: u8 = 0;
    /// The warn log level. See [`_console_log`].
    pub const LOG_LEVEL_WARN: u8 = 1;
    /// The info log level. See [`_console_log`].
    pub const LOG_LEVEL_INFO: u8 = 2;
    /// The debug log level. See [`_console_log`].
    pub const LOG_LEVEL_DEBUG: u8 = 3;
    /// The trace log level. See [`_console_log`].
    pub const LOG_LEVEL_TRACE: u8 = 4;
    /// The panic log level. See [`_console_log`].
    ///
    /// A panic level is emitted just before a fatal error causes the WASM module to trap.
    pub const LOG_LEVEL_PANIC: u8 = 101;

    /// A handle into a buffer of bytes in the host environment that can be read from.
    ///
    /// Used for transporting bytes from host to WASM linear memory.
    #[derive(PartialEq, Eq, Copy, Clone)]
    #[repr(transparent)]
    pub struct BytesSource(u32);

    impl BytesSource {
        /// An invalid handle, used e.g., when the reducer arguments were empty.
        pub const INVALID: Self = Self(0);
    }

    /// A handle into a buffer of bytes in the host environment that can be written to.
    ///
    /// Used for transporting bytes from WASM linear memory to host.
    #[derive(PartialEq, Eq, Copy, Clone)]
    #[repr(transparent)]
    pub struct BytesSink(u32);

    /// Represents table iterators.
    #[derive(PartialEq, Eq, Copy, Clone)]
    #[repr(transparent)]
    pub struct RowIter(u32);

    impl RowIter {
        /// An invalid handle, used e.g., when the iterator has been exhausted.
        pub const INVALID: Self = Self(0);
    }

    #[cfg(any())]
    mod module_exports {
        type Encoded<T> = Buffer;
        type Identity = Encoded<[u8; 32]>;
        /// microseconds since the unix epoch
        type Timestamp = u64;
        /// Buffer::INVALID => Ok(()); else errmsg => Err(errmsg)
        type Result = Buffer;
        extern "C" {
            /// All functions prefixed with `__preinit__` are run first in alphabetical order.
            /// For those it's recommended to use /etc/xxxx.d conventions of like `__preinit__20_do_thing`:
            /// <https://man7.org/linux/man-pages/man5/sysctl.d.5.html#CONFIGURATION_DIRECTORIES_AND_PRECEDENCE>
            fn __preinit__XX_XXXX();
            /// Optional. Run after `__preinit__`; can return an error. Intended for dynamic languages; this
            /// would be where you would initialize the interepreter and load the user module into it.
            fn __setup__() -> Result;
            /// Required. Runs after `__setup__`; returns all the exports for the module.
            fn __describe_module__() -> Encoded<ModuleDef>;
            /// Required. id is an index into the `ModuleDef.reducers` returned from `__describe_module__`.
            /// args is a bsatn-encoded product value defined by the schema at `reducers[id]`.
            fn __call_reducer__(
                id: usize,
                sender_0: u64,
                sender_1: u64,
                sender_2: u64,
                sender_3: u64,
                address_0: u64,
                address_1: u64,
                timestamp: u64,
                args: Buffer,
            ) -> Result;
            /// Currently unused?
            fn __migrate_database__XXXX(sender: Identity, timestamp: Timestamp, something: Buffer) -> Result;
        }
    }
}

/// Error values used in the safe bindings API.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Errno(NonZeroU16);

// once Error gets exposed from core this crate can be no_std again
impl std::error::Error for Errno {}

macro_rules! def_errno {
    ($($err_name:ident($errno:literal, $errmsg:literal),)*) => {
        impl Errno {
            $(#[doc = $errmsg] pub const $err_name: Errno = Errno(errno::$err_name);)*
        }
    };
}
errnos!(def_errno);

impl Errno {
    /// Returns a description of the errno value, if any.
    pub const fn message(self) -> Option<&'static str> {
        errno::strerror(self.0)
    }

    /// Converts the given `code` to an error number in `Errno`'s representation.
    #[inline]
    pub const fn from_code(code: u16) -> Option<Self> {
        match NonZeroU16::new(code) {
            Some(code) => Some(Errno(code)),
            None => None,
        }
    }

    /// Converts this `errno` into a primitive error code.
    #[inline]
    pub const fn code(self) -> u16 {
        self.0.get()
    }
}

impl fmt::Debug for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = f.debug_struct("Errno");
        fmt.field("code", &self.code());
        if let Some(msg) = self.message() {
            fmt.field("message", &msg);
        }
        fmt.finish()
    }
}

impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self.message().unwrap_or("Unknown error");
        write!(f, "{message} (error {})", self.code())
    }
}

/// Convert the status value `x` into a result.
/// When `x = 0`, we have a success status.
fn cvt(x: u16) -> Result<(), Errno> {
    match Errno::from_code(x) {
        None => Ok(()),
        Some(err) => Err(err),
    }
}

/// Runs the given function `f` provided with an uninitialized `out` pointer.
///
/// Assuming the call to `f` succeeds (`Ok(_)`), the `out` pointer's value is returned.
///
/// # Safety
///
/// This function is safe to call, if and only if,
/// - The function `f` writes a safe and valid `T` to the `out` pointer.
///   It's not required to write to `out` when `f(out)` returns an error code.
/// - The function `f` never reads a safe and valid `T` from the `out` pointer
///   before writing a safe and valid `T` to it.
#[inline]
unsafe fn call<T: Copy>(f: impl FnOnce(*mut T) -> u16) -> Result<T, Errno> {
    let mut out = MaybeUninit::uninit();
    let f_code = f(out.as_mut_ptr());
    cvt(f_code)?;
    Ok(out.assume_init())
}

/// Queries and returns the `table_id` associated with the given (table) `name`.
///
/// Returns an error if the table does not exist.
#[inline]
pub fn table_id_from_name(name: &str) -> Result<TableId, Errno> {
    unsafe { call(|out| raw::_table_id_from_name(name.as_ptr(), name.len(), out)) }
}

/// Queries and returns the number of rows in the table identified by `table_id`.
///
/// Returns an error if the table does not exist or if not in a transaction.
#[inline]
pub fn datastore_table_row_count(table_id: TableId) -> Result<u64, Errno> {
    unsafe { call(|out| raw::_datastore_table_row_count(table_id, out)) }
}

/// Finds all rows in the table identified by `table_id`,
/// where the row has a column, identified by `col_id`,
/// with data matching the byte string `val`.
///
/// Matching is defined BSATN-decoding `val` to an `AlgebraicValue`
/// according to the column's schema and then `Ord for AlgebraicValue`.
///
/// The rows found are BSATN encoded and then concatenated.
/// The resulting byte string from the concatenation is written
/// to a fresh buffer with a handle to it returned as a `Buffer`.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - `col_id` does not identify a column of the table
/// - `val` cannot be BSATN-decoded to an `AlgebraicValue`
///   typed at the `AlgebraicType` of the column
#[inline]
pub fn iter_by_col_eq(table_id: TableId, col_id: ColId, val: &[u8]) -> Result<RowIter, Errno> {
    let raw = unsafe { call(|out| raw::_iter_by_col_eq(table_id, col_id, val.as_ptr(), val.len(), out)) }?;
    Ok(RowIter { raw })
}

/// Inserts a row into the table identified by `table_id`,
/// where the row is a BSATN-encoded `ProductValue`
/// matching the table's `ProductType` row-schema.
///
/// The `row` is `&mut` due to auto-incrementing columns.
/// So `row` is written to with the inserted row re-encoded.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - there were unique constraint violations
/// - `row` doesn't decode from BSATN to a `ProductValue`
///   according to the `ProductType` that the table's schema specifies.
#[inline]
pub fn insert(table_id: TableId, row: &mut [u8]) -> Result<(), Errno> {
    cvt(unsafe { raw::_insert(table_id, row.as_mut_ptr(), row.len()) })
}

/// Deletes all rows in the table identified by `table_id`
/// where the column identified by `col_id` matches `value`.
///
/// Matching is defined by BSATN-decoding `value` to an `AlgebraicValue`
/// according to the column's schema and then `Ord for AlgebraicValue`.
///
/// Returns the number of rows deleted.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - no columns were deleted
/// - `col_id` does not identify a column of the table
#[inline]
pub fn delete_by_col_eq(table_id: TableId, col_id: ColId, value: &[u8]) -> Result<u32, Errno> {
    unsafe { call(|out| raw::_delete_by_col_eq(table_id, col_id, value.as_ptr(), value.len(), out)) }
}

/// Deletes those rows, in the table identified by `table_id`,
/// that match any row in `relation`.
///
/// Matching is defined by first BSATN-decoding
/// the byte string pointed to at by `relation` to a `Vec<ProductValue>`
/// according to the row schema of the table
/// and then using `Ord for AlgebraicValue`.
///
/// Returns the number of rows deleted.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - `(relation, relation_len)` doesn't decode from BSATN to a `Vec<ProductValue>`
#[inline]
pub fn delete_by_rel(table_id: TableId, relation: &[u8]) -> Result<u32, Errno> {
    unsafe { call(|out| raw::_delete_by_rel(table_id, relation.as_ptr(), relation.len(), out)) }
}

/// Starts iteration on each row, as BSATN-encoded, of a table identified by `table_id`.
/// Returns iterator handle is written to the `out` pointer.
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
pub fn datastore_table_scan_bsatn(table_id: TableId) -> Result<RowIter, Errno> {
    let raw = unsafe { call(|out| raw::_datastore_table_scan_bsatn(table_id, out))? };
    Ok(RowIter { raw })
}

/// Iterate through a table, filtering by an encoded `spacetimedb_lib::filter::Expr`.
///
/// # Errors
///
/// - `NO_SUCH_TABLE`, if `table_id` doesn't exist.
#[inline]
pub fn iter_filtered(table_id: TableId, filter: &[u8]) -> Result<RowIter, Errno> {
    let raw = unsafe { call(|out| raw::_iter_start_filtered(table_id, filter.as_ptr(), filter.len(), out))? };
    Ok(RowIter { raw })
}

/// A log level that can be used in `console_log`.
/// The variants are convertible into a raw `u8` log level.
#[repr(u8)]
pub enum LogLevel {
    /// The error log level. See [`console_log`].
    Error = raw::LOG_LEVEL_ERROR,
    /// The warn log level. See [`console_log`].
    Warn = raw::LOG_LEVEL_WARN,
    /// The info log level. See [`console_log`].
    Info = raw::LOG_LEVEL_INFO,
    /// The debug log level. See [`console_log`].
    Debug = raw::LOG_LEVEL_DEBUG,
    /// The trace log level. See [`console_log`].
    Trace = raw::LOG_LEVEL_TRACE,
    /// The panic log level. See [`console_log`].
    ///
    /// A panic level is emitted just before a fatal error causes the WASM module to trap.
    Panic = raw::LOG_LEVEL_PANIC,
}

/// Log at `level` a `text` message occuring in `filename:line_number`
/// with [`target`] being the module path at the `log!` invocation site.
///
/// [`target`]: https://docs.rs/log/latest/log/struct.Record.html#method.target
#[inline]
pub fn console_log(
    level: LogLevel,
    target: Option<&str>,
    filename: Option<&str>,
    line_number: Option<u32>,
    text: &str,
) {
    let opt_ptr = |b: Option<&str>| b.map_or(ptr::null(), |b| b.as_ptr());
    let opt_len = |b: Option<&str>| b.map_or(0, |b| b.len());
    unsafe {
        raw::_console_log(
            level as u8,
            opt_ptr(target),
            opt_len(target),
            opt_ptr(filename),
            opt_len(filename),
            line_number.unwrap_or(u32::MAX),
            text.as_ptr(),
            text.len(),
        )
    }
}

/// Schedule a reducer to be called asynchronously, nonatomically, and immediately
/// on a best-effort basis.
///
/// The reducer is assigned `name` and is provided `args` as its argument.
#[cfg(feature = "unstable_abi")]
#[inline]
pub fn volatile_nonatomic_schedule_immediate(name: &str, args: &[u8]) {
    unsafe { raw::_volatile_nonatomic_schedule_immediate(name.as_ptr(), name.len(), args.as_ptr(), args.len()) }
}

pub struct RowIter {
    raw: raw::RowIter,
}

impl RowIter {
    /// Read some number of BSATN-encoded rows into the provided buffer.
    ///
    /// Returns the number of new bytes added to the end of the buffer.
    /// When the iterator has been exhausted,
    /// `self.is_exhausted()` will return `true`.
    pub fn read(&mut self, buf: &mut Vec<u8>) -> usize {
        loop {
            let buf_ptr = buf.spare_capacity_mut();
            let mut buf_len = buf_ptr.len();
            let ret = unsafe { raw::_row_iter_bsatn_advance(self.raw, buf_ptr.as_mut_ptr().cast(), &mut buf_len) };
            if let -1 | 0 = ret {
                // SAFETY: `_row_iter_bsatn_advance` just wrote `buf_len` bytes into the end of `buf`.
                unsafe { buf.set_len(buf.len() + buf_len) };
            }

            const TOO_SMALL: i16 = errno::BUFFER_TOO_SMALL.get() as i16;
            match ret {
                -1 => {
                    self.raw = raw::RowIter::INVALID;
                    return buf_len;
                }
                0 => return buf_len,
                TOO_SMALL => buf.reserve(buf_len),
                e => panic!("unexpected error from `_row_iter_bsatn_advance`: {e}"),
            }
        }
    }

    /// Returns whether the iterator is exhausted or not.
    pub fn is_exhausted(&self) -> bool {
        self.raw == raw::RowIter::INVALID
    }
}

impl Drop for RowIter {
    fn drop(&mut self) {
        // Avoid this syscall when `_row_iter_bsatn_advance` above
        // notifies us that the iterator is exhausted.
        if self.is_exhausted() {
            return;
        }
        unsafe {
            raw::_row_iter_bsatn_close(self.raw);
        }
    }
}
