//! Defines sys calls to interact with SpacetimeDB.
//! This forms an ABI of sorts that modules written in Rust can use.

extern crate alloc;

#[macro_use]
mod errno;

use core::fmt;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;
use std::ptr;

use alloc::boxed::Box;

use spacetimedb_primitives::{ColId, TableId};

/// Provides a raw set of sys calls which abstractions can be built atop of.
pub mod raw {
    use core::mem::ManuallyDrop;
    use spacetimedb_primitives::{ColId, TableId};

    // this module identifier determines the abi version that modules built with this crate depend
    // on. Any non-breaking additions to the abi surface should be put in a new `extern {}` block
    // with a module identifier with a minor version 1 above the previous highest minor version.
    // For breaking changes, all functions should be moved into one new `spacetime_X.0` block.
    #[link(wasm_import_module = "spacetime_7.0")]
    extern "C" {
        /*
        /// Create a table with `name`, a UTF-8 slice in WASM memory lasting `name_len` bytes,
        /// and with the table's `schema` in a slice in WASM memory lasting `schema_len` bytes.
        ///
        /// Writes the table id of the new table into the WASM pointer `out`.
        pub fn _create_table(
            name: *const u8,
            name_len: usize,
            schema: *const u8,
            schema_len: usize,
            out: *mut TableId,
        ) -> u16;
        */

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
        pub fn _get_table_id(name: *const u8, name_len: usize, out: *mut TableId) -> u16;

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
        /// Traps if `index_type == 1` or `col_ids.len() != 1`.
        pub fn _create_index(
            index_name: *const u8,
            index_name_len: usize,
            table_id: TableId,
            index_type: u8,
            col_ids: *const u8,
            col_len: usize,
        ) -> u16;

        /// Finds all rows in the table identified by `table_id`,
        /// where the row has a column, identified by `col_id`,
        /// with data matching the byte string, in WASM memory, pointed to at by `val`.
        ///
        /// Matching is defined BSATN-decoding `val` to an `AlgebraicValue`
        /// according to the column's schema and then `Ord for AlgebraicValue`.
        ///
        /// The rows found are BSATN encoded and then concatenated.
        /// The resulting byte string from the concatenation is written
        /// to a fresh buffer with the buffer's identifier written to the WASM pointer `out`.
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
            out: *mut Buffer,
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

        /// Start iteration on each row, as bytes, of a table identified by `table_id`.
        ///
        /// The iterator is registered in the host environment
        /// under an assigned index which is written to the `out` pointer provided.
        ///
        /// Returns an error if
        /// - a table with the provided `table_id` doesn't exist
        pub fn _iter_start(table_id: TableId, out: *mut BufferIter) -> u16;

        /// Like [`_iter_start`], start iteration on each row,
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
        pub fn _iter_start_filtered(
            table_id: TableId,
            filter: *const u8,
            filter_len: usize,
            out: *mut BufferIter,
        ) -> u16;

        /// Advances the registered iterator with the index given by `iter_key`.
        ///
        /// On success, the next element (the row as bytes) is written to a buffer.
        /// The buffer's index is returned and written to the `out` pointer.
        /// If there are no elements left, an invalid buffer index is written to `out`.
        /// On failure however, the error is returned.
        ///
        /// Returns an error if
        /// - `iter` does not identify a registered `BufferIter`
        /// - writing to `out` would overflow a 64-bit integer
        /// - advancing the iterator resulted in an error
        pub fn _iter_next(iter: ManuallyDrop<BufferIter>, out: *mut Buffer) -> u16;

        /// Drops the entire registered iterator with the index given by `iter_key`.
        /// The iterator is effectively de-registered.
        ///
        /// Returns an error if the iterator does not exist.
        pub fn _iter_drop(iter: ManuallyDrop<BufferIter>) -> u16;

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

        /// Schedules a reducer to be called asynchronously at `time`.
        ///
        /// The reducer is named as the valid UTF-8 slice `(name, name_len)`,
        /// and is passed the slice `(args, args_len)` as its argument.
        ///
        /// A generated schedule id is assigned to the reducer.
        /// This id is written to the pointer `out`.
        ///
        /// Traps if
        /// - the `time` delay exceeds `64^6 - 1` milliseconds from now
        /// - `name` does not point to valid UTF-8
        /// - `name + name_len` or `args + args_len` overflow a 64-bit integer
        pub fn _schedule_reducer(
            name: *const u8,
            name_len: usize,
            args: *const u8,
            args_len: usize,
            time: u64,
            out: *mut u64,
        );

        /// Unschedule a reducer using the same `id` generated as when it was scheduled.
        ///
        /// This assumes that the reducer hasn't already been executed.
        pub fn _cancel_reducer(id: u64);

        /// Returns the length (number of bytes) of buffer `bufh` without
        /// transferring ownership of the data into the function.
        ///
        /// The `bufh` must have previously been allocating using `_buffer_alloc`.
        ///
        /// Traps if the buffer does not exist.
        pub fn _buffer_len(bufh: ManuallyDrop<Buffer>) -> usize;

        /// Consumes the `buffer`,
        /// moving its contents to the slice `(dst, dst_len)`.
        ///
        /// Traps if
        /// - the buffer does not exist
        /// - `dst + dst_len` overflows a 64-bit integer
        pub fn _buffer_consume(buffer: Buffer, dst: *mut u8, dst_len: usize);

        /// Creates a buffer of size `data_len` in the host environment.
        ///
        /// The contents of the byte slice pointed to by `data`
        /// and lasting `data_len` bytes
        /// is written into the newly initialized buffer.
        ///
        /// The buffer is registered in the host environment and is indexed by the returned `u32`.
        ///
        /// Traps if `data + data_len` overflows a 64-bit integer.
        pub fn _buffer_alloc(data: *const u8, data_len: usize) -> Buffer;

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

    /// A handle into a buffer of bytes in the host environment.
    ///
    /// Used for transporting bytes host <-> WASM linear memory.
    #[repr(transparent)]
    pub struct Buffer {
        /// The actual handle. A key into a `ResourceSlab`.
        raw: u32,
    }

    impl Buffer {
        /// Returns a "handle" that can be passed across the FFI boundary
        /// as if it was the Buffer itself, but without consuming it.
        pub const fn handle(&self) -> ManuallyDrop<Self> {
            ManuallyDrop::new(Self { raw: self.raw })
        }

        /// An invalid buffer handle.
        ///
        /// Could happen if too many buffers exist, making the key overflow a `u32`.
        /// `INVALID` is also used for parts of the protocol
        /// that are "morally" sending a `None`s in `Option<Box<[u8]>>`s.
        pub const INVALID: Self = Self { raw: u32::MAX };

        /// Is the buffer handle invalid?
        pub const fn is_invalid(&self) -> bool {
            self.raw == Self::INVALID.raw
        }
    }

    /// Represents table iterators, with a similar API to [`Buffer`].
    #[repr(transparent)]
    pub struct BufferIter {
        /// The actual handle. A key into a `ResourceSlab`.
        raw: u32,
    }

    impl BufferIter {
        /// Returns a handle usable for non-consuming operations.
        pub const fn handle(&self) -> ManuallyDrop<Self> {
            ManuallyDrop::new(Self { raw: self.raw })
        }
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
            fn __call_reducer__(id: usize, sender: Identity, timestamp: Timestamp, args: Buffer) -> Result;
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
    ($($name:ident => $desc:literal,)*) => {
        impl Errno {
            // SAFETY: We've checked that `errnos!` contains no `0` values.
            $(#[doc = $desc] pub const $name: Errno = Errno(unsafe { NonZeroU16::new_unchecked(errno::$name) });)*
        }

        /// Returns a string representation of the error.
        const fn strerror(err: Errno) -> Option<&'static str> {
            match err {
                $(Errno::$name => Some($desc),)*
                _ => None,
            }
        }
    };
}
errnos!(def_errno);

impl Errno {
    /// Returns a description of the errno value, if any.
    pub const fn message(self) -> Option<&'static str> {
        strerror(self)
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
/// - If running `Drop` on `T` is required for safety,
///   `f` must never panic nor return an error once `out` has been written to.
#[inline]
unsafe fn call<T>(f: impl FnOnce(*mut T) -> u16) -> Result<T, Errno> {
    let mut out = MaybeUninit::uninit();
    // TODO: If we have a panic here after writing a safe `T` to `out`,
    // we will may have a memory leak if `T` requires running `Drop` for cleanup.
    let f_code = f(out.as_mut_ptr());
    // TODO: A memory leak may also result due to an error code from `f(out)`
    // if `out` has been written to.
    cvt(f_code)?;
    Ok(out.assume_init())
}

/// Queries and returns the `table_id` associated with the given (table) `name`.
///
/// Returns an error if the table does not exist.
#[inline]
pub fn get_table_id(name: &str) -> Result<TableId, Errno> {
    unsafe { call(|out| raw::_get_table_id(name.as_ptr(), name.len(), out)) }
}

/// Creates an index with the name `index_name` and type `index_type`,
/// on a product of the given columns ids in `col_ids`,
/// identifying columns in the table identified by `table_id`.
///
/// Currently only single-column-indices are supported
/// and they may only be of the btree index type.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - `index_type > 1`
///
/// Traps if `index_type == 1` or `col_ids.len() != 1`.
#[inline]
pub fn create_index(index_name: &str, table_id: TableId, index_type: u8, col_ids: &[u8]) -> Result<(), Errno> {
    cvt(unsafe {
        raw::_create_index(
            index_name.as_ptr(),
            index_name.len(),
            table_id,
            index_type,
            col_ids.as_ptr(),
            col_ids.len(),
        )
    })
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
pub fn iter_by_col_eq(table_id: TableId, col_id: ColId, val: &[u8]) -> Result<Buffer, Errno> {
    unsafe { call(|out| raw::_iter_by_col_eq(table_id, col_id, val.as_ptr(), val.len(), out)) }
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

/// Returns an iterator for each row, as bytes, of a table identified by `table_id`.
/// The rows can be put through an optional `filter`,
/// which is encoded in the embedded language defined by `spacetimedb_lib::filter::Expr`.
///
/// The actual return value is a handle to an iterator registered with the host environment,
/// but [`BufferIter`] can be used directly as an `Iterator`.
///
/// Returns an error if
///
/// - a table with the provided `table_id` doesn't exist
/// - `Some(filter)` doesn't decode to a filter expression
#[inline]
pub fn iter(table_id: TableId, filter: Option<&[u8]>) -> Result<BufferIter, Errno> {
    unsafe {
        call(|out| match filter {
            None => raw::_iter_start(table_id, out),
            Some(filter) => raw::_iter_start_filtered(table_id, filter.as_ptr(), filter.len(), out),
        })
    }
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

/// Schedule a reducer to be called asynchronously at `time`.
///
/// The reducer is assigned `name` and is provided `args` as its argument.
///
/// A generated schedule id is assigned to the reducer which is returned.
///
/// Returns an error if the `time` delay exceeds `64^6 - 1` milliseconds from now.
///
/// TODO: not fully implemented yet
/// TODO(Centril): Unsure what is unimplemented; perhaps it refers to a new
///   implementation with a special system table rather than a special sys call.
#[inline]
pub fn schedule(name: &str, args: &[u8], time: u64) -> u64 {
    let mut out = 0;
    unsafe { raw::_schedule_reducer(name.as_ptr(), name.len(), args.as_ptr(), args.len(), time, &mut out) }
    out
}

/// Unschedule a reducer using the same `id` generated as when it was scheduled.
///
/// This assumes that the reducer hasn't already been executed.
pub fn cancel_reducer(id: u64) {
    unsafe { raw::_cancel_reducer(id) }
}

pub use raw::{Buffer, BufferIter};

impl Buffer {
    /// Returns the number of bytes of the data stored in the buffer.
    pub fn data_len(&self) -> usize {
        unsafe { raw::_buffer_len(self.handle()) }
    }

    /// Read the contents of the buffer into the provided Vec.
    /// The Vec is cleared in the process.
    pub fn read_into(self, buf: &mut Vec<u8>) {
        let data_len = self.data_len();
        buf.clear();
        buf.reserve(data_len);
        self.read_uninit(&mut buf.spare_capacity_mut()[..data_len]);
        // SAFETY: We just wrote `data_len` bytes into `buf`.
        unsafe { buf.set_len(data_len) };
    }

    /// Read the contents of the buffer into a new boxed byte slice.
    pub fn read(self) -> Box<[u8]> {
        let mut buf = alloc::vec::Vec::new();
        self.read_into(&mut buf);
        buf.into_boxed_slice()
    }

    /// Read the contents of the buffer into an array of fixed size `N`.
    ///
    /// If the length is wrong, the module will crash.
    pub fn read_array<const N: usize>(self) -> [u8; N] {
        // use MaybeUninit::uninit_array once stable
        let mut arr = unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() };
        self.read_uninit(&mut arr);
        // use MaybeUninit::array_assume_init once stable
        unsafe { (&arr as *const [_; N]).cast::<[u8; N]>().read() }
    }

    /// Reads the buffer into an uninitialized byte string `buf`.
    ///
    /// The module will crash if `buf`'s length doesn't match the buffer.
    pub fn read_uninit(self, buf: &mut [MaybeUninit<u8>]) {
        unsafe { raw::_buffer_consume(self, buf.as_mut_ptr().cast(), buf.len()) }
    }

    /// Allocates a buffer with the contents of `data`.
    pub fn alloc(data: &[u8]) -> Self {
        unsafe { raw::_buffer_alloc(data.as_ptr(), data.len()) }
    }
}

impl Iterator for BufferIter {
    type Item = Result<Buffer, Errno>;

    fn next(&mut self) -> Option<Self::Item> {
        let buf = unsafe { call(|out| raw::_iter_next(self.handle(), out)) };
        match buf {
            Ok(buf) if buf.is_invalid() => None,
            res => Some(res),
        }
    }
}

impl Drop for BufferIter {
    fn drop(&mut self) {
        cvt(unsafe { raw::_iter_drop(self.handle()) }).unwrap();
    }
}

// TODO: eventually there should be a way to set a consistent random seed for a module
#[cfg(feature = "getrandom")]
fn fake_random(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    #[allow(clippy::needless_range_loop)]
    for i in 0..buf.len() {
        let start = match i % 4 {
            0 => 0x64,
            1 => 0xe9,
            2 => 0x48,
            _ => 0xb5,
        };
        buf[i] = (start ^ i) as u8;
    }

    Result::Ok(())
}
#[cfg(feature = "getrandom")]
getrandom::register_custom_getrandom!(fake_random);
