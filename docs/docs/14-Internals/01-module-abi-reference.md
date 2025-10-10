---
slug: /webassembly-abi
---

# Module ABI Reference

This document specifies the _low level details_ of module-host interactions (_"Module ABI"_). _**Most users**_ looking to interact with the host will want to use derived and higher level functionality like [`bindings`], `#[spacetimedb(table)]`, and `#[derive(SpacetimeType)]` rather than this low level ABI. For more on those, read the [Rust module quick start][module_quick_start] guide and the [Rust module reference][module_ref].

The Module ABI is defined in [`bindings_sys::raw`] and is used by modules to interact with their host and perform various operations like:

- logging,
- transporting data,
- scheduling reducers,
- altering tables,
- inserting and deleting rows,
- querying tables.

In the next few sections, we'll define the functions that make up the ABI and what these functions do.

## General notes

The functions in this ABI all use the [`C` ABI on the `wasm32` platform][wasm_c_abi]. They are specified in a Rust `extern "C" { .. }` block. For those more familiar with the `C` notation, an [appendix][c_header] is provided with equivalent definitions as would occur in a `.h` file.

Many functions in the ABI take in- or out-pointers, e.g. `*const u8` and `*mut u8`. The WASM host itself does not have undefined behavior. However, what WASM does not consider a memory access violation could be one according to some other language's abstract machine. For example, running the following on a WASM host would violate Rust's rules around writing across allocations:

```rust
fn main() {
    let mut bytes = [0u8; 12];
    let other_bytes = [0u8; 4];
    unsafe { ffi_func_with_out_ptr_and_len(&mut bytes as *mut u8, 16); }
    assert_eq!(other_bytes, [0u8; 4]);
}
```

When we note in this reference that traps occur or errors are returned on memory access violations, we only mean those that WASM can directly detected, and not cases like the one above.

Should memory access violations occur, such as a buffer overrun, undefined behavior will never result, as it does not exist in WASM. However, in many cases, an error code will result.

Some functions will treat UTF-8 strings _lossily_. That is, if the slice identified by a `(ptr, len)` contains non-UTF-8 bytes, these bytes will be replaced with `�` in the read string.

Most functions return a `u16` value. This is how these functions indicate an error where a `0` value means that there were no errors. Such functions will instead return any data they need to through out pointers.

## Logging

```rust
/// The error log level.
const LOG_LEVEL_ERROR: u8 = 0;
/// The warn log level.
const LOG_LEVEL_WARN: u8 = 1;
/// The info log level.
const LOG_LEVEL_INFO: u8 = 2;
/// The debug log level.
const LOG_LEVEL_DEBUG: u8 = 3;
/// The trace log level.
const LOG_LEVEL_TRACE: u8 = 4;
/// The panic log level.
///
/// A panic level is emitted just before
/// a fatal error causes the WASM module to trap.
const LOG_LEVEL_PANIC: u8 = 101;

/// Log at `level` a `text` message occuring in `filename:line_number`
/// with `target` being the module path at the `log!` invocation site.
///
/// These various pointers are interpreted lossily as UTF-8 strings.
/// The data pointed to are copied. Ownership does not transfer.
///
/// See https://docs.rs/log/latest/log/struct.Record.html#method.target
/// for more info on `target`.
///
/// Calls to the function cannot fail
/// irrespective of memory access violations.
/// If they occur, no message is logged.
fn _console_log(
    // The level we're logging at.
    // One of the `LOG_*` constants above.
    level: u8,
    // The module path, if any, associated with the message
    // or to "blame" for the reason we're logging.
    //
    // This is a pointer to a buffer holding an UTF-8 encoded string.
    // When the pointer is `NULL`, `target` is ignored.
    target: *const u8,
    // The length of the buffer pointed to by `text`.
    // Unused when `target` is `NULL`.
    target_len: usize,
    // The file name, if any, associated with the message
    // or to "blame" for the reason we're logging.
    //
    // This is a pointer to a buffer holding an UTF-8 encoded string.
    // When the pointer is `NULL`, `filename` is ignored.
    filename: *const u8,
    // The length of the buffer pointed to by `text`.
    // Unused when `filename` is `NULL`.
    filename_len: usize,
    // The line number associated with the message
    // or to "blame" for the reason we're logging.
    line_number: u32,
    // A pointer to a buffer holding an UTF-8 encoded message to log.
    text: *const u8,
    // The length of the buffer pointed to by `text`.
    text_len: usize,
);
```

## Buffer handling

```rust
/// Returns the length of buffer `bufh` without
/// transferring ownership of the data into the function.
///
/// The `bufh` must have previously been allocating using `_buffer_alloc`.
///
/// Traps if the buffer does not exist.
fn _buffer_len(
    // The buffer previously allocated using `_buffer_alloc`.
    // Ownership of the buffer is not taken.
    bufh: ManuallyDrop<Buffer>
) -> usize;

/// Consumes the buffer `bufh`,
/// moving its contents to the WASM byte slice `(ptr, len)`.
///
/// Returns an error if the buffer does not exist
/// or on any memory access violations associated with `(ptr, len)`.
fn _buffer_consume(
    // The buffer to consume and move into `(ptr, len)`.
    // Ownership of the buffer and its contents are taken.
    // That is, `bufh` won't be usable after this call.
    bufh: Buffer,
    // A WASM out pointer to write the contents of `bufh` to.
    ptr: *mut u8,
    // The size of the buffer pointed to by `ptr`.
    // This size must match that of `bufh` or a trap will occur.
    len: usize
);

/// Creates a buffer of size `data_len` in the host environment.
///
/// The contents of the byte slice lasting `data_len` bytes
/// at the `data` WASM pointer are read
/// and written into the newly initialized buffer.
///
/// Traps on any memory access violations.
fn _buffer_alloc(data: *const u8, data_len: usize) -> Buffer;
```

## Reducer scheduling

```rust
/// Schedules a reducer to be called asynchronously at `time`.
///
/// The reducer is named as the valid UTF-8 slice `(name, name_len)`,
/// and is passed the slice `(args, args_len)` as its argument.
///
/// A generated schedule id is assigned to the reducer.
/// This id is written to the pointer `out`.
///
/// Errors on any memory access violations,
/// if `(name, name_len)` does not point to valid UTF-8,
/// or if the `time` delay exceeds `64^6 - 1` milliseconds from now.
fn _schedule_reducer(
    // A pointer to a buffer
    // with a valid UTF-8 string of `name_len` many bytes.
    name: *const u8,
    // The number of bytes in the `name` buffer.
    name_len: usize,
    // A pointer to a byte buffer of `args_len` many bytes.
    args: *const u8,
    // The number of bytes in the `args` buffer.
    args_len: usize,
    // When to call the reducer.
    time: u64,
    // The schedule ID is written to this out pointer on a successful call.
    out: *mut u64,
);

/// Unschedules a reducer
/// using the same `id` generated as when it was scheduled.
///
/// This assumes that the reducer hasn't already been executed.
fn _cancel_reducer(id: u64);
```

## Altering tables

```rust
/// Creates an index with the name `index_name` and type `index_type`,
/// on a product of the given columns in `col_ids`
/// in the table identified by `table_id`.
///
/// Here `index_name` points to a UTF-8 slice in WASM memory
/// and `col_ids` points to a byte slice in WASM memory
/// with each element being a column.
///
/// Currently only single-column-indices are supported
/// and they may only be of the btree index type.
/// In the former case, the function will panic,
/// and in latter, an error is returned.
///
/// Returns an error on any memory access violations,
/// if `(index_name, index_name_len)` is not valid UTF-8,
/// or when a table with the provided `table_id` doesn't exist.
///
/// Traps if `index_type /= 0` or if `col_len /= 1`.
fn _create_index(
    // A pointer to a buffer holding an UTF-8 encoded index name.
    index_name: *const u8,
    // The length of the buffer pointed to by `index_name`.
    index_name_len: usize,
    // The ID of the table to create the index for.
    table_id: u32,
    // The type of the index.
    // Must be `0` currently, that is, a btree-index.
    index_type: u8,
    // A pointer to a buffer holding a byte slice
    // where each element is the position
    // of a column to include in the index.
    col_ids: *const u8,
    // The length of the byte slice in `col_ids`. Must be `1`.
    col_len: usize,
) -> u16;
```

## Inserting and deleting rows

```rust
/// Inserts a row into the table identified by `table_id`,
/// where the row is read from the byte slice `row_ptr` in WASM memory,
/// lasting `row_len` bytes.
///
/// Errors if there were unique constraint violations,
/// if there were any memory access violations in associated with `row`,
/// if the `table_id` doesn't identify a table,
/// or if `(row, row_len)` doesn't decode from BSATN to a `ProductValue`
/// according to the `ProductType` that the table's schema specifies.
fn _insert(
    // The table to insert the row into.
    // The interpretation of `(row, row_len)` depends on this ID
    // as it's table schema determines how to decode the raw bytes.
    table_id: u32,
    // An in/out pointer to a byte buffer
    // holding the BSATN-encoded `ProductValue` row data to insert.
    //
    // The pointer is written to with the inserted row re-encoded.
    // This is due to auto-incrementing columns.
    row: *mut u8,
    // The length of the buffer pointed to by `row`.
    row_len: usize
) -> u16;

/// Deletes all rows in the table identified by `table_id`
/// where the column identified by `col_id` matches the byte string,
/// in WASM memory, pointed to by `value`.
///
/// Matching is defined by decoding of `value` to an `AlgebraicValue`
/// according to the column's schema and then `Ord for AlgebraicValue`.
///
/// The number of rows deleted is written to the WASM pointer `out`.
///
/// Errors if there were memory access violations
/// associated with `value` or `out`,
/// if no columns were deleted,
/// or if the column wasn't found.
fn _delete_by_col_eq(
    // The table to delete rows from.
    table_id: u32,
    // The position of the column to match `(value, value_len)` against.
    col_id: u32,
    // A pointer to a byte buffer holding a BSATN-encoded `AlgebraicValue`
    // of the `AlgebraicType` that the table's schema specifies
    // for the column identified by `col_id`.
    value: *const u8,
    // The length of the buffer pointed to by `value`.
    value_len: usize,
    // An out pointer that the number of rows deleted is written to.
    out: *mut u32
) -> u16;
```

## Querying tables

```rust
/// Queries the `table_id` associated with the given (table) `name`
/// where `name` points to a UTF-8 slice
/// in WASM memory of `name_len` bytes.
///
/// The table id is written into the `out` pointer.
///
/// Errors on memory access violations associated with `name`
/// or if the table does not exist.
fn _get_table_id(
    // A pointer to a buffer holding the name of the table
    // as a valid UTF-8 encoded string.
    name: *const u8,
    // The length of the buffer pointed to by `name`.
    name_len: usize,
    // An out pointer to write the table ID to.
    out: *mut u32
) -> u16;

/// Finds all rows in the table identified by `table_id`,
/// where the row has a column, identified by `col_id`,
/// with data matching the byte string,
/// in WASM memory, pointed to at by `val`.
///
/// Matching is defined by decoding of `value`
/// to an `AlgebraicValue` according to the column's schema
/// and then `Ord for AlgebraicValue`.
///
/// The rows found are BSATN encoded and then concatenated.
/// The resulting byte string from the concatenation
/// is written to a fresh buffer
/// with the buffer's identifier written to the WASM pointer `out`.
///
/// Errors if no table with `table_id` exists,
/// if `col_id` does not identify a column of the table,
/// if `(value, value_len)` cannot be decoded to an `AlgebraicValue`
/// typed at the `AlgebraicType` of the column,
/// or if memory access violations occurred associated with `value` or `out`.
fn _iter_by_col_eq(
    // Identifies the table to find rows in.
    table_id: u32,
    // The position of the column in the table
    // to match `(value, value_len)` against.
    col_id: u32,
    // A pointer to a byte buffer holding a BSATN encoded
    // value typed at the `AlgebraicType` of the column.
    value: *const u8,
    // The length of the buffer pointed to by `value`.
    value_len: usize,
    // An out pointer to which the new buffer's id is written to.
    out: *mut Buffer
) -> u16;

/// Starts iteration on each row, as bytes,
/// of a table identified by `table_id`.
///
/// The iterator is registered in the host environment
/// under an assigned index which is written to the `out` pointer provided.
///
/// Errors if the table doesn't exist
/// or if memory access violations occurred in association with `out`.
fn _iter_start(
    // The ID of the table to start row iteration on.
    table_id: u32,
    // An out pointer to which an identifier
    // to the newly created buffer is written.
    out: *mut BufferIter
) -> u16;

/// Like [`_iter_start`], starts iteration on each row,
/// as bytes, of a table identified by `table_id`.
///
/// The rows are filtered through `filter`, which is read from WASM memory
/// and is encoded in the embedded language defined by `spacetimedb_lib::filter::Expr`.
///
/// The iterator is registered in the host environment
/// under an assigned index which is written to the `out` pointer provided.
///
/// Errors if `table_id` doesn't identify a table,
/// if `(filter, filter_len)` doesn't decode to a filter expression,
/// or if there were memory access violations
/// in association with `filter` or `out`.
fn _iter_start_filtered(
    // The ID of the table to start row iteration on.
    table_id: u32,
    // A pointer to a buffer holding an encoded filter expression.
    filter: *const u8,
    // The length of the buffer pointed to by `filter`.
    filter_len: usize,
    // An out pointer to which an identifier
    // to the newly created buffer is written.
    out: *mut BufferIter
) -> u16;

/// Advances the registered iterator with the index given by `iter_key`.
///
/// On success, the next element (the row as bytes) is written to a buffer.
/// The buffer's index is returned and written to the `out` pointer.
/// If there are no elements left, an invalid buffer index is written to `out`.
/// On failure however, the error is returned.
///
/// Errors if `iter` does not identify a registered `BufferIter`,
/// or if there were memory access violations in association with `out`.
fn _iter_next(
    // An identifier for the iterator buffer to advance.
    // Ownership of the buffer nor the identifier is moved into the function.
    iter: ManuallyDrop<BufferIter>,
    // An out pointer to write the newly created buffer's identifier to.
    out: *mut Buffer
) -> u16;

/// Drops the entire registered iterator with the index given by `iter_key`.
/// The iterator is effectively de-registered.
///
/// Returns an error if the iterator does not exist.
fn _iter_drop(
    // An identifier for the iterator buffer to unregister / drop.
    iter: ManuallyDrop<BufferIter>
) -> u16;
```

## Appendix, `bindings.h`

```c
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef uint32_t Buffer;
typedef uint32_t BufferIter;

void _console_log(
    uint8_t level,
    const uint8_t *target,
    size_t target_len,
    const uint8_t *filename,
    size_t filename_len,
    uint32_t line_number,
    const uint8_t *text,
    size_t text_len
);


Buffer _buffer_alloc(
    const uint8_t *data,
    size_t data_len
);
void _buffer_consume(
    Buffer bufh,
    uint8_t *into,
    size_t len
);
size_t _buffer_len(Buffer bufh);


void _schedule_reducer(
    const uint8_t *name,
    size_t name_len,
    const uint8_t *args,
    size_t args_len,
    uint64_t time,
    uint64_t *out
);
void _cancel_reducer(uint64_t id);


uint16_t _create_index(
    const uint8_t *index_name,
    size_t index_name_len,
    uint32_t table_id,
    uint8_t index_type,
    const uint8_t *col_ids,
    size_t col_len
);


uint16_t _insert(
    uint32_t table_id,
    uint8_t *row,
    size_t row_len
);
uint16_t _delete_by_col_eq(
    uint32_t table_id,
    uint32_t col_id,
    const uint8_t *value,
    size_t value_len,
    uint32_t *out
);


uint16_t _get_table_id(
    const uint8_t *name,
    size_t name_len,
    uint32_t *out
);
uint16_t _iter_by_col_eq(
    uint32_t table_id,
    uint32_t col_id,
    const uint8_t *value,
    size_t value_len,
    Buffer *out
);
uint16_t _iter_drop(BufferIter iter);
uint16_t _iter_next(BufferIter iter, Buffer *out);
uint16_t _iter_start(uint32_t table_id, BufferIter *out);
uint16_t _iter_start_filtered(
    uint32_t table_id,
    const uint8_t *filter,
    size_t filter_len,
    BufferIter *out
);
```

[`bindings_sys::raw`]: https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/bindings-sys/src/lib.rs#L44-L215
[`bindings`]: https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/bindings/src/lib.rs
[module_ref]: /modules/rust
[module_quick_start]: /modules/rust/quickstart
[wasm_c_abi]: https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md
[c_header]: #appendix-bindingsh
