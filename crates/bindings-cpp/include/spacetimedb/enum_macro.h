#pragma once

#include "spacetimedb/bsatn/traits.h"
#include "spacetimedb/bsatn/sum_type.h"
#include "spacetimedb/macros.h"
#include "spacetimedb/internal/module_type_registration.h"

/**
 * @file enum_macro.h
 * @brief Unified SpacetimeDB Enum Macros for C++ bindings
 * 
 * Provides SPACETIMEDB_ENUM macro with automatic syntax detection to create
 * SpacetimeDB-compatible enum types. The macro supports two distinct patterns:
 * 
 * **1. Simple Unit Enums** (C-style enums with named constants):
 * ```cpp
 * SPACETIMEDB_ENUM(Direction, North, South, East, West)
 * ```
 * Generated code:
 * - `enum class Direction : uint8_t { North = 0, South = 1, East = 2, West = 3 }`
 * - Efficient: 1 byte per value
 * - Use when: Variants carry no data
 * 
 * **2. Variant Enums** (Rust-style enums with associated data):
 * ```cpp
 * SPACETIMEDB_ENUM(Result, (Ok, uint32_t), (Err, std::string))
 * ```
 * Generated code:
 * - `struct Result` with `std::variant<uint32_t, std::string>`
 * - Tagged union with named accessors
 * - Use when: Variants carry different types of data
 * 
 * @section syntax_detection Automatic Syntax Detection
 * 
 * The macro automatically detects which pattern you're using:
 * - Simple enum: `SPACETIMEDB_ENUM(Name, Variant1, Variant2, ...)`
 * - Variant enum: `SPACETIMEDB_ENUM(Name, (Variant1, Type1), (Variant2, Type2), ...)`
 * 
 * The presence of parentheses `(name, type)` triggers variant enum generation.
 * 
 * @section simple_examples Simple Enum Examples
 * 
 * ```cpp
 * // Game state enum
 * SPACETIMEDB_ENUM(GameState, Lobby, Playing, Paused, Ended)
 * 
 * // Usage:
 * GameState state = GameState::Playing;
 * if (state == GameState::Lobby) { ... }
 * ```
 * 
 * ```cpp
 * // Boolean-like enum
 * SPACETIMEDB_ENUM(Status, Active, Inactive)
 * ```
 * 
 * @section variant_examples Variant Enum Examples
 * 
 * ```cpp
 * // Message types with different payloads
 * SPACETIMEDB_ENUM(Message,
 *     (Text, std::string),
 *     (Image, std::vector<uint8_t>),
 *     (Audio, std::vector<uint8_t>)
 * )
 * 
 * // Usage:
 * Message msg = std::string("Hello");  // Implicit construction
 * if (msg.index() == 0) {
 *     auto& text = std::get<std::string>(msg.value);
 * }
 * ```
 * 
 * ```cpp
 * // Result type with success/error variants
 * SPACETIMEDB_ENUM(ApiResult,
 *     (Success, UserData),
 *     (NotFound, Unit),
 *     (Error, std::string)
 * )
 * ```
 * 
 * @section namespace_qual Namespace Qualification
 * 
 * For type reuse across modules, add namespace prefixes:
 * 
 * ```cpp
 * SPACETIMEDB_ENUM(Status, Active, Inactive)
 * SPACETIMEDB_NAMESPACE(Status, "MyModule")  // Registers as "MyModule.Status"
 * ```
 * 
 * @note Simple enums are more efficient (1 byte) than variant enums (std::variant overhead)
 * @warning Variant enum alternative order is part of the wire format - don't reorder!
 * 
 * @ingroup sdk_macros
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
 * @brief Unified enum macro with automatic syntax detection
 * 
 * These macros automatically detect whether you're defining a simple unit enum
 * or a variant enum based on the syntax used, then routes to the appropriate
 * implementation.
 * 
 * @param EnumName The name of the enum type to create
 * @param ... Either:
 *   - Simple: List of variant names (e.g., `Red, Green, Blue`)
 *   - Variant: List of (name, type) pairs (e.g., `(Ok, int), (Err, std::string)`)
 * 
 * **Decision Tree:**
 * 
 * Q: Do your variants carry data?
 * - **No** → Use simple syntax: `SPACETIMEDB_ENUM(Name, Var1, Var2, ...)`
 *   - Generated: `enum class Name : uint8_t`
 *   - Size: 1 byte
 *   - Example: `SPACETIMEDB_ENUM(Color, Red, Green, Blue)`
 * 
 * - **Yes** → Use variant syntax: `SPACETIMEDB_ENUM(Name, (Var1, Type1), (Var2, Type2), ...)`
 *   - Generated: `struct Name` with `std::variant<Type1, Type2, ...>`
 *   - Size: sizeof(largest type) + discriminant
 *   - Example: `SPACETIMEDB_ENUM(Result, (Ok, uint32_t), (Err, std::string))`
 * 
 * @example Simple enum (no data):
 * @code
 * // Define direction enum
 * SPACETIMEDB_ENUM(Direction, North, South, East, West)
 * 
 * // Define table struct that uses the enum
 * struct Player {
 *     uint32_t id;
 *     std::string name;
 *     Direction facing;
 * };
 * SPACETIMEDB_STRUCT(Player, id, name, facing)
 * SPACETIMEDB_TABLE(Player, players, Public)
 * FIELD_PrimaryKey(players, id)
 * 
 * // Usage in reducer
 * SPACETIMEDB_REDUCER(void, move_player, ReducerContext ctx, Direction dir) {
 *     if (dir == Direction::North) {
 *         // Move north
 *     }
 * }
 * @endcode
 * 
 * @example Variant enum (with data):
 * @code
 * // Define event enum with different payload types
 * SPACETIMEDB_ENUM(GameEvent,
 *     (PlayerJoined, uint32_t),      // player_id
 *     (ChatMessage, std::string),     // message text
 *     (PlayerLeft, Unit)              // no data
 * )
 * 
 * // Define table struct that uses the enum
 * struct EventLog {
 *     uint32_t id;
 *     Timestamp timestamp;
 *     GameEvent event;
 * };
 * SPACETIMEDB_STRUCT(EventLog, id, timestamp, event)
 * SPACETIMEDB_TABLE(EventLog, events, Public)
 * FIELD_PrimaryKeyAutoInc(events, id)
 * 
 * // Usage in reducer
 * SPACETIMEDB_REDUCER(void, log_event, ReducerContext ctx, GameEvent event) {
 *     if (event.index() == 0) {  // PlayerJoined
 *         auto player_id = std::get<uint32_t>(event.value);
 *         LOG_INFO("Player " + std::to_string(player_id) + " joined");
 *     }
 * }
 * @endcode
 * 
 * @note The macro uses compile-time detection to determine which implementation to use
 * @note Simple enums serialize as 1 byte; variant enums serialize as tag + data
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
    namespace SpacetimeDB::bsatn { \
    template<> \
    struct bsatn_traits<EnumName> { \
        static AlgebraicType algebraic_type() { \
            return SpacetimeDB::Internal::LazyTypeRegistrar<EnumName>::getOrRegister( \
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
    namespace SpacetimeDB::bsatn { \
    template<> \
    struct bsatn_traits<EnumName> { \
        static AlgebraicType algebraic_type() { \
            return SpacetimeDB::Internal::LazyTypeRegistrar<EnumName>::getOrRegister( \
                []() -> AlgebraicType { \
                    std::vector<SumTypeVariant> variants; \
                    SpacetimeDB::named_variant_helper<0, SPACETIMEDB_ENUM_VARIANT_TYPES(__VA_ARGS__)>::add_variants( \
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
                SpacetimeDB::bsatn::serialize(writer, v); \
            }, enum_value.value); \
        } \
        \
        static EnumName deserialize(Reader& reader) { \
            uint8_t tag = reader.read_u8(); \
            return EnumName{SpacetimeDB::named_variant_helper<0, SPACETIMEDB_ENUM_VARIANT_TYPES(__VA_ARGS__)>:: \
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

namespace SpacetimeDB {

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

} // namespace SpacetimeDB

// =============================================================================
// COMPILE-TIME NAMESPACE STORAGE
// =============================================================================

namespace SpacetimeDB::detail {
    // Template to store namespace information for types
    template<typename T>
    struct namespace_info {
        static constexpr const char* value = nullptr;
    };
}

/**
 * @brief Add namespace qualification to an enum type for module reusability
 * 
 * This macro registers a namespace prefix with an enum type, allowing the same
 * enum name to be used across different modules without conflicts. The namespace
 * is stored at compile-time and applied during type registration.
 * 
 * **Use Cases:**
 * - Sharing enum definitions across multiple modules
 * - Avoiding name conflicts in large projects
 * - Organizing types into logical namespaces
 * 
 * @param EnumType The C++ enum type (already defined with SPACETIMEDB_ENUM)
 * @param NamespacePrefix The namespace prefix as a string literal (e.g., "MyModule")
 * 
 * @example Basic namespace qualification:
 * @code
 * // Module A
 * SPACETIMEDB_ENUM(Status, Active, Inactive, Banned)
 * SPACETIMEDB_NAMESPACE(Status, "PlayerSystem")
 * // Registered as: "PlayerSystem.Status"
 * 
 * // Module B (can reuse the same enum name)
 * SPACETIMEDB_ENUM(Status, Pending, Approved, Rejected)
 * SPACETIMEDB_NAMESPACE(Status, "OrderSystem")
 * // Registered as: "OrderSystem.Status"
 * @endcode
 * 
 * @example Nested namespaces:
 * @code
 * SPACETIMEDB_ENUM(EventType, Create, Update, Delete)
 * SPACETIMEDB_NAMESPACE(EventType, "Game.Combat")
 * // Registered as: "Game.Combat.EventType"
 * @endcode
 * 
 * @example Without namespace (global scope):
 * @code
 * SPACETIMEDB_ENUM(GlobalState, Initializing, Running, Shutdown)
 * // No SPACETIMEDB_NAMESPACE call → registered as just "GlobalState"
 * @endcode
 * 
 * @note The namespace does NOT affect C++ code - it only applies to SpacetimeDB's type registry
 * @note Call this macro immediately after the SPACETIMEDB_ENUM definition
 * @warning Changing the namespace after data is stored will break schema compatibility
 */
#define SPACETIMEDB_NAMESPACE(EnumType, NamespacePrefix) \
    namespace SpacetimeDB::detail { \
        template<> \
        struct namespace_info<EnumType> { \
            static constexpr const char* value = NamespacePrefix; \
        }; \
    }



