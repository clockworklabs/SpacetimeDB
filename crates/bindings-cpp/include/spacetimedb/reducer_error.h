#ifndef SPACETIMEDB_REDUCER_ERROR_H
#define SPACETIMEDB_REDUCER_ERROR_H

#include <string>
#include <optional>
#include <variant>
#include <utility>

/**
 * @file reducer_error.h
 * @brief Graceful error handling for reducers using Outcome<T, E> pattern
 *
 * This module provides a Rust-like Outcome type for reducers to fail gracefully
 * with error messages, matching the Rust SDK's Result<(), E> pattern.
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

namespace SpacetimeDb {

// Forward declarations
template<typename T> class Outcome;

/**
 * @brief Outcome type for operations that can succeed with a value or fail with an error.
 *
 * This type is similar to Rust's Result<T, E> where E is always std::string.
 * It provides a type-safe way to handle errors without exceptions.
 *
 * @tparam T The success value type
 */
template<typename T>
class [[nodiscard]] Outcome {
private:
    std::variant<T, std::string> value_;
    bool is_ok_;
    
    // Private constructors - use Ok() and Err() factory functions
    Outcome(T value, bool) : value_(std::move(value)), is_ok_(true) {}
    Outcome(std::string error, int) : value_(std::move(error)), is_ok_(false) {}
    
public:
    /**
     * @brief Create a successful Outcome with a value
     */
    static Outcome Ok(T value) {
        return Outcome(std::move(value), true);
    }
    
    /**
     * @brief Create a failed Outcome with an error message
     */
    static Outcome Err(std::string error) {
        return Outcome(std::move(error), 0);
    }
    
    /**
     * @brief Check if the result is successful
     */
    bool is_ok() const { return is_ok_; }
    
    /**
     * @brief Check if the result is an error
     */
    bool is_err() const { return !is_ok_; }
    
    /**
     * @brief Get the success value (only valid if is_ok())
     */
    T& value() & { return std::get<T>(value_); }
    T&& value() && { return std::get<T>(std::move(value_)); }
    const T& value() const & { return std::get<T>(value_); }
    
    /**
     * @brief Get the error message (only valid if is_err())
     */
    const std::string& error() const { return std::get<std::string>(value_); }
};

/**
 * @brief Specialization of Outcome for void (used by reducers)
 *
 * This matches Rust's Result<(), E> pattern where () represents success with no value.
 */
template<>
class [[nodiscard]] Outcome<void> {
private:
    std::optional<std::string> error_;
    
    // Private constructors
    Outcome(bool success) : error_(success ? std::nullopt : std::optional<std::string>("")) {}
    Outcome(std::string error) : error_(std::move(error)) {}
    
public:
    /**
     * @brief Create a successful Outcome (no value)
     */
    static Outcome Ok() {
        return Outcome(true);
    }
    
    /**
     * @brief Create a failed Outcome with an error message
     */
    static Outcome Err(std::string error) {
        return Outcome(std::move(error));
    }
    
    /**
     * @brief Check if the result is successful
     */
    bool is_ok() const { return !error_.has_value(); }
    
    /**
     * @brief Check if the result is an error
     */
    bool is_err() const { return error_.has_value(); }
    
    /**
     * @brief Get the error message (only valid if is_err())
     */
    const std::string& error() const { return error_.value(); }
};

/**
 * @brief Type alias for reducer return type, matching Rust's ReducerResult
 */
using ReducerResult = Outcome<void>;

/**
 * @brief Helper function to create a successful Outcome<void>
 * 
 * Usage: return Ok();
 */
inline ReducerResult Ok() { 
    return ReducerResult::Ok(); 
}

/**
 * @brief Helper function to create a failed Outcome with an error message
 * 
 * Usage: return Err("Something went wrong");
 */
inline ReducerResult Err(std::string msg) { 
    return ReducerResult::Err(std::move(msg)); 
}

/**
 * @brief Helper function to create a successful Outcome<T> with a value
 * 
 * Usage: return Ok(user);
 */
template<typename T>
inline Outcome<T> Ok(T value) { 
    return Outcome<T>::Ok(std::move(value)); 
}

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
 * @deprecated Prefer using Outcome-based error handling: return Err("message");
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
 * // Old style (still works):
 * if (amount <= 0) {
 *     SpacetimeDb::fail_reducer("Amount must be positive");
 *     return Ok();
 * }
 *
 * // Preferred style:
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

} // namespace SpacetimeDb

#endif // SPACETIMEDB_REDUCER_ERROR_H