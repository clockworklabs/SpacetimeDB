#pragma once

#include "spacetimedb/bsatn/traits.h"
#include "spacetimedb/bsatn/sum_type.h"
#include "spacetimedb/macros.h"
#include "spacetimedb/internal/v9_type_registration.h"

/**
 * @file enum_macro.h
 * @brief SpacetimeDB Enum Macros - Clean, efficient enum type generation
 * 
 * Provides unified macros for creating SpacetimeDB-compatible enum types:
 * 
 * Simple Unit Enums:
 *   SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)
 *   → enum class SimpleEnum : uint8_t { Zero = 0, One = 1, Two = 2 };
 * 
 * Complex Variant Enums:
 *   SPACETIMEDB_ENUM(ComplexEnum, (U8, uint8_t), (Str, std::string))
 *   → struct ComplexEnum with std::variant<uint8_t, std::string>
 */

// =============================================================================
// HELPER MACROS FOR VARIANT EXTRACTION
// =============================================================================

// Extract type from (name, type) pair
#define VARIANT_TYPE(pair) VARIANT_TYPE_IMPL pair
#define VARIANT_TYPE_IMPL(name, type) type

// Extract name from (name, type) pair  
#define VARIANT_NAME(pair) VARIANT_NAME_IMPL pair
#define VARIANT_NAME_IMPL(name, type) #name

// Separator macros
#define COMMA() ,
#define EMPTY()

// =============================================================================
// UNIFIED ENUM SYSTEM
// =============================================================================

/**
 * @brief SPACETIMEDB_ENUM - Unified enum macro with automatic syntax detection
 * 
 * Simple unit enums:
 *   SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)
 *   → enum class SimpleEnum : uint8_t { Zero = 0, One = 1, Two = 2 };
 * 
 * Complex variant enums:
 *   SPACETIMEDB_ENUM(ComplexEnum, (U8, uint8_t), (Str, std::string))
 *   → struct ComplexEnum with std::variant<uint8_t, std::string>
 */

// Detect if first argument is parenthesized (name, type) pair
#define SPACETIMEDB_IS_PARENTHESIZED(x) SPACETIMEDB_IS_PARENTHESIZED_IMPL(SPACETIMEDB_IS_PARENTHESIZED_PROBE x)
#define SPACETIMEDB_IS_PARENTHESIZED_IMPL(...) SPACETIMEDB_IS_PARENTHESIZED_GET_SECOND(__VA_ARGS__, 0)
#define SPACETIMEDB_IS_PARENTHESIZED_GET_SECOND(a, b, ...) b
#define SPACETIMEDB_IS_PARENTHESIZED_PROBE(...) ~, 1

// Route to appropriate implementation
#define SPACETIMEDB_ENUM(EnumName, ...) \
    SPACETIMEDB_CONCAT(SPACETIMEDB_ENUM_, \
        SPACETIMEDB_IS_PARENTHESIZED(SPACETIMEDB_FIRST_ARG(__VA_ARGS__))) \
    (EnumName, __VA_ARGS__)

#define SPACETIMEDB_ENUM_0 SPACETIMEDB_ENUM_SIMPLE
#define SPACETIMEDB_ENUM_1 SPACETIMEDB_ENUM_VARIANT
#define SPACETIMEDB_FIRST_ARG(first, ...) first

// =============================================================================
// SIMPLE ENUM IMPLEMENTATION (Unit Variants)
// =============================================================================

/**
 * @brief Creates efficient enum class with uint8_t underlying type
 * 
 * Generates named Sum type variants for SpacetimeDB compatibility.
 * Used internally by SPACETIMEDB_ENUM for simple syntax.
 */
#define SPACETIMEDB_ENUM_SIMPLE(EnumName, ...) \
    enum class EnumName : uint8_t { \
        SPACETIMEDB_ENUM_ASSIGN_VALUES(__VA_ARGS__) \
    }; \
    \
    namespace SpacetimeDb::bsatn { \
    template<> \
    struct bsatn_traits<EnumName> { \
        static AlgebraicType algebraic_type() { \
            return SpacetimeDb::Internal::LazyTypeRegistrar<EnumName>::getOrRegister( \
                []() -> AlgebraicType { \
                    SumTypeBuilder builder; \
                    SPACETIMEDB_ENUM_ADD_VARIANTS(__VA_ARGS__) \
                    return AlgebraicType::make_sum(builder.build()); \
                }, \
                #EnumName \
            ); \
        } \
        \
        static void serialize(Writer& writer, const EnumName& value) { \
            writer.write_u8(static_cast<uint8_t>(value)); \
        } \
        \
        static EnumName deserialize(Reader& reader) { \
            uint8_t tag = reader.read_u8(); \
            return static_cast<EnumName>(tag); \
        } \
    }; \
    template<> \
    struct algebraic_type_of<EnumName> { \
        static AlgebraicType get() { \
            return bsatn_traits<EnumName>::algebraic_type(); \
        } \
    }; \
    } \
    \
    SPACETIMEDB_GENERATE_EMPTY_FIELD_REGISTRAR(EnumName)

// Auto-assign enum values (0, 1, 2, etc.) - supports up to 10 variants
#define SPACETIMEDB_ENUM_ASSIGN_VALUES(...) \
    SPACETIMEDB_ENUM_DISPATCH_COUNT(__VA_ARGS__, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1)(__VA_ARGS__)

#define SPACETIMEDB_ENUM_DISPATCH_COUNT(_1,_2,_3,_4,_5,_6,_7,_8,_9,_10,N,...) SPACETIMEDB_ENUM_ASSIGN_##N

#define SPACETIMEDB_ENUM_ASSIGN_1(a) a = 0
#define SPACETIMEDB_ENUM_ASSIGN_2(a, b) a = 0, b = 1
#define SPACETIMEDB_ENUM_ASSIGN_3(a, b, c) a = 0, b = 1, c = 2
#define SPACETIMEDB_ENUM_ASSIGN_4(a, b, c, d) a = 0, b = 1, c = 2, d = 3
#define SPACETIMEDB_ENUM_ASSIGN_5(a, b, c, d, e) a = 0, b = 1, c = 2, d = 3, e = 4
#define SPACETIMEDB_ENUM_ASSIGN_6(a, b, c, d, e, f) a = 0, b = 1, c = 2, d = 3, e = 4, f = 5
#define SPACETIMEDB_ENUM_ASSIGN_7(a, b, c, d, e, f, g) a = 0, b = 1, c = 2, d = 3, e = 4, f = 5, g = 6
#define SPACETIMEDB_ENUM_ASSIGN_8(a, b, c, d, e, f, g, h) a = 0, b = 1, c = 2, d = 3, e = 4, f = 5, g = 6, h = 7
#define SPACETIMEDB_ENUM_ASSIGN_9(a, b, c, d, e, f, g, h, i) a = 0, b = 1, c = 2, d = 3, e = 4, f = 5, g = 6, h = 7, i = 8
#define SPACETIMEDB_ENUM_ASSIGN_10(a, b, c, d, e, f, g, h, i, j) a = 0, b = 1, c = 2, d = 3, e = 4, f = 5, g = 6, h = 7, i = 8, j = 9

// Register variants in SumTypeBuilder
#define SPACETIMEDB_ENUM_ADD_VARIANTS(...) \
    SPACETIMEDB_FOR_EACH_VARIANT(SPACETIMEDB_ENUM_ADD_UNIT_VARIANT, EMPTY, __VA_ARGS__)

#define SPACETIMEDB_ENUM_ADD_UNIT_VARIANT(name) \
    builder.with_unit_variant(#name);

// =============================================================================
// VARIANT ENUM IMPLEMENTATION (Data Variants)
// =============================================================================

/**
 * @brief Creates flexible std::variant struct with named variants
 * 
 * Supports data-carrying variants with explicit type specifications.
 * Used internally by SPACETIMEDB_ENUM for complex syntax.
 */
#define SPACETIMEDB_ENUM_VARIANT(EnumName, ...) \
    struct EnumName { \
        using variant_type = std::variant<SPACETIMEDB_ENUM_VARIANT_TYPES(__VA_ARGS__)>; \
        variant_type value; \
        \
        static constexpr const char* variant_names[] = { \
            FOR_EACH_VARIANT(VARIANT_NAME, COMMA, __VA_ARGS__) \
        }; \
        \
        EnumName() = default; \
        EnumName(const variant_type& v) : value(v) {} \
        EnumName(variant_type&& v) : value(std::move(v)) {} \
        \
        template<typename T, typename = std::enable_if_t< \
            !std::is_same_v<std::decay_t<T>, EnumName> && \
            !std::is_same_v<std::decay_t<T>, variant_type>>> \
        EnumName(T&& t) : value(std::forward<T>(t)) {} \
        \
        size_t index() const { return value.index(); } \
        const char* variant_name() const { \
            return (index() < sizeof(variant_names)/sizeof(variant_names[0])) ? \
                variant_names[index()] : "unknown"; \
        } \
    }; \
    \
    namespace SpacetimeDb::bsatn { \
    template<> \
    struct bsatn_traits<EnumName> { \
        static AlgebraicType algebraic_type() { \
            return SpacetimeDb::Internal::LazyTypeRegistrar<EnumName>::getOrRegister( \
                []() -> AlgebraicType { \
                    std::vector<SumTypeVariant> variants; \
                    SpacetimeDb::named_variant_helper<0, SPACETIMEDB_ENUM_VARIANT_TYPES(__VA_ARGS__)>::add_variants( \
                        variants, EnumName::variant_names); \
                    auto sum_type = std::make_unique<SumTypeSchema>(std::move(variants)); \
                    return AlgebraicType::make_sum(std::move(sum_type)); \
                }, \
                #EnumName \
            ); \
        } \
        \
        static void serialize(Writer& writer, const EnumName& enum_value) { \
            writer.write_u8(static_cast<uint8_t>(enum_value.value.index())); \
            std::visit([&](const auto& v) { \
                SpacetimeDb::bsatn::serialize(writer, v); \
            }, enum_value.value); \
        } \
        \
        static EnumName deserialize(Reader& reader) { \
            uint8_t tag = reader.read_u8(); \
            return EnumName{SpacetimeDb::named_variant_helper<0, SPACETIMEDB_ENUM_VARIANT_TYPES(__VA_ARGS__)>:: \
                template deserialize_variant<typename EnumName::variant_type>(tag, reader)}; \
        } \
    }; \
    template<> \
    struct algebraic_type_of<EnumName> { \
        static AlgebraicType get() { \
            return bsatn_traits<EnumName>::algebraic_type(); \
        } \
    }; \
    } \
    \
    SPACETIMEDB_GENERATE_EMPTY_FIELD_REGISTRAR(EnumName)

// Extract variant types from (name, type) pairs
#define SPACETIMEDB_ENUM_VARIANT_TYPES(...) FOR_EACH_VARIANT(VARIANT_TYPE, COMMA, __VA_ARGS__)

// Type alias for unit variants (no data)
using Unit = std::monostate;

// =============================================================================
// VARIANT HELPER TEMPLATES
// =============================================================================

namespace SpacetimeDb {

/**
 * @brief Recursive helper for variant type processing
 * 
 * Builds variant lists with compile-time names and handles serialization.
 * Used internally by SPACETIMEDB_ENUM_VARIANT.
 */
template<size_t I, typename... Types>
struct named_variant_helper;

// Recursion base case
template<size_t I>
struct named_variant_helper<I> {
    static void add_variants(std::vector<bsatn::SumTypeVariant>& variants, 
                            const char* const* names) {
        (void)variants; (void)names; // End recursion
    }
    
    template<typename Variant>
    static Variant deserialize_variant(size_t index, bsatn::Reader& reader) {
        (void)index;
        (void)reader;
        std::abort(); // Invalid variant index
    }
};

// Recursive case
template<size_t I, typename T, typename... Rest>
struct named_variant_helper<I, T, Rest...> {
    static void add_variants(std::vector<bsatn::SumTypeVariant>& variants,
                            const char* const* names) {
        const char* name = names[I];
        
        // Trigger type registration if needed (bottom-up dependency resolution)
        bsatn::AlgebraicType variant_type = bsatn::bsatn_traits<T>::algebraic_type();
        
        variants.emplace_back(name, std::move(variant_type));
        
        // Continue with remaining types
        named_variant_helper<I + 1, Rest...>::add_variants(variants, names);
    }
    
    template<typename Variant>
    static Variant deserialize_variant(size_t index, bsatn::Reader& reader) {
        if (index == I) {
            return Variant{bsatn::deserialize<T>(reader)};
        } else {
            return named_variant_helper<I + 1, Rest...>::template deserialize_variant<Variant>(index, reader);
        }
    }
};

} // namespace SpacetimeDb

// =============================================================================
// COMPILE-TIME NAMESPACE STORAGE
// =============================================================================

namespace SpacetimeDb::detail {
    // Template to store namespace information for types
    template<typename T>
    struct namespace_info {
        static constexpr const char* value = nullptr;
    };
}

/**
 * @brief Add namespace qualification to an existing enum type
 * 
 * This macro creates a template specialization that stores the namespace
 * information at compile time. When the enum is registered, the LazyTypeRegistrar
 * will check for this namespace information and use it.
 * 
 * @param EnumType The C++ enum class type (already defined with SPACETIMEDB_ENUM)
 * @param NamespacePrefix The namespace prefix as a string literal
 * 
 * @example
 * SPACETIMEDB_ENUM(TestC, Foo, Bar)           // Define the enum normally
 * SPACETIMEDB_NAMESPACE(TestC, "Namespace")   // Add namespace qualification
 * 
 * This stores the namespace info at compile time for use during registration.
 */
#define SPACETIMEDB_NAMESPACE(EnumType, NamespacePrefix) \
    namespace SpacetimeDb::detail { \
        template<> \
        struct namespace_info<EnumType> { \
            static constexpr const char* value = NamespacePrefix; \
        }; \
    }


