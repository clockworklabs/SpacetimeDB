#ifndef SPACETIMEDB_REDUCER_ERROR_H
#define SPACETIMEDB_REDUCER_ERROR_H

#include "spacetimedb/outcome.h"
#include <string>
#include <optional>

/**
 * @file reducer_error.h
 * @brief Graceful error handling for reducers using Outcome<T> pattern
 *
 * This module provides reducer-specific error handling utilities using the Outcome type.
 * For the general Outcome<T> type definition, see outcome.h.
 *
 * Modern usage (recommended):
 * @code
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id) {
 *     if (id == 0) {
 *         return Err("ID must be non-zero");
 *     }
 *     // ... rest of logic
 *     return Ok();
 * }
 * @endcode
 *
 * Legacy usage (still supported):
 * @code
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id) {
 *     if (id == 0) {
 *         fail_reducer("ID must be non-zero");
 *         return Ok();  // Or just return;
 *     }
 *     // ... rest of logic
 *     return Ok();
 * }
 * @endcode
 *
 * When a reducer returns Err():
 * - The transaction is rolled back (not committed to the log)
 * - The error message is captured and returned to the caller
 * - No database changes are persisted
 * - No WASM crash or panic occurs
 *
 * @ingroup sdk_runtime
 */

namespace SpacetimeDB {

/**
 * @brief Type alias for reducer return type, matching Rust's ReducerResult
 * 
 * Reducers use Outcome<void> for their return type. The Ok() and Err() helper
 * functions from outcome.h can be used directly:
 * 
 * @code
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id) {
 *     if (id == 0) {
 *         return Err("ID must be non-zero");
 *     }
 *     // ... rest of logic
 *     return Ok();
 * }
 * @endcode
 */
using ReducerResult = Outcome<void>;

namespace Internal {
    /**
     * Thread-local error state for the current reducer invocation.
     * This is cleared at the start of each reducer call and checked at the end.
     */
    extern thread_local std::optional<std::string> g_reducer_error_message;

    /**
     * Clear the error state. Called automatically at the start of each reducer.
     * @internal
     */
    inline void clear_reducer_error() {
        g_reducer_error_message = std::nullopt;
    }

    /**
     * Check if the current reducer has failed.
     * @internal
     */
    inline bool has_reducer_error() {
        return g_reducer_error_message.has_value();
    }

    /**
     * Get the error message if one exists.
     * @internal
     */
    inline std::string get_reducer_error() {
        return g_reducer_error_message.value_or(std::string());
    }
}

/**
 * @brief Fail the current reducer with an error message.
 *
 * This function marks the current reducer invocation as failed.
 * The transaction will be rolled back and the error message will be
 * returned to the caller. Failed transactions are NOT committed to the
 * log and will not appear in temporal queries or transaction history.
 *
 * After calling this function, the reducer should return immediately
 * to avoid executing additional logic on inconsistent state.
 *
 * @param message A descriptive error message explaining why the reducer failed
 *
 * @code
 * if (amount <= 0) {
 *     SpacetimeDB::fail_reducer("Amount must be positive");
 *     return Ok();  // Or just return;
 * }
 *
 * // Alternative using Outcome pattern:
 * if (amount <= 0) {
 *     return Err("Amount must be positive");
 * }
 * @endcode
 *
 * @note This is thread-safe and works in WASM single-threaded environments.
 * @note This does NOT throw exceptions or cause panics.
 */
inline void fail_reducer(std::string message) {
    Internal::g_reducer_error_message = std::move(message);
}

/**
 * @brief Fail the current reducer with a formatted error message.
 *
 * Convenience function for creating formatted error messages.
 *
 * @tparam Args Variadic template arguments for formatting
 * @param format Printf-style format string
 * @param args Arguments to format into the message
 *
 * @code
 * fail_reducer_fmt("Tracker %u not found", tracker_id);
 * fail_reducer_fmt("Invalid coordinates (%.2f, %.2f)", lat, lon);
 * @endcode
 */
template<typename... Args>
inline void fail_reducer_fmt(const char* format, Args... args) {
    // Get required size
    int size = std::snprintf(nullptr, 0, format, args...);
    if (size <= 0) {
        fail_reducer("Error formatting failure message");
        return;
    }

    // Format string
    std::string result(size + 1, '\0');
    std::snprintf(result.data(), result.size(), format, args...);
    result.resize(size); // Remove null terminator

    fail_reducer(std::move(result));
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_REDUCER_ERROR_H