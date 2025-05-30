#ifndef SPACETIMEDB_ABI_H
#define SPACETIMEDB_ABI_H

#include <cstdint> // For uint8_t, uint32_t, uint64_t, etc.
#include <cstddef> // For size_t

// Type Definitions from bindings.h
typedef uint32_t Buffer;
typedef uint32_t BufferIter;

// All function declarations must be within an extern "C" block
extern "C" {

// Logging
// As per docs: "Calls to the function cannot fail irrespective of memory access violations." -> void return
__attribute__((import_module("spacetime"), import_name("_console_log")))
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

// Buffer handling
// _buffer_alloc: Returns Buffer directly.
__attribute__((import_module("spacetime"), import_name("_buffer_alloc")))
Buffer _buffer_alloc(
    const uint8_t *data,
    size_t data_len
);

// _buffer_consume: Documented as void, but text implies error states.
// Assuming uint16_t for error code consistency with other fallible functions.
// "Returns an error if the buffer does not exist or on any memory access violations associated with (ptr, len)."
__attribute__((import_module("spacetime"), import_name("_buffer_consume")))
uint16_t _buffer_consume(
    Buffer bufh, // Taken by value, implies consumption
    uint8_t *into, // out-parameter for the data
    size_t len    // length of the `into` buffer, must match buffer_len(bufh)
);

// _buffer_len: Returns size_t directly.
// "Traps if the buffer does not exist." -> No error code, direct return or trap.
__attribute__((import_module("spacetime"), import_name("_buffer_len")))
size_t _buffer_len(
    Buffer bufh // Taken by value, but it's a query
);


// Reducer scheduling
// _schedule_reducer: Documented as void, but text implies error states.
// Assuming uint16_t for error code consistency.
// "Errors on any memory access violations, if ... does not point to valid UTF-8, or if the time delay exceeds..."
__attribute__((import_module("spacetime"), import_name("_schedule_reducer")))
uint16_t _schedule_reducer(
    const uint8_t *name,
    size_t name_len,
    const uint8_t *args,
    size_t args_len,
    uint64_t time,
    uint64_t *out_schedule_id_ptr // out-parameter
);

// _cancel_reducer: Documented as void, but text implies error states.
// Assuming uint16_t for error code consistency if cancellation can fail (e.g., ID not found).
__attribute__((import_module("spacetime"), import_name("_cancel_reducer")))
uint16_t _cancel_reducer(
    uint64_t id
);


// Altering tables
__attribute__((import_module("spacetime"), import_name("_create_index")))
uint16_t _create_index(
    const uint8_t *index_name,
    size_t index_name_len,
    uint32_t table_id,
    uint8_t index_type,
    const uint8_t *col_ids,
    size_t col_len
);


// Inserting and deleting rows
__attribute__((import_module("spacetime"), import_name("_insert")))
uint16_t _insert(
    uint32_t table_id,
    uint8_t *row_bsatn_ptr, // in-out: host can modify this (e.g., for auto-inc PK)
    size_t row_bsatn_len
);

__attribute__((import_module("spacetime"), import_name("_delete_by_col_eq")))
uint16_t _delete_by_col_eq(
    uint32_t table_id,
    uint32_t col_id,
    const uint8_t *value_bsatn_ptr,
    size_t value_bsatn_len,
    uint32_t *out_deleted_count_ptr // out-parameter
);


// Querying tables
__attribute__((import_module("spacetime"), import_name("_get_table_id")))
uint16_t _get_table_id(
    const uint8_t *name_ptr,
    size_t name_len,
    uint32_t *out_table_id_ptr // out-parameter
);

__attribute__((import_module("spacetime"), import_name("_iter_by_col_eq")))
uint16_t _iter_by_col_eq(
    uint32_t table_id,
    uint32_t col_id,
    const uint8_t *value_bsatn_ptr,
    size_t value_bsatn_len,
    Buffer *out_buffer_ptr_with_rows // out-parameter for the buffer of concatenated rows
);

__attribute__((import_module("spacetime"), import_name("_iter_drop")))
uint16_t _iter_drop(
    BufferIter iter_handle // Taken by value, implies consumption
);

__attribute__((import_module("spacetime"), import_name("_iter_next")))
uint16_t _iter_next(
    BufferIter iter_handle,
    Buffer *out_row_data_buf_ptr // out-parameter for the next row buffer
);

__attribute__((import_module("spacetime"), import_name("_iter_start")))
uint16_t _iter_start(
    uint32_t table_id,
    BufferIter *out_iter_ptr // out-parameter
);

__attribute__((import_module("spacetime"), import_name("_iter_start_filtered")))
uint16_t _iter_start_filtered(
    uint32_t table_id,
    const uint8_t *filter_bsatn_ptr,
    size_t filter_bsatn_len,
    BufferIter *out_iter_ptr // out-parameter
);

} // extern "C"

#endif // SPACETIMEDB_ABI_H
