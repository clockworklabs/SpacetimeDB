#ifndef SPACETIMEDB_BSATN_SUM_TYPE_H
#define SPACETIMEDB_BSATN_SUM_TYPE_H

#include <variant>
#include <cstdint>
#include <type_traits>
#include "traits.h"
#include "reader.h"
#include "writer.h"
#include "algebraic_type.h"

namespace SpacetimeDb {
namespace bsatn {

/**
 * @brief A sum type (tagged union) implementation for BSATN serialization
 * 
 * This class wraps std::variant to provide a sum type that can be serialized
 * and deserialized using BSATN format. It's similar to Rust's enum types
 * with data payloads.
 * 
 * @tparam Ts... The types that can be stored in this sum type
 */
template<typename... Ts>
class SumType {
public:
    using VariantType = std::variant<Ts...>;
    
private:
    VariantType value_;
    
public:
    // Constructors
    SumType() = default;
    
    template<typename T>
    SumType(T&& val) : value_(std::forward<T>(val)) {}
    
    // Assignment operators
    template<typename T>
    SumType& operator=(T&& val) {
        value_ = std::forward<T>(val);
        return *this;
    }
    
    // Get the current tag (variant index)
    uint8_t tag() const {
        return static_cast<uint8_t>(value_.index());
    }
    
    // Check if the sum type holds a specific type
    template<typename T>
    bool is() const {
        return std::holds_alternative<T>(value_);
    }
    
    // Get the value as a specific type (throws if wrong type)
    template<typename T>
    T& get() {
        return std::get<T>(value_);
    }
    
    template<typename T>
    const T& get() const {
        return std::get<T>(value_);
    }
    
    // Get the value as a specific type (returns nullptr if wrong type)
    template<typename T>
    T* get_if() {
        return std::get_if<T>(&value_);
    }
    
    template<typename T>
    const T* get_if() const {
        return std::get_if<T>(&value_);
    }
    
    // Visit the sum type with a visitor
    template<typename Visitor>
    auto visit(Visitor&& vis) {
        return std::visit(std::forward<Visitor>(vis), value_);
    }
    
    template<typename Visitor>
    auto visit(Visitor&& vis) const {
        return std::visit(std::forward<Visitor>(vis), value_);
    }
    
    // Access the underlying variant
    VariantType& variant() { return value_; }
    const VariantType& variant() const { return value_; }
};

// Helper to serialize a variant value at a specific index
template<size_t I, typename... Ts>
void serialize_variant_at_index(Writer& writer, const std::variant<Ts...>& var) {
    if constexpr (I < sizeof...(Ts)) {
        if (var.index() == I) {
            serialize(writer, std::get<I>(var));
        } else {
            serialize_variant_at_index<I + 1>(writer, var);
        }
    }
}

// Helper to deserialize a variant value at a specific index  
template<size_t I, typename... Ts>
void deserialize_variant_at_index(Reader& reader, std::variant<Ts...>& var, uint8_t tag) {
    if constexpr (I < sizeof...(Ts)) {
        if (tag == I) {
            using T = std::variant_alternative_t<I, std::variant<Ts...>>;
            var = deserialize<T>(reader);
        } else {
            deserialize_variant_at_index<I + 1>(reader, var, tag);
        }
    } else {
        std::abort(); // Invalid sum type tag
    }
}

// BSATN traits specialization for SumType
template<typename... Ts>
struct bsatn_traits<SumType<Ts...>> {
    using sum_type = SumType<Ts...>;
    
    static AlgebraicType algebraic_type() {
        // For now, return a string type as placeholder
        // TODO: Implement proper sum type registration in V9TypeRegistration system
        return AlgebraicType::String();
    }
};

// Serialization for SumType
template<typename... Ts>
void serialize(Writer& writer, const SumType<Ts...>& value) {
    // Write the tag byte
    writer.write_u8(value.tag());
    
    // Write the variant data
    serialize_variant_at_index<0>(writer, value.variant());
}

// Deserialization for SumType
template<typename... Ts>
SumType<Ts...> deserialize(Reader& reader, std::type_identity<SumType<Ts...>>) {
    // Read the tag byte
    uint8_t tag = reader.read_u8();
    
    // Check tag is valid
    if (tag >= sizeof...(Ts)) {
        std::abort(); // Invalid sum type tag
    }
    
    // Deserialize the appropriate variant
    SumType<Ts...> result;
    deserialize_variant_at_index<0>(reader, result.variant(), tag);
    
    return result;
}

} // namespace bsatn

// Result type helper (like Rust's Result<T, E>)
template<typename T, typename E>
using Result = bsatn::SumType<T, E>;

// Factory functions for Result
template<typename T, typename E>
Result<T, E> Ok(T&& value) {
    return Result<T, E>(std::forward<T>(value));
}

template<typename T, typename E>
Result<T, E> Err(E&& error) {
    return Result<T, E>(std::forward<E>(error));
}

} // namespace SpacetimeDb

#endif // SPACETIMEDB_BSATN_SUM_TYPE_H