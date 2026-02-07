#ifndef SPACETIMEDB_FFI_H
#define SPACETIMEDB_FFI_H

#include <cstdint>
#include <cstddef>
#include "spacetimedb/abi/opaque_types.h"
#include "spacetimedb/abi/abi.h"

/**
 * @file FFI.h
 * @brief SpacetimeDB Foreign Function Interface (FFI) layer for C++ modules
 * 
 * This file re-exports the raw ABI functions with additional type aliases
 * and convenience functions. Since we now use C# style opaque types that
 * are ABI-compatible, no conversion is needed.
 * 
 * Organization:
 * - Raw C ABI with opaque types is in abi.h
 * - This file provides type aliases and convenience functions
 * 
 * Key Features:
 * - Type-safe opaque types prevent mixing TableId with IndexId etc.
 * - Full BSATN integration for all data operations
 * - Modern iterator API with proper resource management
 * - Comprehensive error handling with Status codes
 * 
 * Note: WASI shims for C++ standard library support are provided separately
 * in the module library implementation.
 */

// ========================================================================
// C++ TYPE ALIASES AND CONVENIENCE FUNCTIONS
// ========================================================================

namespace SpacetimeDB {
namespace FFI {

using LogLevel = ::SpacetimeDB::LogLevel;
using IndexType = ::SpacetimeDB::IndexType;

// Re-export all functions from the raw ABI
// Since we now use ABI-compatible opaque types, no conversion is needed
using ::table_id_from_name;
using ::index_id_from_name;
using ::datastore_table_row_count;
using ::datastore_table_scan_bsatn;
using ::datastore_index_scan_range_bsatn;
using ::datastore_index_scan_point_bsatn;
using ::datastore_btree_scan_bsatn;
using ::row_iter_bsatn_advance;
using ::row_iter_bsatn_close;
using ::datastore_insert_bsatn;
using ::datastore_update_bsatn;
using ::datastore_delete_by_index_scan_range_bsatn;
using ::datastore_delete_by_index_scan_point_bsatn;
using ::datastore_delete_by_btree_scan_bsatn;
using ::datastore_delete_all_by_eq_bsatn;
using ::bytes_source_read;
using ::bytes_source_remaining_length;
using ::bytes_sink_write;
using ::console_log;
using ::console_timer_start;
using ::console_timer_end;

// ===== Scheduling =====
#ifdef SPACETIMEDB_UNSTABLE_FEATURES
using ::volatile_nonatomic_schedule_immediate;
#endif

// ===== Identity =====
using ::identity;

// ===== JWT =====
using ::get_jwt;

// ===== Procedure Transactions =====
#ifdef SPACETIMEDB_UNSTABLE_FEATURES
using ::procedure_start_mut_tx;
using ::procedure_commit_mut_tx;
using ::procedure_abort_mut_tx;
#endif

// ===== Module Export Helpers =====

// Helper for __describe_module__ implementation
inline void describe_module(BytesSink description) {
    ::__describe_module__(description);
}

// Helper for __call_reducer__ implementation
inline int16_t call_reducer(
    uint32_t id,
    uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
    uint64_t conn_id_0, uint64_t conn_id_1,
    uint64_t timestamp, 
    BytesSource args, 
    BytesSink error) {
    return ::__call_reducer__(id, sender_0, sender_1, sender_2, sender_3,
                             conn_id_0, conn_id_1, timestamp, 
                             args, error);
}

// Utility functions for common operations
namespace Utils {

// Helper to write data to a BytesSink
inline void write_bytes_to_sink(BytesSink sink_handle, const uint8_t* data, size_t len) {
    size_t buffer_len = len;
    Status status = bytes_sink_write(sink_handle, data, &buffer_len);
    if (is_error(status)) {
        // In a real implementation, this would use the SDK's exception system
        // For now, we just ignore errors in this utility function
    }
}

// Helper to read all data from a BytesSource
inline bool read_all_from_source(BytesSource source_handle, uint8_t* buffer, size_t* buffer_len) {
    int16_t result = bytes_source_read(source_handle, buffer, buffer_len);
    return result >= 0;
}

} // namespace Utils

// Additional status codes
namespace StatusCode {
    using namespace ::SpacetimeDB::StatusCode;
    constexpr Status EXHAUSTED{16};
}

// Custom wrapper for simplified logging
inline void console_log(const uint8_t* message, size_t message_len, LogLevel level) {
    ::console_log(level, nullptr, 0, nullptr, 0, 0, message, message_len);
}

} // namespace FFI
} // namespace SpacetimeDB

#endif // SPACETIMEDB_FFI_H