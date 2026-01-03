#ifndef SPACETIMEDB_FILTERABLE_VALUE_CONCEPT_H
#define SPACETIMEDB_FILTERABLE_VALUE_CONCEPT_H

#include <type_traits>
#include <concepts>
#include <string>
#include "bsatn/types.h"

// Note: This file should be included within the SpacetimeDb namespace context

/**
 * C++20 concept defining types that can be used as index keys.
 * 
 * A type is filterable if it's one of:
 * - Integral types (including bool)
 * - std::string
 * - Identity, ConnectionId, Timestamp
 * - Simple enums (C-style enums without payloads)
 */
template<typename T>
concept FilterableValue = 
    std::integral<T> ||                    // All integer types + bool
    std::same_as<T, std::string> ||        // String
    std::same_as<T, Identity> ||           // Identity  
    std::same_as<T, ConnectionId> ||       // ConnectionId
    std::same_as<T, Timestamp> ||          // Timestamp
    std::same_as<T, I128> ||               // I128
    std::same_as<T, U128> ||               // U128
    std::is_enum_v<T>;                     // Simple enums

/**
 * Helper to validate constraint at compile time
 */
template<typename TableType, FilterableValue auto member_ptr>
struct ValidateIndexConstraint {
    static constexpr bool value = true;
};

/**
 * Compile-time constraint validation helper
 * Use this to validate that a field can have index constraints
 */
template<typename TableType>
struct ConstraintValidator {
    template<auto member_ptr>
    static constexpr void validate_unique() {
        using FieldType = typename std::remove_reference<decltype(((TableType*)nullptr)->*member_ptr)>::type;
        static_assert(FilterableValue<FieldType>,
            "Field cannot have Unique constraint. "
            "Only integers, bool, std::string, Identity, ConnectionId, Timestamp, "
            "and simple enums can have Unique constraints.");
    }
    
    template<auto member_ptr>
    static constexpr void validate_index() {
        using FieldType = typename std::remove_reference<decltype(((TableType*)nullptr)->*member_ptr)>::type;
        static_assert(FilterableValue<FieldType>,
            "Field cannot have Index constraint. "
            "Only integers, bool, std::string, Identity, ConnectionId, Timestamp, "
            "and simple enums can be indexed.");
    }
    
    template<auto member_ptr>
    static constexpr void validate_primary_key() {
        using FieldType = typename std::remove_reference<decltype(((TableType*)nullptr)->*member_ptr)>::type;
        static_assert(FilterableValue<FieldType>,
            "Field cannot be a PrimaryKey. "
            "Only integers, bool, std::string, Identity, ConnectionId, Timestamp, "
            "and simple enums can be primary keys.");
    }
};

#endif // SPACETIMEDB_FILTERABLE_VALUE_CONCEPT_H