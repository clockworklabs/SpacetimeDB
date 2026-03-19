#pragma once

/**
 * @file result.h
 * @brief Result<T, E> type for SpacetimeDB C++ bindings
 * 
 * This header provides a Result type that represents either success (ok) or failure (err).
 * It is a structural sum type compatible with SpacetimeDB's type system.
 */

#include <variant>
#include <optional>
#include <string>
#include <cstdlib>
#include "reader.h"
#include "writer.h"

namespace SpacetimeDB {

/**
 * @brief A Result type that represents either success (ok) or failure (err).
 * 
 * This is a structural sum type compatible with SpacetimeDB's type system.
 * It uses lowercase variant names ("ok" and "err") to match the Rust/C# implementations.
 * 
 * The internal representation uses std::variant<T, E> where:
 * - index 0 (holding T) = BSATN tag 0 = "ok" variant
 * - index 1 (holding E) = BSATN tag 1 = "err" variant
 * 
 * @tparam T The type of the success value
 * @tparam E The type of the error value
 */
template<typename T, typename E>
class Result {
private:
    // Tag 0 = ok (index 0), Tag 1 = err (index 1)
    std::variant<T, E> value_;
    
public:
    // ======================================================================
    // Constructors and Factory Methods
    // ======================================================================
    
    /**
     * @brief Default constructor - creates an ok Result with default-constructed value.
     * This is required for use in table structs where SPACETIMEDB_STRUCT needs
     * to create temporary instances. Only available when T is default-constructible.
     */
    Result() : value_(std::in_place_index<0>) {}
    
    /**
     * @brief Create a successful Result containing a value.
     * @param value The success value (moved into the Result)
     * @return A Result in the ok state
     */
    static Result ok(T value) {
        Result r;
        r.value_ = std::move(value);
        return r;
    }
    
    /**
     * @brief Create a failed Result containing an error.
     * @param error The error value (moved into the Result)
     * @return A Result in the err state
     */
    static Result err(E error) {
        Result r;
        r.value_ = std::move(error);
        return r;
    }
    
    // ======================================================================
    // State Checking
    // ======================================================================
    
    /**
     * @brief Check if this Result contains a success value.
     * @return true if this is ok, false if err
     */
    bool is_ok() const {
        return value_.index() == 0;
    }
    
    /**
     * @brief Check if this Result contains an error value.
     * @return true if this is err, false if ok
     */
    bool is_err() const {
        return value_.index() == 1;
    }
    
    // ======================================================================
    // Value Accessors (with panic on wrong variant)
    // ======================================================================
    
    /**
     * @brief Get the success value, aborting if this is err.
     * @return Reference to the success value
     * @note Calls std::abort() if this Result is err
     */
    const T& unwrap() const {
        if (is_err()) {
            std::abort(); // Called unwrap() on an err Result
        }
        return std::get<0>(value_);
    }
    
    T& unwrap() {
        if (is_err()) {
            std::abort(); // Called unwrap() on an err Result
        }
        return std::get<0>(value_);
    }
    
    /**
     * @brief Get the error value, aborting if this is ok.
     * @return Reference to the error value
     * @note Calls std::abort() if this Result is ok
     */
    const E& unwrap_err() const {
        if (is_ok()) {
            std::abort(); // Called unwrap_err() on an ok Result
        }
        return std::get<1>(value_);
    }
    
    E& unwrap_err() {
        if (is_ok()) {
            std::abort(); // Called unwrap_err() on an ok Result
        }
        return std::get<1>(value_);
    }
    
    // ======================================================================
    // Safe Value Accessors (returning optional)
    // ======================================================================
    
    /**
     * @brief Get the success value if ok, otherwise nullopt.
     * @return Optional containing the success value, or nullopt if err
     */
    std::optional<T> ok_value() const {
        if (is_ok()) {
            return std::get<0>(value_);
        }
        return std::nullopt;
    }
    
    /**
     * @brief Get the error value if err, otherwise nullopt.
     * @return Optional containing the error value, or nullopt if ok
     */
    std::optional<E> err_value() const {
        if (is_err()) {
            return std::get<1>(value_);
        }
        return std::nullopt;
    }
    
    // ======================================================================
    // Transformations
    // ======================================================================
    
    /**
     * @brief Get the success value or a default value.
     * @param default_value The value to return if this is err
     * @return The success value if ok, otherwise default_value
     */
    T unwrap_or(T default_value) const {
        return is_ok() ? std::get<0>(value_) : default_value;
    }
    
    // ======================================================================
    // BSATN Serialization
    // ======================================================================
    
    void bsatn_serialize(bsatn::Writer& writer) const {
        // Write variant tag (0 = ok, 1 = err) - directly from variant index
        writer.write_u8(static_cast<uint8_t>(value_.index()));
        
        // Write variant data
        if (is_ok()) {
            bsatn::bsatn_traits<T>::serialize(writer, std::get<0>(value_));
        } else {
            bsatn::bsatn_traits<E>::serialize(writer, std::get<1>(value_));
        }
    }
    
    static Result bsatn_deserialize(bsatn::Reader& reader) {
        uint8_t tag = reader.read_u8();
        
        if (tag == 0) {
            // ok variant (tag 0 = variant index 0)
            T value = bsatn::bsatn_traits<T>::deserialize(reader);
            return Result::ok(std::move(value));
        } else if (tag == 1) {
            // err variant (tag 1 = variant index 1)
            E error = bsatn::bsatn_traits<E>::deserialize(reader);
            return Result::err(std::move(error));
        } else {
            std::abort(); // Invalid Result variant tag
        }
    }
};

} // namespace SpacetimeDB
