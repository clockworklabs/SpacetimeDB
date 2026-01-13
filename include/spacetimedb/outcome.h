#ifndef SPACETIMEDB_OUTCOME_H
#define SPACETIMEDB_OUTCOME_H

#include <string>
#include <optional>
#include <variant>
#include <utility>

/**
 * @file outcome.h
 * @brief General-purpose error handling wrapper type
 *
 * This module provides a Rust-like Outcome type for functions to return
 * either a success value or an error message, matching the Rust SDK's Result<T, E> pattern
 * where E is always std::string.
 *
 * This type is used throughout the SpacetimeDB C++ SDK for error handling:
 * - Reducers: Outcome<void> (ReducerResult)
 * - HTTP requests: Outcome<HttpResponse>
 * - Future APIs: Outcome<T> for any T
 *
 * @ingroup sdk_runtime
 */

namespace SpacetimeDb {

// Forward declaration
template<typename T> class Outcome;

/**
 * @brief Outcome type for operations that can succeed with a value or fail with an error.
 *
 * This type is similar to Rust's Result<T, E> where E is always std::string.
 * It provides a type-safe way to handle errors without exceptions.
 *
 * Example:
 * @code
 * Outcome<int> divide(int a, int b) {
 *     if (b == 0) return Outcome<int>::Err("Division by zero");
 *     return Outcome<int>::Ok(a / b);
 * }
 *
 * auto result = divide(10, 2);
 * if (result.is_ok()) {
 *     std::cout << "Result: " << result.value() << std::endl;
 * } else {
 *     std::cout << "Error: " << result.error() << std::endl;
 * }
 * @endcode
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
     * 
     * @param value The success value to wrap
     * @return Outcome<T> containing the success value
     */
    static Outcome Ok(T value) {
        return Outcome(std::move(value), true);
    }
    
    /**
     * @brief Create a failed Outcome with an error message
     * 
     * @param error The error message describing what went wrong
     * @return Outcome<T> containing the error message
     */
    static Outcome Err(std::string error) {
        return Outcome(std::move(error), 0);
    }
    
    /**
     * @brief Check if the result is successful
     * 
     * @return true if this Outcome contains a success value
     */
    bool is_ok() const { return is_ok_; }
    
    /**
     * @brief Check if the result is an error
     * 
     * @return true if this Outcome contains an error message
     */
    bool is_err() const { return !is_ok_; }
    
    /**
     * @brief Get the success value (only valid if is_ok())
     * 
     * @warning Calling this when is_err() is true results in undefined behavior
     * @return Reference to the success value
     */
    T& value() & { return std::get<T>(value_); }
    
    /**
     * @brief Get the success value by moving (only valid if is_ok())
     * 
     * @warning Calling this when is_err() is true results in undefined behavior
     * @return Rvalue reference to the success value
     */
    T&& value() && { return std::get<T>(std::move(value_)); }
    
    /**
     * @brief Get the success value (const version, only valid if is_ok())
     * 
     * @warning Calling this when is_err() is true results in undefined behavior
     * @return Const reference to the success value
     */
    const T& value() const & { return std::get<T>(value_); }
    
    /**
     * @brief Get the error message (only valid if is_err())
     * 
     * @warning Calling this when is_ok() is true results in undefined behavior
     * @return Const reference to the error message
     */
    const std::string& error() const { return std::get<std::string>(value_); }
};

/**
 * @brief Specialization of Outcome for void (success with no value)
 *
 * This matches Rust's Result<(), E> pattern where () represents success with no value.
 * Commonly used for reducers and other operations that either succeed or fail without
 * returning a value.
 *
 * Example:
 * @code
 * Outcome<void> validate_user(uint32_t user_id) {
 *     if (user_id == 0) {
 *         return Outcome<void>::Err("User ID cannot be zero");
 *     }
 *     return Outcome<void>::Ok();
 * }
 *
 * auto result = validate_user(42);
 * if (result.is_ok()) {
 *     std::cout << "Validation passed" << std::endl;
 * } else {
 *     std::cout << "Error: " << result.error() << std::endl;
 * }
 * @endcode
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
     * @brief Create a successful Outcome with no value
     * 
     * @return Outcome<void> representing success
     */
    static Outcome Ok() {
        return Outcome(true);
    }
    
    /**
     * @brief Create a failed Outcome with an error message
     * 
     * @param error The error message describing what went wrong
     * @return Outcome<void> containing the error message
     */
    static Outcome Err(std::string error) {
        return Outcome(std::move(error));
    }
    
    /**
     * @brief Check if the result is successful
     * 
     * @return true if this Outcome represents success
     */
    bool is_ok() const { return !error_.has_value(); }
    
    /**
     * @brief Check if the result is an error
     * 
     * @return true if this Outcome contains an error message
     */
    bool is_err() const { return error_.has_value(); }
    
    /**
     * @brief Get the error message (only valid if is_err())
     * 
     * @warning Calling this when is_ok() is true results in undefined behavior
     * @return Const reference to the error message
     */
    const std::string& error() const { return error_.value(); }
};

// ==================== Helper Functions ====================

/**
 * @brief Create a successful Outcome<void> (no value)
 * 
 * This is a convenience function for creating successful results.
 * Usage: return Ok();
 */
inline Outcome<void> Ok() { 
    return Outcome<void>::Ok(); 
}

/**
 * @brief Create a failed Outcome<void> with an error message
 * 
 * This is a convenience function for creating error results.
 * Usage: return Err("Something went wrong");
 */
inline Outcome<void> Err(std::string msg) { 
    return Outcome<void>::Err(std::move(msg)); 
}

/**
 * @brief Create a successful Outcome<T> with a value
 * 
 * This is a template convenience function for creating successful results with values.
 * Usage: return Ok(user);
 * 
 * @tparam T The type of the success value
 * @param value The success value
 * @return Outcome<T> containing the value
 */
template<typename T>
inline Outcome<T> Ok(T value) { 
    return Outcome<T>::Ok(std::move(value)); 
}

/**
 * @brief Create a failed Outcome<T> with an error message (const char* version)
 * 
 * This is a template convenience function for creating error results.
 * The return type must be explicitly specified as a template parameter.
 * 
 * Usage: 
 *   Outcome<uint32_t> foo() {
 *       return Err<uint32_t>("Something went wrong");
 *   }
 * 
 * @tparam T The expected success type
 * @param msg The error message
 * @return Outcome<T> containing the error
 */
template<typename T>
inline Outcome<T> Err(const char* msg) { 
    return Outcome<T>::Err(std::string(msg)); 
}

/**
 * @brief Create a failed Outcome<T> with an error message (std::string version)
 * 
 * This is a template convenience function for creating error results.
 * The return type must be explicitly specified as a template parameter.
 * 
 * Usage: 
 *   Outcome<uint32_t> foo() {
 *       return Err<uint32_t>("Something went wrong");
 *   }
 * 
 * @tparam T The expected success type
 * @param msg The error message
 * @return Outcome<T> containing the error
 */
template<typename T>
inline Outcome<T> Err(std::string msg) { 
    return Outcome<T>::Err(std::move(msg)); 
}

} // namespace SpacetimeDb

#endif // SPACETIMEDB_OUTCOME_H
