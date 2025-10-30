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

namespace SpacetimeDb::bsatn {

// ============================================================================
// CORE TYPES AND CONCEPTS
// ============================================================================

/**
 * Concepts for types that can be serialized with BSATN
 */
template<typename T>
concept HasMemberSerialize = requires(const T& t, Writer& w) {
    { t.bsatn_serialize(w) } -> std::same_as<void>;
};

template<typename T>
concept HasStaticDeserialize = requires(Reader& r) {
    { T::bsatn_deserialize(r) } -> std::same_as<T>;
};

template<typename T>
concept HasAlgebraicType = requires {
    { algebraic_type_of<T>::get() } -> std::same_as<AlgebraicType>;
};

/**
 * Primary template for BSATN serialization traits.
 * Specialize this for your types to enable serialization.
 * 
 * The default implementation will:
 * - Call member function bsatn_serialize() if available
 * - Call static function T::bsatn_deserialize() if available
 * - Use algebraic_type_of<T> for type information
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
            return SpacetimeDb::bsatn::algebraic_type_of<T>::get();
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
    std::vector<SpacetimeDb::bsatn::ProductTypeElement> elements_;
    
public:
    template<typename T>
    ProductTypeBuilder& with_field(const std::string& name) {
        elements_.emplace_back(name, bsatn_traits<T>::algebraic_type());
        return *this;
    }
    
    std::unique_ptr<SpacetimeDb::bsatn::ProductType> build() {
        return std::make_unique<SpacetimeDb::bsatn::ProductType>(std::move(elements_));
    }
};

/**
 * Simplified helper for building sum types (enums).
 * Only used by SPACETIMEDB_ENUM macro.
 */
class SumTypeBuilder {
private:
    std::vector<SpacetimeDb::bsatn::SumTypeVariant> variants_;
    
public:
    SumTypeBuilder& with_unit_variant(const std::string& name) {
        variants_.emplace_back(name, AlgebraicType::Unit());
        return *this;
    }
    
    std::unique_ptr<SpacetimeDb::bsatn::SumTypeSchema> build() {
        return std::make_unique<SpacetimeDb::bsatn::SumTypeSchema>(std::move(variants_));
    }
};

// Special type detection is in type_extensions.h

// ============================================================================
// CONTAINER TYPE TRAITS
// ============================================================================

/**
 * BSATN traits for std::vector<T> (all vectors including bytes)
 */
template<typename T>
struct bsatn_traits<std::vector<T>> {
    static void serialize(Writer& writer, const std::vector<T>& value) {
        writer.write_u32_le(static_cast<uint32_t>(value.size()));
        for (const auto& item : value) {
            SpacetimeDb::bsatn::serialize(writer, item);
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
 * BSATN traits for std::optional<T>
 * 
 * IMPORTANT: SpacetimeDB uses a non-standard Option serialization:
 * - Some(value) = discriminant 0 (not 1 as in standard Rust enums)
 * - None = discriminant 1 (not 0 as in standard Rust enums)
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
        std::vector<SpacetimeDb::bsatn::SumTypeVariant> variants;
        variants.emplace_back("some", std::move(inner_type));
        variants.emplace_back("none", AlgebraicType::Unit());
        
        return AlgebraicType::make_sum(
            std::make_unique<SpacetimeDb::bsatn::SumTypeSchema>(std::move(variants))
        );
    }
};

// ============================================================================
// VARIANT TYPE TRAITS
// ============================================================================

// Forward declaration for variant index_sequence helper
template<typename Variant, typename T, std::size_t Index = 0>
struct variant_index_of;

template<typename T, typename First, typename... Rest, std::size_t Index>
struct variant_index_of<std::variant<First, Rest...>, T, Index> 
    : variant_index_of<std::variant<Rest...>, T, Index + 1> {};

template<typename T, typename... Rest, std::size_t Index>
struct variant_index_of<std::variant<T, Rest...>, T, Index> 
    : std::integral_constant<std::size_t, Index> {};

/**
 * BSATN traits for std::variant
 */
template<typename... Ts>
struct bsatn_traits<std::variant<Ts...>> {
    using variant_t = std::variant<Ts...>;
    
    static void serialize(Writer& writer, const variant_t& value) {
        writer.write_u8(static_cast<uint8_t>(value.index()));
        std::visit([&writer](const auto& v) {
            using value_type = std::decay_t<decltype(v)>;
            if constexpr (!std::is_same_v<value_type, std::monostate>) {
                SpacetimeDb::bsatn::serialize(writer, v);
            }
        }, value);
    }
    
    static variant_t deserialize(Reader& reader) {
        uint8_t tag = reader.read_u8();
        return deserialize_variant<0>(reader, tag);
    }
    
    static AlgebraicType algebraic_type() {
        std::vector<SpacetimeDb::bsatn::SumTypeVariant> variants;
        build_variants<0, Ts...>(variants);
        return AlgebraicType::make_sum(
            std::make_unique<SpacetimeDb::bsatn::SumTypeSchema>(std::move(variants))
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
                    return SpacetimeDb::bsatn::deserialize<type>(reader);
                }
            }
            return deserialize_variant<Index + 1>(reader, tag);
        } else {
            std::abort(); // Invalid variant tag
        }
    }
    
    template<std::size_t Index, typename First, typename... Rest>
    static void build_variants(std::vector<SpacetimeDb::bsatn::SumTypeVariant>& variants) {
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

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_TRAITS_H