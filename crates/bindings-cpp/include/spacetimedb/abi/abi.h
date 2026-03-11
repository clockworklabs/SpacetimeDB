#ifndef SPACETIMEDB_ABI_H
#define SPACETIMEDB_ABI_H

#include <cstdint>
#include <cstddef>
#include "opaque_types.h"

/**
 * @file abi.h
 * @brief Raw C ABI interface for SpacetimeDB modules
 * 
 * This file contains the raw C-compatible ABI declarations.
 * These functions use only C-compatible types and are suitable
 * for extern "C" linkage.
 */

// ========================================================================
// SECTION 1: IMPORT DECLARATIONS - Functions provided by SpacetimeDB host
// ========================================================================

// Macro for declaring imported functions from the SpacetimeDB host
// Most stable syscalls are in spacetime_10.0
#define STDB_IMPORT(name) \
    __attribute__((import_module("spacetime_10.0"), import_name(#name))) extern

// BytesSource length query is in spacetime_10.1
#define STDB_IMPORT_10_1(name) \
    __attribute__((import_module("spacetime_10.1"), import_name(#name))) extern

// Procedure-specific syscalls are in spacetime_10.3
#define STDB_IMPORT_10_3(name) \
    __attribute__((import_module("spacetime_10.3"), import_name(#name))) extern

// Point scan delete is in spacetime_10.4
#define STDB_IMPORT_10_4(name) \
    __attribute__((import_module("spacetime_10.4"), import_name(#name))) extern

// Import opaque types into global namespace for C compatibility
using SpacetimeDB::Status;
using SpacetimeDB::TableId;
using SpacetimeDB::IndexId;
using SpacetimeDB::ColId;
using SpacetimeDB::IndexType;
using SpacetimeDB::LogLevel;
using SpacetimeDB::BytesSink;
using SpacetimeDB::BytesSource;
using SpacetimeDB::RowIter;
using SpacetimeDB::ConsoleTimerId;

// Disable warnings about C-linkage with user-defined types
// This is safe because our opaque types are single-field structs
// which have the same ABI as their underlying type
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wreturn-type-c-linkage"

extern "C" {

// ===== Table and Index Management =====
STDB_IMPORT(table_id_from_name)
Status table_id_from_name(const uint8_t* name_ptr, size_t name_len, TableId* out);

STDB_IMPORT(index_id_from_name)
Status index_id_from_name(const uint8_t* name_ptr, size_t name_len, IndexId* out);

// ===== Table Operations =====
STDB_IMPORT(datastore_table_row_count)
Status datastore_table_row_count(TableId table_id, uint64_t* out);

STDB_IMPORT(datastore_table_scan_bsatn)
Status datastore_table_scan_bsatn(TableId table_id, RowIter* out);

// ===== Index Scanning =====
STDB_IMPORT(datastore_index_scan_range_bsatn)
Status datastore_index_scan_range_bsatn(
    IndexId index_id, const uint8_t* prefix_ptr, size_t prefix_len, ColId prefix_elems,
    const uint8_t* rstart_ptr, size_t rstart_len, const uint8_t* rend_ptr, size_t rend_len, 
    RowIter* out);

STDB_IMPORT_10_4(datastore_index_scan_point_bsatn)
Status datastore_index_scan_point_bsatn(
    IndexId index_id, const uint8_t* point_ptr, size_t point_len,
    RowIter* out);

// Deprecated: Use datastore_index_scan_range_bsatn instead
STDB_IMPORT(datastore_btree_scan_bsatn)
Status datastore_btree_scan_bsatn(
    IndexId index_id, const uint8_t* prefix_ptr, size_t prefix_len, ColId prefix_elems,
    const uint8_t* rstart_ptr, size_t rstart_len, const uint8_t* rend_ptr, size_t rend_len, 
    RowIter* out);

// ===== Row Iterator Operations =====
STDB_IMPORT(row_iter_bsatn_advance)
int16_t row_iter_bsatn_advance(RowIter iter, uint8_t* buffer_ptr, size_t* buffer_len_ptr);

STDB_IMPORT(row_iter_bsatn_close)
Status row_iter_bsatn_close(RowIter iter);

// ===== Data Manipulation =====
STDB_IMPORT(datastore_insert_bsatn)
Status datastore_insert_bsatn(TableId table_id, uint8_t* row_ptr, size_t* row_len_ptr);

STDB_IMPORT(datastore_update_bsatn)
Status datastore_update_bsatn(TableId table_id, IndexId index_id, uint8_t* row_ptr, size_t* row_len_ptr);

// ===== Delete Operations =====
STDB_IMPORT(datastore_delete_by_index_scan_range_bsatn)
Status datastore_delete_by_index_scan_range_bsatn(
    IndexId index_id, const uint8_t* prefix_ptr, size_t prefix_len, ColId prefix_elems,
    const uint8_t* rstart_ptr, size_t rstart_len, const uint8_t* rend_ptr, size_t rend_len, 
    uint32_t* out);

STDB_IMPORT_10_4(datastore_delete_by_index_scan_point_bsatn)
Status datastore_delete_by_index_scan_point_bsatn(
    IndexId index_id, const uint8_t* point_ptr, size_t point_len,
    uint32_t* out);

STDB_IMPORT(datastore_delete_by_btree_scan_bsatn)
Status datastore_delete_by_btree_scan_bsatn(
    IndexId index_id, const uint8_t* prefix_ptr, size_t prefix_len, ColId prefix_elems,
    const uint8_t* rstart_ptr, size_t rstart_len, const uint8_t* rend_ptr, size_t rend_len, 
    uint32_t* out);

STDB_IMPORT(datastore_delete_all_by_eq_bsatn)
Status datastore_delete_all_by_eq_bsatn(
    TableId table_id, const uint8_t* rel_ptr, size_t rel_len,
    uint32_t* out);

// ===== Bytes Source/Sink Operations =====
STDB_IMPORT(bytes_source_read)
int16_t bytes_source_read(BytesSource source, uint8_t* buffer_ptr, size_t* buffer_len_ptr);

STDB_IMPORT_10_1(bytes_source_remaining_length)
int16_t bytes_source_remaining_length(BytesSource source, uint32_t* out);

STDB_IMPORT(bytes_sink_write)
Status bytes_sink_write(BytesSink sink, const uint8_t* buffer_ptr, size_t* buffer_len_ptr);

// ===== Console/Logging Operations =====
STDB_IMPORT(console_log)
void console_log(
    LogLevel level, const uint8_t* target_ptr, size_t target_len,
    const uint8_t* filename_ptr, size_t filename_len, uint32_t line_number,
    const uint8_t* message_ptr, size_t message_len);

STDB_IMPORT(console_timer_start)
ConsoleTimerId console_timer_start(const uint8_t* name_ptr, size_t name_len);

STDB_IMPORT(console_timer_end)
Status console_timer_end(ConsoleTimerId timer_id);

// ===== Scheduling =====
#ifdef SPACETIMEDB_UNSTABLE_FEATURES
STDB_IMPORT(volatile_nonatomic_schedule_immediate)
void volatile_nonatomic_schedule_immediate(
    const uint8_t* name_ptr, size_t name_len, const uint8_t* args_ptr, size_t args_len);
#endif

// ===== Identity =====
STDB_IMPORT(identity)
void identity(uint8_t* id_ptr);

// ===== JWT (spacetime_10.2) =====
#define STDB_IMPORT_10_2(name) \
    __attribute__((import_module("spacetime_10.2"), import_name(#name))) extern

STDB_IMPORT_10_2(get_jwt)
Status get_jwt(const uint8_t* connection_id_ptr, BytesSource* out);

// ===== Procedure Transactions (spacetime_10.3) =====
#ifdef SPACETIMEDB_UNSTABLE_FEATURES
STDB_IMPORT_10_3(procedure_start_mut_tx)
Status procedure_start_mut_tx(int64_t* out);

STDB_IMPORT_10_3(procedure_commit_mut_tx)
Status procedure_commit_mut_tx();

STDB_IMPORT_10_3(procedure_abort_mut_tx)
Status procedure_abort_mut_tx();

// ===== Procedure HTTP (spacetime_10.3) =====
STDB_IMPORT_10_3(procedure_http_request)
Status procedure_http_request(
    const uint8_t* request_ptr, size_t request_len,
    const uint8_t* body_ptr, size_t body_len,
    BytesSource out[2]);
#endif

} // extern "C"

// ========================================================================
// SECTION 2: EXPORT DECLARATIONS - Functions modules provide to SpacetimeDB
// ========================================================================

// Macro for declaring exported functions that the module provides
#define STDB_EXPORT(name) __attribute__((export_name(#name)))

extern "C" {

// ===== Required Module Exports =====
STDB_EXPORT(__describe_module__)
void __describe_module__(BytesSink description);

STDB_EXPORT(__call_reducer__)
int16_t __call_reducer__(
    uint32_t id,
    uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
    uint64_t conn_id_0, uint64_t conn_id_1,
    uint64_t timestamp, 
    BytesSource args, 
    BytesSink error);

// ========================================================================
// WASI SHIMS
// ========================================================================

// This indicates that WASI shims are provided by the SpacetimeDB C++ Module Library
// When this is defined, modules can safely use the C++ standard library
// The actual WASI function declarations come from system headers (wasi/api.h)
// Our implementations are in wasi_shims.cpp
#define SPACETIMEDB_HAS_WASI_SHIMS 1

} // extern "C"

#pragma clang diagnostic pop

#endif // SPACETIMEDB_ABI_H