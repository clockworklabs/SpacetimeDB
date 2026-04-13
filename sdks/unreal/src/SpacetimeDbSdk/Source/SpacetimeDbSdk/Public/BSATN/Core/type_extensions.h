#ifndef SPACETIMEDB_BSATN_TYPE_EXTENSIONS_H
#define SPACETIMEDB_BSATN_TYPE_EXTENSIONS_H

/**
 * @file type_extensions.h
 * @brief Extended type support for SpacetimeDB BSATN serialization
 * 
 * This header combines functionality from special_types.h and extended_types.h
 * It provides:
 * - Special type tags and identification functions
 * - BSATN trait specializations for extended types
 * - Support for large integers, container types, and SpacetimeDB core types
 */

namespace SpacetimeDb::bsatn {

// =============================================================================
// SPECIAL TYPE CONSTANTS - Must be defined early for use in other headers
// =============================================================================

// Special type field tags
constexpr const char* IDENTITY_TAG = "__identity__";
constexpr const char* CONNECTION_ID_TAG = "__connection_id__";
constexpr const char* TIMESTAMP_TAG = "__timestamp_micros_since_unix_epoch__";
constexpr const char* TIME_DURATION_TAG = "__time_duration_micros__";

} // namespace SpacetimeDb::bsatn

// Now include headers that may use these constants
#include "algebraic_type.h"
#include "types.h"
#include "timestamp.h"
#include "time_duration.h"
#include <string_view>
#include <algorithm>
#include <optional>
#include <vector>

namespace SpacetimeDb::bsatn {

// Forward declarations
class Reader;
class Writer;
template<typename T> struct bsatn_traits;

// Helper to detect optional types
template<typename T>
struct is_optional : std::false_type {};
template<typename T>
struct is_optional<std::optional<T>> : std::true_type {};

// Helper to detect vector types
template<typename T>
struct is_vector : std::false_type {};
template<typename T>
struct is_vector<std::vector<T>> : std::true_type {};

// Helper to get primitive type tags
template<typename T>
constexpr uint32_t get_primitive_type_tag() {
    if constexpr (std::is_same_v<T, bool>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::Bool);
    } else if constexpr (std::is_same_v<T, uint8_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::U8);
    } else if constexpr (std::is_same_v<T, uint16_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::U16);
    } else if constexpr (std::is_same_v<T, uint32_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::U32);
    } else if constexpr (std::is_same_v<T, uint64_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::U64);
    } else if constexpr (std::is_same_v<T, int8_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::I8);
    } else if constexpr (std::is_same_v<T, int16_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::I16);
    } else if constexpr (std::is_same_v<T, int32_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::I32);
    } else if constexpr (std::is_same_v<T, int64_t>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::I64);
    } else if constexpr (std::is_same_v<T, float>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::F32);
    } else if constexpr (std::is_same_v<T, double>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::F64);
    } else if constexpr (std::is_same_v<T, std::string>) {
        return static_cast<uint32_t>(AlgebraicTypeTag::String);
    } else {
        return 0; // Not a primitive type
    }
}

// =============================================================================
// SPECIAL TYPE TAGS AND IDENTIFICATION
// =============================================================================

/**
 * Special type tags used by SpacetimeDB to identify built-in types.
 * These match the Rust implementation exactly.
 * Note: Constants are defined at the top of this file before includes.
 */

/**
 * Enumeration of special types recognized by SpacetimeDB.
 */
enum class SpecialTypeKind {
    None,
    Identity,
    ConnectionId,
    Timestamp,
    TimeDuration,
    Unit,        // Empty Product type
    Never,       // Empty Sum type
    ScheduleAt,  // Sum with Interval and Time variants
    Option       // Sum with some and none variants
};

/**
 * Check if a ProductType represents a special SpacetimeDB type.
 * Special types include field-tagged types and structural types.
 */
inline bool is_special_product_type(const ProductType& product) {
    // Unit type: empty product
    if (product.elements.size() == 0) {
        return true;
    }
    
    // Field-tagged special types
    if (product.elements.size() == 1 && product.elements[0].name.has_value()) {
        const std::string& tag = *product.elements[0].name;
        return tag == IDENTITY_TAG ||
               tag == CONNECTION_ID_TAG ||
               tag == TIMESTAMP_TAG ||
               tag == TIME_DURATION_TAG;
    }
    
    return false;
}

/**
 * Check if a SumType represents a special SpacetimeDB type.
 */
inline bool is_special_sum_type(const SumTypeSchema& sum) {
    // Never type: empty sum
    if (sum.variants.size() == 0) {
        return true;
    }
    
    // Two-variant sum types: ScheduleAt and Option
    if (sum.variants.size() == 2) {
        bool has_interval = false, has_time = false;
        bool has_some = false, has_none = false;
        
        for (const auto& variant : sum.variants) {
            if (variant.name == "Interval") has_interval = true;
            else if (variant.name == "Time") has_time = true;
            else if (variant.name == "some") has_some = true;
            else if (variant.name == "none") has_none = true;
        }
        
        // ScheduleAt type: sum with Interval and Time variants
        if (has_interval && has_time) {
            return true;
        }
        
        // Option type: sum with some and none variants
        if (has_some && has_none) {
            return true;
        }
    }
    
    return false;
}

/**
 * Check if an AlgebraicType represents a special SpacetimeDB type.
 */
inline bool is_special_type(const AlgebraicType& type) {
    if (type.tag() == AlgebraicTypeTag::Product) {
        return is_special_product_type(type.as_product());
    } else if (type.tag() == AlgebraicTypeTag::Sum) {
        return is_special_sum_type(type.as_sum());
    }
    return false;
}


/**
 * Get the kind of special type represented by an AlgebraicType.
 */
inline SpecialTypeKind get_special_type_kind(const AlgebraicType& type) {
    if (type.tag() == AlgebraicTypeTag::Product) {
        const auto& product = type.as_product();
        
        // Unit type
        if (product.elements.size() == 0) {
            return SpecialTypeKind::Unit;
        }
        
        // Field-tagged types
        if (product.elements.size() == 1 && product.elements[0].name.has_value()) {
            const std::string& tag = *product.elements[0].name;
            // Use string_view comparison for more robust matching
            if (tag == IDENTITY_TAG) return SpecialTypeKind::Identity;
            if (tag == CONNECTION_ID_TAG) return SpecialTypeKind::ConnectionId;
            if (tag == TIMESTAMP_TAG) return SpecialTypeKind::Timestamp;
            if (tag == TIME_DURATION_TAG) return SpecialTypeKind::TimeDuration;
            
            // Fallback: Check for exact field name matches in case of string issues
            if (tag == "__identity__") return SpecialTypeKind::Identity;
            if (tag == "__connection_id__") return SpecialTypeKind::ConnectionId;
            if (tag == "__timestamp_micros_since_unix_epoch__") return SpecialTypeKind::Timestamp;
            if (tag == "__time_duration_micros__") return SpecialTypeKind::TimeDuration;
        }
    } else if (type.tag() == AlgebraicTypeTag::Sum) {
        const auto& sum = type.as_sum();
        
        // Never type
        if (sum.variants.size() == 0) {
            return SpecialTypeKind::Never;
        }
        
        // Two-variant sum types: ScheduleAt and Option
        if (sum.variants.size() == 2) {
            bool has_interval = false, has_time = false;
            bool has_some = false, has_none = false;
            
            for (const auto& variant : sum.variants) {
                if (variant.name == "Interval") has_interval = true;
                else if (variant.name == "Time") has_time = true;
                else if (variant.name == "some") has_some = true;
                else if (variant.name == "none") has_none = true;
            }
            
            // ScheduleAt type
            if (has_interval && has_time) {
                return SpecialTypeKind::ScheduleAt;
            }
            
            // Option type
            if (has_some && has_none) {
                return SpecialTypeKind::Option;
            }
        }
    }
    
    return SpecialTypeKind::None;
}

/**
 * Create a special type ProductType with the given tag and data type.
 */
inline std::unique_ptr<ProductType> make_special_type(const char* tag, AlgebraicType data_type) {
    std::vector<ProductTypeElement> elements;
    elements.emplace_back(tag, std::move(data_type));
    return std::make_unique<ProductType>(std::move(elements));
}

/**
 * @brief Factory functions for SpacetimeDB special types.
 * 
 * These types are represented as ProductTypes with a single specially-tagged field.
 * The tag identifies the semantic meaning of the type.
 */
namespace special_types {
    
    /**
     * @brief Create an Identity type (256-bit identifier).
     * 
     * Identity is represented as an array of 32 bytes (U8[32]).
     * 
     * @return AlgebraicType representing an Identity
     * @todo Integrate with type registry for proper type indexing
     */
    inline AlgebraicType identity() {
        // Identity uses U256 as inner type (matches Rust implementation)
        // CRITICAL: Pass the actual U256 type, not a Ref to it - special types are inlined!
        auto product = make_special_type(IDENTITY_TAG, AlgebraicType::U256());
        return AlgebraicType::make_product(std::move(product));
    }
    
    /**
     * @brief Create a ConnectionId type (64-bit connection identifier).
     * 
     * @return AlgebraicType representing a ConnectionId
     * @todo Integrate with type registry for proper type indexing
     */
    inline AlgebraicType connection_id() {
        // ConnectionId uses U128 as inner type (matches Rust implementation)
        // CRITICAL: Pass the actual U128 type, not a Ref to it - special types are inlined!
        auto product = make_special_type(CONNECTION_ID_TAG, AlgebraicType::U128());
        return AlgebraicType::make_product(std::move(product));
    }
    
    /**
     * @brief Create a Timestamp type (microseconds since Unix epoch).
     * 
     * @return AlgebraicType representing a Timestamp
     * @todo Integrate with type registry for proper type indexing
     */
    inline AlgebraicType timestamp() {
        // Timestamp uses I64 as inner type (matches Rust implementation)
        // CRITICAL: Pass the actual I64 type, not a Ref to it - special types are inlined!
        auto product = make_special_type(TIMESTAMP_TAG, AlgebraicType::I64());
        return AlgebraicType::make_product(std::move(product));
    }
    
    /**
     * @brief Create a TimeDuration type (duration in microseconds).
     * 
     * @return AlgebraicType representing a TimeDuration
     * @todo Integrate with type registry for proper type indexing
     */
    inline AlgebraicType time_duration() {
        // TimeDuration uses I64 as inner type (matches Rust implementation)
        // CRITICAL: Pass the actual I64 type, not a Ref to it - special types are inlined!
        auto product = make_special_type(TIME_DURATION_TAG, AlgebraicType::I64());
        return AlgebraicType::make_product(std::move(product));
    }
    
} // namespace special_types

// =============================================================================
// LARGE INTEGER TYPES (u128, i128)
// =============================================================================

/**
 * BSATN serialization for 128-bit unsigned integer
 */
template<>
struct bsatn_traits<::SpacetimeDb::u128> {
    static void serialize(Writer& writer, const ::SpacetimeDb::u128& value) {
        // Serialize as 16 bytes in little-endian order
        writer.write_u64_le(value.low);
        writer.write_u64_le(value.high);
    }
    
    static ::SpacetimeDb::u128 deserialize(Reader& reader) {
        uint64_t low = reader.read_u64_le();
        uint64_t high = reader.read_u64_le();
        return ::SpacetimeDb::u128(high, low);
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U128();
    }
};

/**
 * BSATN serialization for 128-bit signed integer
 */
template<>
struct bsatn_traits<::SpacetimeDb::i128> {
    static void serialize(Writer& writer, const ::SpacetimeDb::i128& value) {
        // Serialize as 16 bytes in little-endian order
        writer.write_u64_le(value.low);
        writer.write_u64_le(static_cast<uint64_t>(value.high));
    }
    
    static ::SpacetimeDb::i128 deserialize(Reader& reader) {
        uint64_t low = reader.read_u64_le();
        uint64_t high = reader.read_u64_le();
        return ::SpacetimeDb::i128(static_cast<int64_t>(high), low);
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I128();
    }
};

// u256 and i256 also need bsatn_traits specializations that call their methods

/**
 * BSATN serialization for 256-bit unsigned integer
 */
template<>
struct bsatn_traits<::SpacetimeDb::u256> {
    static void serialize(Writer& writer, const ::SpacetimeDb::u256& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::u256 deserialize(Reader& reader) {
        ::SpacetimeDb::u256 result;
        result.bsatn_deserialize(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U256();
    }
};

/**
 * BSATN serialization for 256-bit signed integer
 */
template<>
struct bsatn_traits<::SpacetimeDb::i256> {
    static void serialize(Writer& writer, const ::SpacetimeDb::i256& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::i256 deserialize(Reader& reader) {
        ::SpacetimeDb::i256 result;
        result.bsatn_deserialize(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I256();
    }
};

// =============================================================================
// SPACETIMEDB CORE TYPES (Identity, ConnectionId, Timestamp, TimeDuration)
// =============================================================================

/**
 * BSATN serialization for Identity
 */
template<>
struct bsatn_traits<::SpacetimeDb::Identity> {
    static void serialize(Writer& writer, const ::SpacetimeDb::Identity& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::Identity deserialize(Reader& reader) {
        ::SpacetimeDb::Identity result;
        result.bsatn_deserialize(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return special_types::identity();
    }
};

/**
 * BSATN serialization for ConnectionId
 */
template<>
struct bsatn_traits<::SpacetimeDb::ConnectionId> {
    static void serialize(Writer& writer, const ::SpacetimeDb::ConnectionId& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::ConnectionId deserialize(Reader& reader) {
        ::SpacetimeDb::ConnectionId result;
        result.bsatn_deserialize(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return special_types::connection_id();
    }
};

/**
 * BSATN serialization for Timestamp
 */
template<>
struct bsatn_traits<::SpacetimeDb::Timestamp> {
    static void serialize(Writer& writer, const ::SpacetimeDb::Timestamp& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::Timestamp deserialize(Reader& reader) {
        return ::SpacetimeDb::Timestamp::bsatn_deserialize(reader);
    }
    
    static AlgebraicType algebraic_type() {
        return special_types::timestamp();
    }
};

/**
 * BSATN serialization for TimeDuration
 */
template<>
struct bsatn_traits<::SpacetimeDb::TimeDuration> {
    static void serialize(Writer& writer, const ::SpacetimeDb::TimeDuration& value) {
        value.bsatn_serialize(writer);
    }
    
    static ::SpacetimeDb::TimeDuration deserialize(Reader& reader) {
        return ::SpacetimeDb::TimeDuration::bsatn_deserialize(reader);
    }
    
    static AlgebraicType algebraic_type() {
        return special_types::time_duration();
    }
};

// =============================================================================
// CONTAINER TYPES
// =============================================================================

// Note: bsatn_traits specializations for std::optional<T> and std::vector<T>
// are now defined in traits.h (as of this consolidation fix).
// This file contains the special type trait definitions and helpers.

// Convenience type aliases for SpacetimeDB vectors
using VecTimeDuration = std::vector<::SpacetimeDb::TimeDuration>;

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_TYPE_EXTENSIONS_H