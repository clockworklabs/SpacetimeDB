#ifndef SPACETIMEDB_OPAQUE_TYPES_H
#define SPACETIMEDB_OPAQUE_TYPES_H

#include <cstdint>
#include <functional>
#include <unordered_map>
#include <string>

/**
 * @file opaque_types.h
 * @brief Type-safe opaque wrappers for SpacetimeDB handle types
 * 
 * This file provides opaque type wrappers matching the C# implementation
 * to prevent accidental mixing of different handle types and improve type safety.
 * 
 * These types use C# style single-field structs for direct ABI compatibility,
 * allowing them to be passed directly to C imports without conversion.
 * 
 * Benefits:
 * - Prevents mixing TableId with IndexId, etc.
 * - Makes APIs more self-documenting
 * - Catches type errors at compile time
 * - Zero runtime overhead (optimized away)
 * - Direct ABI compatibility with C imports
 */

namespace SpacetimeDB {

// Macro to define opaque typedef with C# style single-field struct
// This ensures ABI compatibility - struct with single field has same
// memory layout and calling convention as the field itself
#define SPACETIMEDB_OPAQUE_TYPEDEF(Name, UnderlyingType)                      \
    struct Name {                                                              \
        UnderlyingType inner;                                                  \
                                                                              \
        /* Constructors */                                                     \
        constexpr Name() : inner(0) {}                                        \
        constexpr explicit Name(UnderlyingType v) : inner(v) {}               \
                                                                              \
        /* Conversion operators for backward compatibility */                  \
        constexpr explicit operator UnderlyingType() const { return inner; }   \
        constexpr UnderlyingType get() const { return inner; }                \
 \
                                                                              \
        /* Comparison operators */                                             \
        constexpr bool operator==(const Name& other) const {                  \
            return inner == other.inner;                                       \
        }                                                                      \
        constexpr bool operator!=(const Name& other) const {                  \
            return inner != other.inner;                                       \
        }                                                                      \
        constexpr bool operator<(const Name& other) const {                   \
            return inner < other.inner;                                        \
        }                                                                      \
        constexpr bool operator<=(const Name& other) const {                  \
            return inner <= other.inner;                                       \
        }                                                                      \
        constexpr bool operator>(const Name& other) const {                   \
            return inner > other.inner;                                        \
        }                                                                      \
        constexpr bool operator>=(const Name& other) const {                  \
            return inner >= other.inner;                                       \
        }                                                                      \
                                                                              \
        /* Check if valid (non-zero) */                                        \
        constexpr bool is_valid() const { return inner != 0; }                \
        constexpr explicit operator bool() const { return is_valid(); }        \
    };                                                                         \
                                                                              \
    /* Hash support for unordered containers */                               \
    inline std::size_t hash_value(const Name& id) {                          \
        return std::hash<UnderlyingType>{}(id.inner);                        \
    }

// Define opaque types matching C# bindings
SPACETIMEDB_OPAQUE_TYPEDEF(Status, uint16_t)
SPACETIMEDB_OPAQUE_TYPEDEF(TableId, uint32_t)
SPACETIMEDB_OPAQUE_TYPEDEF(IndexId, uint32_t)
SPACETIMEDB_OPAQUE_TYPEDEF(ColId, uint16_t)
SPACETIMEDB_OPAQUE_TYPEDEF(IndexType, uint8_t)
SPACETIMEDB_OPAQUE_TYPEDEF(LogLevel, uint8_t)
SPACETIMEDB_OPAQUE_TYPEDEF(BytesSink, uint32_t)
SPACETIMEDB_OPAQUE_TYPEDEF(BytesSource, uint32_t)
SPACETIMEDB_OPAQUE_TYPEDEF(RowIter, uint32_t)
SPACETIMEDB_OPAQUE_TYPEDEF(ConsoleTimerId, uint32_t)

// Common invalid/sentinel values
namespace Invalid {
    constexpr TableId TABLE_ID{0};
    constexpr IndexId INDEX_ID{0};
    constexpr RowIter ROW_ITER{0xFFFFFFFF};
    constexpr BytesSource BYTES_SOURCE{0xFFFFFFFF};
    constexpr BytesSink BYTES_SINK{0xFFFFFFFF};
    constexpr ConsoleTimerId CONSOLE_TIMER{0};
}

// Define all status codes in one place using X-macro pattern
#define SPACETIMEDB_STATUS_CODES(X) \
    X(OK, 0) \
    X(HOST_CALL_FAILURE, 1) \
    X(NOT_IN_TRANSACTION, 2) \
    X(BSATN_DECODE_ERROR, 3) \
    X(NO_SUCH_TABLE, 4) \
    X(NO_SUCH_INDEX, 5) \
    X(NO_SUCH_ITER, 6) \
    X(NO_SUCH_CONSOLE_TIMER, 7) \
    X(NO_SUCH_BYTES, 8) \
    X(NO_SPACE, 9) \
    X(BUFFER_TOO_SMALL, 11) \
    X(UNIQUE_ALREADY_EXISTS, 12) \
    X(SCHEDULE_AT_DELAY_TOO_LONG, 13) \
    X(INDEX_NOT_UNIQUE, 14) \
    X(NO_SUCH_ROW, 15) \
    X(AUTO_INC_OVERFLOW, 16) \
    X(NO_SUCH_REDUCER, 999) \
    X(UNKNOWN, 0xFFFF)

// Status code constants generated from the X-macro
namespace StatusCode {
    // Generate constants
    #define X(name, value) constexpr Status name{value};
    SPACETIMEDB_STATUS_CODES(X)
    #undef X
    
    // Generate string lookup function
    inline const char* to_string(Status status) {
        switch (status.inner) {
            #define X(name, value) case value: return #name;
            SPACETIMEDB_STATUS_CODES(X)
            #undef X
            default: return "UNKNOWN_ERROR";
        }
    }
}

// Log level constants
namespace LogLevelValue {
    constexpr LogLevel ERROR{0};
    constexpr LogLevel WARN{1};
    constexpr LogLevel INFO{2};
    constexpr LogLevel DEBUG{3};
    constexpr LogLevel TRACE{4};
    constexpr LogLevel PANIC{101};
}

// Index type constants
namespace IndexTypeValue {
    constexpr IndexType BTREE{0};
    constexpr IndexType HASH{1};
}

// Helper functions
inline bool is_ok(Status status) { 
    return status == StatusCode::OK; 
}

inline bool is_error(Status status) { 
    return status != StatusCode::OK; 
}

// Format status with both name and numeric code
inline std::string format_status(Status status) {
    return std::string(StatusCode::to_string(status)) + " (" + std::to_string(status.inner) + ")";
}

// Enable std::hash for opaque types
} // namespace SpacetimeDB

// Specializations for std::hash
namespace std {
    template<> struct hash<SpacetimeDB::TableId> {
        size_t operator()(const SpacetimeDB::TableId& id) const {
            return hash<uint32_t>{}(id.inner);
        }
    };
    
    template<> struct hash<SpacetimeDB::IndexId> {
        size_t operator()(const SpacetimeDB::IndexId& id) const {
            return hash<uint32_t>{}(id.inner);
        }
    };
    
    template<> struct hash<SpacetimeDB::RowIter> {
        size_t operator()(const SpacetimeDB::RowIter& iter) const {
            return hash<uint32_t>{}(iter.inner);
        }
    };
    
    template<> struct hash<SpacetimeDB::ConsoleTimerId> {
        size_t operator()(const SpacetimeDB::ConsoleTimerId& id) const {
            return hash<uint32_t>{}(id.inner);
        }
    };
}

#endif // SPACETIMEDB_OPAQUE_TYPES_H