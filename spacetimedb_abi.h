#ifndef SPACETIMEDB_ABI_H
#define SPACETIMEDB_ABI_H

#include <cstdint> // For uint8_t, uint32_t, uint64_t, etc.
#include <cstddef> // For size_t

// Type Definitions
typedef uint32_t Buffer;
typedef uint32_t BufferIter;

extern "C" {

// Logging
__attribute__((import_module("spacetime"), import_name("_console_log")))
void _console_log(
    uint8_t level,
    const uint8_t *target_ptr,
    size_t target_len,
    const uint8_t *message_ptr,
    size_t message_len);

// Buffer allocation and management
__attribute__((import_module("spacetime"), import_name("_buffer_alloc")))
Buffer _buffer_alloc(size_t size);

__attribute__((import_module("spacetime"), import_name("_buffer_free")))
void _buffer_free(Buffer buffer_id);

__attribute__((import_module("spacetime"), import_name("_buffer_len")))
size_t _buffer_len(Buffer buffer_id);

__attribute__((import_module("spacetime"), import_name("_buffer_get_ptr")))
uint8_t* _buffer_get_ptr(Buffer buffer_id);

__attribute__((import_module("spacetime"), import_name("_buffer_copy_to_host")))
void _buffer_copy_to_host(Buffer buffer_id, uint8_t* host_ptr, size_t len);

__attribute__((import_module("spacetime"), import_name("_buffer_copy_from_host")))
void _buffer_copy_from_host(Buffer buffer_id, const uint8_t* host_ptr, size_t len);


// Database operations
__attribute__((import_module("spacetime"), import_name("_db_insert_row")))
int32_t _db_insert_row(
    const uint8_t *table_name_ptr,
    size_t table_name_len,
    Buffer row_data_buffer_id);

__attribute__((import_module("spacetime"), import_name("_db_update_row")))
int32_t _db_update_row(
    const uint8_t *table_name_ptr,
    size_t table_name_len,
    Buffer old_row_data_buffer_id,
    Buffer new_row_data_buffer_id);

__attribute__((import_module("spacetime"), import_name("_db_delete_row")))
int32_t _db_delete_row(
    const uint8_t *table_name_ptr,
    size_t table_name_len,
    Buffer row_data_buffer_id);

__attribute__((import_module("spacetime"), import_name("_db_query_row")))
Buffer _db_query_row(
    const uint8_t *table_name_ptr,
    size_t table_name_len,
    Buffer query_data_buffer_id);

__attribute__((import_module("spacetime"), import_name("_db_query_table")))
BufferIter _db_query_table(
    const uint8_t* table_name_ptr,
    size_t table_name_len,
    Buffer query_data_buffer_id);


// Buffer Iterator operations
__attribute__((import_module("spacetime"), import_name("_iter_next")))
Buffer _iter_next(BufferIter iter_id);

__attribute__((import_module("spacetime"), import_name("_iter_free")))
void _iter_free(BufferIter iter_id);


// SpacetimeDB specific operations (examples)
__attribute__((import_module("spacetime"), import_name("_commit")))
int32_t _commit(void);

__attribute__((import_module("spacetime"), import_name("_register_reducer")))
void _register_reducer(
    const uint8_t* name_ptr,
    size_t name_len,
    uint32_t reducer_func_idx // Assuming reducers are referenced by an index in the wasm table
);

__attribute__((import_module("spacetime"), import_name("_get_identity")))
Buffer _get_identity(void);

__attribute__((import_module("spacetime"), import_name("_get_transaction")))
Buffer _get_transaction(void);

__attribute__((import_module("spacetime"), import_name("_get_timestamp")))
uint64_t _get_timestamp(void);

__attribute__((import_module("spacetime"), import_name("_get_arg_buffer")))
Buffer _get_arg_buffer(void);

__attribute__((import_module("spacetime"), import_name("_set_return_buffer")))
void _set_return_buffer(Buffer buffer_id);

// For `_buffer_alloc` in the example which takes initial data
__attribute__((import_module("spacetime"), import_name("_buffer_alloc_with_data")))
Buffer _buffer_alloc_with_data(
    const uint8_t *data_ptr,
    size_t data_len);

} // extern "C"

#endif // SPACETIMEDB_ABI_H
