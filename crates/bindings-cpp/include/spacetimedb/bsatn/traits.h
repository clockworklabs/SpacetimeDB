#ifndef SPACETIMEDB_BSATN_TRAITS_H
#define SPACETIMEDB_BSATN_TRAITS_H

#include <type_traits>
#include <concepts>
#include <variant>
#include <optional>
#include <vector>
#include <string>
#include <cctype>

// Then custom headers
#include "algebraic_type.h"
#include "reader.h"
#include "writer.h"
#include "types.h"  // For u128, u256, i128, i256
#include "type_extensions.h"  // For special type constants

namespace SpacetimeDB::bsatn {

// ============================================================================
// CORE TYPES AND CONCEPTS
// ============================================================================

/**
 * @brief C++20 concept checking if a type has BSATN serialization method
 * 
 * **FOR DEVELOPERS:** This concept is satisfied automatically by SPACETIMEDB_STRUCT!
 * You never implement bsatn_serialize() manually.
 * 
 * @section what_devs_write **What You Actually Write:**
 * ```cpp
 * struct MyType {
 *     int value;
 *     std::string name;
 * };
 * SPACETIMEDB_STRUCT(MyType, value, name)  // This generates bsatn_serialize() for you!
 * ```
 * 
 * @section what_macro_generates **What SPACETIMEDB_STRUCT Generates:**
 * The macro creates a bsatn_traits specialization with serialize() that effectively does:
 * ```cpp
 * // Generated code (you don't write this):
 * static void serialize(Writer& w, const MyType& v) {
 *     bsatn::serialize(w, v.value);
 *     bsatn::serialize(w, v.name);
 * }
 * ```
 * 
 * @section for_contributors **For SDK Contributors:**
 * This concept enables the default bsatn_traits implementation to detect if a type
 * has member-based serialization. Used internally by primitive types and special cases.
 * 
 * @tparam T The type to check for serialization capability
 */
template<typename T>
concept HasMemberSerialize = requires(const T& t, Writer& w) {
    { t.bsatn_serialize(w) } -> std::same_as<void>;
};

/**
 * @brief C++20 concept checking if a type has BSATN deserialization method
 * 
 * **FOR DEVELOPERS:** This concept is satisfied automatically by SPACETIMEDB_STRUCT!
 * You never implement bsatn_deserialize() manually.
 * 
 * @section what_devs_write **What You Actually Write:**
 * ```cpp
 * struct MyType {
 *     int value;
 *     std::string name;
 * };
 * SPACETIMEDB_STRUCT(MyType, value, name)  // This generates bsatn_deserialize() for you!
 * ```
 * 
 * @section what_macro_generates **What SPACETIMEDB_STRUCT Generates:**
 * The macro creates a bsatn_traits specialization with deserialize() that effectively does:
 * ```cpp
 * // Generated code (you don't write this):
 * static MyType deserialize(Reader& r) {
 *     MyType v;
 *     v.value = bsatn::deserialize<int>(r);
 *     v.name = bsatn::deserialize<std::string>(r);
 *     return v;
 * }
 * ```
 * 
 * @section for_contributors **For SDK Contributors:**
 * This concept enables the default bsatn_traits implementation to detect if a type
 * has static deserialization. Used internally by primitive types and special cases.
 * 
 * @tparam T The type to check for deserialization capability
 */
template<typename T>
concept HasStaticDeserialize = requires(Reader& r) {
    { T::bsatn_deserialize(r) } -> std::same_as<T>;
};

/**
 * @brief C++20 concept checking if a type has SpacetimeDB schema metadata
 * 
 * **FOR DEVELOPERS:** This concept is satisfied automatically by SPACETIMEDB_STRUCT!
 * The macro generates the algebraic_type_of specialization for you.
 * 
 * @section what_devs_write **What You Actually Write:**
 * ```cpp
 * struct Player {
 *     uint32_t id;
 *     std::string name;
 *     uint32_t score;
 * };
 * SPACETIMEDB_STRUCT(Player, id, name, score)  // Generates schema metadata!
 * SPACETIMEDB_TABLE(Player, players, Public)    // Uses the generated metadata
 * ```
 * 
 * @section what_macro_generates **What SPACETIMEDB_STRUCT Generates:**
 * The macro creates an algebraic_type_of specialization that registers the type
 * with field names and types in SpacetimeDB's schema system.
 * 
 * @section for_contributors **For SDK Contributors:**
 * This concept ensures types can provide SpacetimeDB schema information.
 * The algebraic_type_of template is specialized by SPACETIMEDB_STRUCT and SPACETIMEDB_ENUM.
 * 
 * @tparam T The type to check for schema metadata
 * @note All types in SpacetimeDB tables must satisfy this (automatic with macros)
 */
template<typename T>
concept HasAlgebraicType = requires {
    { algebraic_type_of<T>::get() } -> std::same_as<AlgebraicType>;
};

/**
 * @brief Internal template for BSATN serialization traits
 * 
 * **IMPORTANT FOR DEVELOPERS:** You typically do NOT use this template directly!
 * Instead, use the SPACETIMEDB_STRUCT macro which auto-generates all serialization code.
 * 
 * This template is internal infrastructure that:
 * - Defines how types are serialized/deserialized in BSATN format
 * - Is automatically specialized by SPACETIMEDB_STRUCT and SPACETIMEDB_ENUM macros
 * - Already has specializations for primitives and standard containers (vector, optional, variant)
 * 
 * @tparam T The type to serialize/deserialize
 * 
 * @section user_approach **What Developers Actually Write**
 * 
 * ```cpp
 * // 1. Define your struct
 * struct User {
 *     uint32_t id;
 *     std::string name;
 *     bool is_active;
 * };
 * 
 * // 2. Use SPACETIMEDB_STRUCT macro - this auto-generates EVERYTHING
 * SPACETIMEDB_STRUCT(User, id, name, is_active)
 * ```
 * 
 * The macro automatically generates:
 * - `bsatn_traits<User>::serialize()` method
 * - `bsatn_traits<User>::deserialize()` method  
 * - `bsatn_traits<User>::algebraic_type()` for schema registration
 * - All necessary template specializations
 * 
 * You **never** need to write `bsatn_serialize()` or `bsatn_deserialize()` manually!
 * 
 * @section internals **How It Works (For SDK Contributors)**
 * 
 * The SPACETIMEDB_STRUCT macro expands to a bsatn_traits<T> specialization that:
 * - Calls `bsatn::serialize(writer, obj.field)` for each field in order
 * - Calls `bsatn::deserialize<FieldType>(reader)` for each field in order
 * - Registers the type with LazyTypeRegistrar for circular reference detection
 * 
 * @example What SPACETIMEDB_STRUCT(User, id, name) generates:
 * @code
 * template<> struct bsatn_traits<User> {
 *     static void serialize(Writer& w, const User& v) {
 *         bsatn::serialize(w, v.id);
 *         bsatn::serialize(w, v.name);
 *     }
 *     static User deserialize(Reader& r) {
 *         User v;
 *         v.id = bsatn::deserialize<uint32_t>(r);
 *         v.name = bsatn::deserialize<std::string>(r);
 *         return v;
 *     }
 *     static AlgebraicType algebraic_type() { ...  }
 * };
 * @endcode
 * 
 * @note For third-party types, manually specialize bsatn_traits if needed (advanced use case)
 * @see SPACETIMEDB_STRUCT macro for user-facing API
 * @see SPACETIMEDB_ENUM macro for enum serialization
 */
template<typename T>
struct bsatn_traits {
    static void serialize(Writer& writer, const T& value) {
        if constexpr (HasMemberSerialize<T>) {
            value.bsatn_serialize(writer);
        } else {
            static_assert(sizeof(T) == 0, "Type must implement bsatn_serialize or specialize bsatn_traits");
        }
    }
    
    static T deserialize(Reader& reader) {
        if constexpr (HasStaticDeserialize<T>) {
            return T::bsatn_deserialize(reader);
        } else {
            static_assert(sizeof(T) == 0, "Type must implement bsatn_deserialize or specialize bsatn_traits");
        }
    }
    
    static AlgebraicType algebraic_type() {
        if constexpr (HasAlgebraicType<T>) {
            return SpacetimeDB::bsatn::algebraic_type_of<T>::get();
        } else {
            static_assert(sizeof(T) == 0, "Type must have algebraic_type_of specialization");
        }
    }
};

// Primitive type specializations are in primitive_traits.h
// Special types (u128, u256, i128, i256, Identity, etc.) are in type_extensions.h

// ============================================================================
// BUILDER HELPERS
// ============================================================================

/**
 * Helper for building product types with named fields.
 * Only used by SPACETIMEDB_STRUCT macro.
 */
class ProductTypeBuilder {
private:
    std::vector<SpacetimeDB::bsatn::ProductTypeElement> elements_;
    
public:
    template<typename T>
    ProductTypeBuilder& with_field(const std::string& name) {
        elements_.emplace_back(name, bsatn_traits<T>::algebraic_type());
        return *this;
    }
    
    std::unique_ptr<SpacetimeDB::bsatn::ProductType> build() {
        return std::make_unique<SpacetimeDB::bsatn::ProductType>(std::move(elements_));
    }
};

/**
 * Simplified helper for building sum types (enums).
 * Only used by SPACETIMEDB_ENUM macro.
 */
class SumTypeBuilder {
private:
    std::vector<SpacetimeDB::bsatn::SumTypeVariant> variants_;
    
public:
    SumTypeBuilder& with_unit_variant(const std::string& name) {
        variants_.emplace_back(name, AlgebraicType::Unit());
        return *this;
    }
    
    std::unique_ptr<SpacetimeDB::bsatn::SumTypeSchema> build() {
        return std::make_unique<SpacetimeDB::bsatn::SumTypeSchema>(std::move(variants_));
    }
};

// Special type detection is in type_extensions.h

// ============================================================================
// CONTAINER TYPE TRAITS
// ============================================================================

/**
 * @brief Internal specialization for std::vector<T> serialization
 * 
 * **FOR DEVELOPERS:** You don't call this directly - it's used automatically when
 * you put `std::vector<T>` in a struct field and use SPACETIMEDB_STRUCT.
 * 
 * @section usage **How Developers Use Vectors**
 * 
 * ```cpp
 * struct Player {
 *     uint32_t id;
 *     std::vector<std::string> inventory;  // Vector field
 *     std::vector<uint32_t> scores;        // Another vector field
 * };
 * SPACETIMEDB_STRUCT(Player, id, inventory, scores)  // Auto-handles vectors!
 * 
 * SPACETIMEDB_TABLE(Player, players, Public)
 * ```
 * 
 * The SPACETIMEDB_STRUCT macro automatically handles vector serialization.
 * You never need to manually serialize vectors!
 * 
 * @section nested **Nested Vectors Work Automatically**
 * 
 * ```cpp
 * struct GameBoard {
 *     std::vector<std::vector<int>> grid;  // 2D grid
 * };
 * SPACETIMEDB_STRUCT(GameBoard, grid)  // Just works!
 * ```
 * 
 * @tparam T The element type (must be BSATN-serializable)
 * @note This is SDK infrastructure - use SPACETIMEDB_STRUCT in your code
 */
template<typename T>
struct bsatn_traits<std::vector<T>> {
    static void serialize(Writer& writer, const std::vector<T>& value) {
        writer.write_u32_le(static_cast<uint32_t>(value.size()));
        for (const auto& item : value) {
            SpacetimeDB::bsatn::serialize(writer, item);
        }
    }
    
    static std::vector<T> deserialize(Reader& reader) {
        uint32_t len = reader.read_u32_le();
        std::vector<T> result;
        result.reserve(len);
        for (uint32_t i = 0; i < len; ++i) {
            result.push_back(bsatn_traits<T>::deserialize(reader));
        }
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        // Arrays are ALWAYS inlined, never registered in typespace
        auto elem_type = bsatn_traits<T>::algebraic_type();
        
        // Special types should always be inlined
        if (is_special_type(elem_type)) {
            return AlgebraicType::Array(std::move(elem_type));
        }
        
        // Return Array type INLINE - matches Rust behavior
        return AlgebraicType::Array(std::move(elem_type));
    }
};

/**
 * @brief Internal specialization for std::optional<T> serialization
 * 
 * **FOR DEVELOPERS:** You don't call this directly - it's used automatically when
 * you put `std::optional<T>` in a struct field and use SPACETIMEDB_STRUCT.
 * 
 * @section usage **How Developers Use Optional Fields**
 * 
 * ```cpp
 * struct User {
 *     uint32_t id;
 *     std::string name;
 *     std::optional<std::string> email;  // Optional field
 *     std::optional<std::string> phone;  // Optional field
 * };
 * SPACETIMEDB_STRUCT(User, id, name, email, phone)  // Auto-handles optionals!
 * 
 * SPACETIMEDB_TABLE(User, users, Public)
 * 
 * // In a reducer:
 * SPACETIMEDB_REDUCER(add_user, ReducerContext ctx, std::string name) {
 *     User user{0, name, "alice@example.com", std::nullopt};  // email present, phone absent
 *     ctx.db[users].insert(user);
 * }
 * ```
 * 
 * The SPACETIMEDB_STRUCT macro automatically handles optional serialization.
 * 
 * @section format **Wire Format (For Reference)**
 * 
 * Optionals are serialized as:
 * - 1-byte discriminant: 0 = Some(value), 1 = None
 * - If Some: followed by the serialized value
 * - If None: no additional bytes
 * 
 * @section examples **Examples**
 * 
 * ```cpp
 * std::optional<uint32_t> has_value = 42;
 * // Serialized: [00] [2A 00 00 00]
 * //              ^Some  ^value=42
 * 
 * std::optional<uint32_t> empty;
 * // Serialized: [01]
 * //              ^None
 * ```
 * 
 * @tparam T The wrapped type (must be BSATN-serializable)
 * @note This is SDK infrastructure - use SPACETIMEDB_STRUCT in your code
 */
template<typename T>
struct bsatn_traits<std::optional<T>> {
    static void serialize(Writer& writer, const std::optional<T>& value) {
        if (value.has_value()) {
            writer.write_u8(0); // Some = 0 (SpacetimeDB convention)
            bsatn_traits<T>::serialize(writer, *value);
        } else {
            writer.write_u8(1); // None = 1 (SpacetimeDB convention)
        }
    }
    
    static std::optional<T> deserialize(Reader& reader) {
        uint8_t tag = reader.read_u8();
        if (tag == 0) { // Some
            return bsatn_traits<T>::deserialize(reader);
        } else if (tag == 1) { // None
            return std::nullopt;
        } else {
            std::abort(); // Invalid optional tag in BSATN deserialization
        }
    }
    
    static AlgebraicType algebraic_type() {
        // Options are ALWAYS inlined, never registered as separate types
        auto inner_type = bsatn_traits<T>::algebraic_type();
        
        // Create Option variants: Some(T) and None
        std::vector<SpacetimeDB::bsatn::SumTypeVariant> variants;
        variants.emplace_back("some", std::move(inner_type));
        variants.emplace_back("none", AlgebraicType::Unit());
        
        return AlgebraicType::make_sum(
            std::make_unique<SpacetimeDB::bsatn::SumTypeSchema>(std::move(variants))
        );
    }
};

// ============================================================================
// MONOSTATE (UNIT TYPE) TRAITS
// ============================================================================

/**
 * @brief BSATN traits for std::monostate (Unit type)
 * 
 * std::monostate is used as a placeholder type for unit variants in enums.
 * It serializes as nothing (0 bytes) and always deserializes to the same value.
 * 
 * Example usage:
 * ```cpp
 * SPACETIMEDB_ENUM(Result,
 *     (Ok, uint32_t),
 *     (Err, std::string),
 *     (Empty, Unit)  // Unit is std::monostate
 * )
 * ```
 */
template<>
struct bsatn_traits<std::monostate> {
    static void serialize(Writer& writer, const std::monostate& value) {
        // Unit type serializes as nothing (0 bytes)
        (void)writer;
        (void)value;
    }
    
    static std::monostate deserialize(Reader& reader) {
        // Unit type deserializes as default-constructed monostate
        (void)reader;
        return std::monostate{};
    }
    
    static AlgebraicType algebraic_type() {
        // Unit type in SpacetimeDB
        return AlgebraicType::Unit();
    }
};

// ============================================================================
// VARIANT TYPE TRAITS
// ============================================================================

/**
 * @brief Helper template to find the index of a type within a std::variant
 * 
 * This compile-time utility recursively searches through variant alternatives
 * to find the index position of a specific type T.
 * 
 * @tparam Variant The std::variant type to search
 * @tparam T The type to find within the variant
 * @tparam Index Current search index (defaults to 0)
 * 
 * @example
 * @code
 * using MyVariant = std::variant<int, std::string, double>;
 * static_assert(variant_index_of<MyVariant, std::string>::value == 1);
 * static_assert(variant_index_of<MyVariant, double>::value == 2);
 * @endcode
 */
template<typename Variant, typename T, std::size_t Index = 0>
struct variant_index_of;

template<typename T, typename First, typename... Rest, std::size_t Index>
struct variant_index_of<std::variant<First, Rest...>, T, Index> 
    : variant_index_of<std::variant<Rest...>, T, Index + 1> {};

template<typename T, typename... Rest, std::size_t Index>
struct variant_index_of<std::variant<T, Rest...>, T, Index> 
    : std::integral_constant<std::size_t, Index> {};

/**
 * @brief Internal specialization for std::variant serialization
 * 
 * **FOR DEVELOPERS:** You typically should NOT use std::variant directly in SpacetimeDB!
 * Instead, use **SPACETIMEDB_ENUM** which provides named variants and better ergonomics.
 * 
 * @section recommended **Recommended Approach: Use SPACETIMEDB_ENUM**
 * 
 * ```cpp
 * // DON'T: Use raw std::variant
 * // std::variant<int, std::string> message_data;
 * 
 * // DO: Use SPACETIMEDB_ENUM with named variants
 * SPACETIMEDB_ENUM(MessageData,
 *     (Number, int),
 *     (Text, std::string)
 * )
 * 
 * struct Message {
 *     uint32_t id;
 *     MessageData content;  // Named variants!
 * };
 * SPACETIMEDB_STRUCT(Message, id, content)
 * 
 * // Usage:
 * Message msg{1, std::string("Hello")};
 * if (msg.content.index() == 1) {
 *     auto& text = std::get<std::string>(msg.content.value);
 * }
 * ```
 * 
 * @section fallback **When You Might Use std::variant**
 * 
 * Only use std::variant directly if:
 * - The variant is purely internal (not in table schema)
 * 
 * ```cpp
 * struct InternalState {
 *     std::variant<int, std::string> temp_data;  // OK for internal use
 * };
 * SPACETIMEDB_STRUCT(InternalState, temp_data)  // Auto-handled
 * ```
 * 
 * @section format **Wire Format**
 * 
 * Variants are serialized as:
 * - 1-byte tag (0 = first type, 1 = second type, etc.)
 * - Followed by the serialized value of the active alternative
 * 
 * @tparam Ts... The alternative types (each must be BSATN-serializable)
 * @warning **Type order matters!** Reordering alternatives breaks wire format compatibility
 * @note This is SDK infrastructure - use SPACETIMEDB_ENUM for user-facing code
 * @see SPACETIMEDB_ENUM for the recommended way to define sum types
 */
template<typename... Ts>
struct bsatn_traits<std::variant<Ts...>> {
    using variant_t = std::variant<Ts...>;
    
    static void serialize(Writer& writer, const variant_t& value) {
        writer.write_u8(static_cast<uint8_t>(value.index()));
        std::visit([&writer](const auto& v) {
            using value_type = std::decay_t<decltype(v)>;
            if constexpr (!std::is_same_v<value_type, std::monostate>) {
                SpacetimeDB::bsatn::serialize(writer, v);
            }
        }, value);
    }
    
    static variant_t deserialize(Reader& reader) {
        uint8_t tag = reader.read_u8();
        return deserialize_variant<0>(reader, tag);
    }
    
    static AlgebraicType algebraic_type() {
        std::vector<SpacetimeDB::bsatn::SumTypeVariant> variants;
        build_variants<0, Ts...>(variants);
        return AlgebraicType::make_sum(
            std::make_unique<SpacetimeDB::bsatn::SumTypeSchema>(std::move(variants))
        );
    }
    
private:
    template<std::size_t Index>
    static variant_t deserialize_variant(Reader& reader, uint8_t tag) {
        if constexpr (Index < sizeof...(Ts)) {
            if (tag == Index) {
                using type = std::variant_alternative_t<Index, variant_t>;
                if constexpr (std::is_same_v<type, std::monostate>) {
                    return type{};
                } else {
                    return SpacetimeDB::bsatn::deserialize<type>(reader);
                }
            }
            return deserialize_variant<Index + 1>(reader, tag);
        } else {
            std::abort(); // Invalid variant tag
        }
    }
    
    template<std::size_t Index, typename First, typename... Rest>
    static void build_variants(std::vector<SpacetimeDB::bsatn::SumTypeVariant>& variants) {
        if constexpr (std::is_same_v<First, std::monostate>) {
            variants.emplace_back("variant_" + std::to_string(Index), AlgebraicType::Unit());
        } else {
            variants.emplace_back("variant_" + std::to_string(Index), bsatn_traits<First>::algebraic_type());
        }
        if constexpr (sizeof...(Rest) > 0) {
            build_variants<Index + 1, Rest...>(variants);
        }
    }
};

// ============================================================================
// FIELD REGISTRATION HELPERS
// ============================================================================

// Helper to get the AlgebraicType for field types
template<typename T>
inline AlgebraicType get_field_algebraic_type() {
    return bsatn_traits<T>::algebraic_type();
}

} // namespace SpacetimeDB::bsatn

#endif // SPACETIMEDB_BSATN_TRAITS_H