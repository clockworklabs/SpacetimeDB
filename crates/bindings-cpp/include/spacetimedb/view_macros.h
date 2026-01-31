#pragma once

#include "spacetimedb/view_context.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/macros.h"  // For parseParameterNames
#include "spacetimedb/error_handling.h"
#include <string>
#include <vector>
#include <type_traits>

namespace SpacetimeDB {
namespace Internal {

// Note: parseParameterNames is in macros.h and works for any context type
// (ReducerContext, ViewContext, or AnonymousViewContext)

/**
 * @brief Type trait to validate view return types
 * 
 * Views must return std::vector<T> or std::optional<T> where T is a SpacetimeType.
 * This is enforced at compile-time via static_assert in the macro.
 */
template<typename T>
struct is_valid_view_return_type : std::false_type {};

// Specialization for std::vector<T> where T is Serializable
template<typename T>
struct is_valid_view_return_type<std::vector<T>> 
    : std::integral_constant<bool, bsatn::Serializable<T>> {};

// Specialization for std::optional<T> where T is Serializable
template<typename T>
struct is_valid_view_return_type<std::optional<T>>
    : std::integral_constant<bool, bsatn::Serializable<T>> {};

} // namespace Internal
} // namespace SpacetimeDB

/**
 * @brief Macro for defining SpacetimeDB views
 * 
 * CRITICAL: Return type must be explicit (no arrow notation in C++ macros).
 * Views return the specified return_type directly.
 * 
 * NOTE: Additional parameters are temporarily disabled as the SpacetimeDB host
 * doesn't fully support parameterized views yet. Only the context parameter is allowed.
 * 
 * Allowed return types:
 * - std::vector<T> where T is a SpacetimeType
 * - std::optional<T> where T is a SpacetimeType
 * 
 * @param return_type The return type (e.g., std::vector<Person>, std::optional<Person>)
 * @param view_name The name of the view function
 * @param access_enum Must be Public (Private views not yet supported)
 * @param ctx_param Must be either ViewContext ctx or AnonymousViewContext ctx
 * 
 * Example:
 * @code
 * SPACETIMEDB_VIEW(std::vector<Person>, get_adults, Public, ViewContext ctx) {
 *     std::vector<Person> adults;
 *     for (const auto& person : ctx.db[person_age].filter(range_from(18u))) {
 *         adults.push_back(person);
 *     }
 *     return adults;
 * }
 * 
 * SPACETIMEDB_VIEW(std::optional<uint64_t>, count_people, Public, AnonymousViewContext ctx) {
 *     return std::optional<uint64_t>(ctx.db[person].count());
 * }
 * 
 * // TODO: Future with parameters:
 * // SPACETIMEDB_VIEW(std::vector<Person>, search_by_age, Public, ViewContext ctx, uint32_t min_age, uint32_t max_age) {
 * //     std::vector<Person> results;
 * //     for (const auto& person : ctx.db[person_age].filter(range_between(min_age, max_age))) {
 * //         results.push_back(person);
 * //     }
 * //     return Ok(results);
 * // }
 * @endcode
 */
/* TODO: When parameters are supported, change signature to:
 * #define SPACETIMEDB_VIEW(return_type, view_name, access_enum, ctx_param, ...)
 */
#define SPACETIMEDB_VIEW(return_type, view_name, access_enum, ctx_param) \
    /* Compile-time assertion that views must be Public for now */ \
    static_assert(access_enum == SpacetimeDB::Internal::TableAccess::Public, \
        "Views must be Public - Private views are not yet supported"); \
    \
    /* Validate return type at compile-time */ \
    static_assert(::SpacetimeDB::Internal::is_valid_view_return_type<return_type>::value, \
        "View return type must be std::vector<T> or std::optional<T> where T is a SpacetimeType"); \
    \
    /* TODO: When parameters are supported, forward declaration becomes: */ \
    /* return_type view_name(ctx_param, __VA_ARGS__); */ \
    return_type view_name(ctx_param); \
    \
    /* Preinit registration function */ \
    /* Views run at priority 40 to ensure tables/reducers are registered first */ \
    __attribute__((export_name("__preinit__40_view_" #view_name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_view_, view_name)() { \
        /* Convert access_enum to bool (matching SPACETIMEDB_TABLE pattern) */ \
        bool is_public = (access_enum == SpacetimeDB::Internal::TableAccess::Public); \
        \
        /* TODO: When parameters are supported, uncomment: */ \
        /* std::vector<std::string> param_names = parseParameterNames(#__VA_ARGS__); */ \
        std::vector<std::string> param_names; \
        \
        /* Register the view with the V9Builder system */ \
        /* RegisterView validates ctx_param is ViewContext or AnonymousViewContext */ \
        ::SpacetimeDB::Internal::getV9Builder().RegisterView<decltype(&view_name)>( \
            #view_name, view_name, is_public, param_names); \
    } \
    \
    /* TODO: When parameters are supported, function definition becomes: */ \
    /* return_type view_name(ctx_param, __VA_ARGS__) */ \
    return_type view_name(ctx_param)
