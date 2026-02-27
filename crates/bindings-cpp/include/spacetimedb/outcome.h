#ifndef SPACETIMEDB_OUTCOME_H
#define SPACETIMEDB_OUTCOME_H

#include <optional>
#include <string>
#include <type_traits>
#include <utility>
#include <variant>

namespace SpacetimeDB {

// Internal distinct error type to avoid std::variant<T, std::string> when T == std::string.
struct OutcomeError {
    std::string msg;
};

// Forward declaration
template <typename T>
class Outcome;

// ==================== Outcome<T> ====================

template <typename T>
class [[nodiscard]] Outcome {
private:
    // index 0 = Ok(T), index 1 = Err(OutcomeError)
    std::variant<T, OutcomeError> value_;

    // Private constructors
    explicit Outcome(T value)
        : value_(std::in_place_index<0>, std::move(value)) {}

    explicit Outcome(OutcomeError error)
        : value_(std::in_place_index<1>, std::move(error)) {}

public:
    using value_type = T;

    // ---- Factories ----
    static Outcome Ok(T value) {
        return Outcome(std::move(value));
    }

    static Outcome Err(std::string error) {
        return Outcome(OutcomeError{std::move(error)});
    }

    static Outcome Err(const char* error) {
        return Outcome(OutcomeError{std::string(error)});
    }

    // ---- State ----
    bool is_ok() const { return value_.index() == 0; }
    bool is_err() const { return value_.index() == 1; }

    // ---- Accessors ----
    // Precondition: is_ok()
    T& value() & { return std::get<0>(value_); }
    const T& value() const & { return std::get<0>(value_); }
    T&& value() && { return std::get<0>(std::move(value_)); }

    // Precondition: is_err()
    const std::string& error() const { return std::get<1>(value_).msg; }

    // Optional convenience: like Rust's unwrap_or
    template <typename U>
    T value_or(U&& fallback) const & {
        return is_ok() ? std::get<0>(value_) : T(std::forward<U>(fallback));
    }
};

// ==================== Outcome<void> specialization ====================

template <>
class [[nodiscard]] Outcome<void> {
private:
    std::optional<OutcomeError> error_;

    explicit Outcome(std::nullopt_t) : error_(std::nullopt) {}
    explicit Outcome(OutcomeError error) : error_(std::move(error)) {}

public:
    using value_type = void;

    // ---- Factories ----
    static Outcome Ok() {
        return Outcome(std::nullopt);
    }

    static Outcome Err(std::string error) {
        return Outcome(OutcomeError{std::move(error)});
    }

    static Outcome Err(const char* error) {
        return Outcome(OutcomeError{std::string(error)});
    }

    // ---- State ----
    bool is_ok() const { return !error_.has_value(); }
    bool is_err() const { return error_.has_value(); }

    // Precondition: is_err()
    const std::string& error() const { return error_->msg; }
};

// ==================== Free helper functions ====================

// Ok() for void
inline Outcome<void> Ok() {
    return Outcome<void>::Ok();
}

// Err() for void
inline Outcome<void> Err(std::string msg) {
    return Outcome<void>::Err(std::move(msg));
}

inline Outcome<void> Err(const char* msg) {
    return Outcome<void>::Err(msg);
}

// Ok(value) for T
template <typename T>
inline Outcome<std::decay_t<T>> Ok(T&& value) {
    return Outcome<std::decay_t<T>>::Ok(std::forward<T>(value));
}

// Err<T>(msg) for T
template <typename T>
inline Outcome<T> Err(std::string msg) {
    return Outcome<T>::Err(std::move(msg));
}

template <typename T>
inline Outcome<T> Err(const char* msg) {
    return Outcome<T>::Err(msg);
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_OUTCOME_H
