#pragma once

#include <optional>
#include <variant>
#include <string>
#include <concepts>
#include <type_traits>
#include <source_location>

namespace SpacetimeDB {

// =============================================================================
// Phase 3: C++20 Error Handling System - Closing the Gap with Rust
// =============================================================================

/// Error types for database operations
enum class DatabaseError {
    ConstraintViolation,
    DuplicateKey,
    NotFound,
    SerializationError,
    ConnectionError,
    Unknown
};

/// Error information with C++20 source location support
struct ErrorInfo {
    DatabaseError error_type;
    std::string message;
    std::source_location location;
    
    ErrorInfo(DatabaseError type, std::string msg, 
              std::source_location loc = std::source_location::current())
        : error_type(type), message(std::move(msg)), location(loc) {}
};

/// Result type similar to Rust's Result<T, E> - using std::variant since std::expected is C++23
template<typename T>
using DatabaseResult = std::variant<T, ErrorInfo>;

/// Helper concepts for database operations
template<typename T>
concept DatabaseType = requires(T t) {
    // Must be serializable and have comparison operators
    requires std::copyable<T>;
    requires std::equality_comparable<T>;
};

// =============================================================================
// Result helper functions (Rust-inspired API)
// =============================================================================

/// Check if result is successful
template<typename T>
constexpr bool is_ok(const DatabaseResult<T>& result) {
    return std::holds_alternative<T>(result);
}

/// Check if result is an error  
template<typename T>
constexpr bool is_error(const DatabaseResult<T>& result) {
    return std::holds_alternative<ErrorInfo>(result);
}

/// Get value from successful result (throws if error)
template<typename T>
constexpr T& get_value(DatabaseResult<T>& result) {
    if (is_error(result)) {
        std::abort(); // Attempted to get value from error result
    }
    return std::get<T>(result);
}

/// Get value from successful result (const version)
template<typename T>
constexpr const T& get_value(const DatabaseResult<T>& result) {
    if (is_error(result)) {
        std::abort(); // Attempted to get value from error result
    }
    return std::get<T>(result);
}

/// Get error from failed result
template<typename T>
constexpr const ErrorInfo& get_error(const DatabaseResult<T>& result) {
    if (is_ok(result)) {
        std::abort(); // Attempted to get error from successful result
    }
    return std::get<ErrorInfo>(result);
}

/// Unwrap result or provide default value
template<typename T>
constexpr T unwrap_or(const DatabaseResult<T>& result, T default_value) {
    if (is_ok(result)) {
        return get_value(result);
    }
    return default_value;
}

/// Convert result to optional (loses error information)
template<typename T>
constexpr std::optional<T> to_optional(const DatabaseResult<T>& result) {
    if (is_ok(result)) {
        return get_value(result);
    }
    return std::nullopt;
}

// =============================================================================
// Upsert result types for insert_or_update operations
// =============================================================================

enum class UpsertAction {
    Inserted,  // Row was newly inserted
    Updated    // Existing row was updated
};

/// Result of an insert_or_update operation
template<typename T>
struct UpsertResult {
    T value;
    UpsertAction action;
    
    UpsertResult(T val, UpsertAction act) : value(std::move(val)), action(act) {}
    
    bool was_inserted() const { return action == UpsertAction::Inserted; }
    bool was_updated() const { return action == UpsertAction::Updated; }
};

// =============================================================================
// Enhanced table accessor base class with error handling
// =============================================================================

/// Base class providing error-safe database operations
template<typename TableType>
requires DatabaseType<TableType>
class ErrorSafeTableAccessor {
public:
    /// Try to insert a row, returning result instead of throwing
    /// Rust equivalent: ctx.db.table().try_insert(row)
    DatabaseResult<TableType> try_insert(const TableType& row) const {
        // Without exceptions, we just call perform_insert
        // If there's an error, it will abort
        auto result = perform_insert(row);
        return DatabaseResult<TableType>(std::in_place_index<0>, result);
    }
    
    /// Insert or update a row based on primary key
    /// Rust equivalent: ctx.db.table().id().try_insert_or_update(row) 
    DatabaseResult<UpsertResult<TableType>> insert_or_update(const TableType& row) const {
        // Without exceptions, we directly perform the operations
        auto existing = find_by_primary_key(row);
        
        if (existing) {
            // Update existing row
            auto updated = perform_update(row);
            return DatabaseResult<UpsertResult<TableType>>(
                std::in_place_index<0>,
                UpsertResult<TableType>(updated, UpsertAction::Updated)
            );
        } else {
            // Insert new row
            auto inserted = perform_insert(row);
            return DatabaseResult<UpsertResult<TableType>>(
                std::in_place_index<0>,
                UpsertResult<TableType>(inserted, UpsertAction::Inserted)
            );
        }
    }
    
    /// Try to delete a row, returning whether it was found and deleted
    DatabaseResult<bool> try_delete(const TableType& row) const {
        // Without exceptions, we directly perform the delete
        bool deleted = perform_delete(row);
        return DatabaseResult<bool>(std::in_place_index<0>, deleted);
    }
    
protected:
    // Pure virtual methods to be implemented by derived classes
    virtual TableType perform_insert(const TableType& row) const = 0;
    virtual TableType perform_update(const TableType& row) const = 0;
    virtual bool perform_delete(const TableType& row) const = 0;
    virtual std::optional<TableType> find_by_primary_key(const TableType& row) const = 0;
};

// =============================================================================
// Convenient macros for error handling patterns
// =============================================================================

/// Try an operation and return early if it fails (Rust-inspired ? operator)
#define TRY_DB_OP(result) \
    ([&]() { \
        auto _temp_result = (result); \
        if (is_error(_temp_result)) { \
            return DatabaseResult<decltype(get_value(_temp_result))>( \
                std::in_place_index<1>, get_error(_temp_result)); \
        } \
        return _temp_result; \
    }())

/// Log error information with C++20 source location
#define LOG_DB_ERROR(result) \
    do { \
        if (is_error(result)) { \
            const auto& err = get_error(result); \
            LOG_ERROR_F("[%s:%d in %s] DB Error: %s", \
                       err.location.file_name(), err.location.line(), \
                       err.location.function_name(), err.message.c_str()); \
        } \
    } while(0)

} // namespace SpacetimeDB

// =============================================================================
// Usage Examples (for documentation) 
// =============================================================================

/*
// Basic error handling - Rust style
auto result = ctx.db[users].try_insert(new_user);
if (is_ok(result)) {
    auto user = get_value(result);
    LOG_INFO_F("Inserted user: %s", user.name.c_str());
} else {
    auto error = get_error(result);
    LOG_ERROR_F("Insert failed: %s", error.message.c_str());
}

// Insert or update pattern - major missing feature
auto upsert_result = ctx.db[users].insert_or_update(user);
if (is_ok(upsert_result)) {
    auto upsert = get_value(upsert_result);
    if (upsert.was_inserted()) {
        LOG_INFO("Created new user");
    } else {
        LOG_INFO("Updated existing user");
    }
}

// Chain operations with error propagation
DatabaseResult<User> create_user_safe(const std::string& name) {
    User new_user{0, name, "default@example.com"};
    
    // Try insert, propagate error if it fails
    auto insert_result = TRY_DB_OP(ctx.db[users].try_insert(new_user));
    
    // If we get here, insert succeeded
    LOG_INFO("User created successfully");
    return insert_result;
}

// Convert to optional for simpler handling
auto user_opt = to_optional(ctx.db[users].try_insert(new_user));
if (user_opt) {
    // Success - use user_opt.value()
} else {
    // Failed - error info is lost
}
*/