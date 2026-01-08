#pragma once

#include "spacetimedb/procedure_context.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/macros.h"  // For CONCAT
#include "spacetimedb/error_handling.h"
#include <string>
#include <vector>
#include <type_traits>

namespace SpacetimeDb {
namespace Internal {

/**
 * @brief Type trait to validate procedure return types
 * 
 * Procedures can return any SpacetimeType, unlike views which are restricted
 * to std::vector<T> or std::optional<T>. This includes primitives, structs,
 * enums, or any custom type that implements the SpacetimeType concept.
 * 
 * The procedure macro wraps the return type in Outcome<T> automatically.
 */
template<typename T>
struct is_valid_procedure_return_type : std::integral_constant<bool, bsatn::Serializable<T>> {};

} // namespace Internal
} // namespace SpacetimeDb

/**
 * @brief Macro for defining SpacetimeDB procedures
 * 
 * Procedures are functions that can return arbitrary values (unlike reducers which return void).
 * They are always public (no access control like reducers).
 * 
 * Part 1 Implementation: Pure Functions
 * - Procedures can perform computations and return results
 * - NO database access (ProcedureContext has no db field)
 * - Return Outcome<T> where T is any SpacetimeType
 * 
 * Future Parts (documented for reference):
 * - Part 2: Transactions via ctx.WithTx() and ctx.TryWithTx()
 * - Part 3: Scheduled execution via table attributes
 * - Part 4: HTTP requests via HttpClient
 * 
 * @param return_type The return type - can be any SpacetimeType (primitive, struct, enum, etc.)
 * @param procedure_name The name of the procedure function
 * @param ctx_param Must be ProcedureContext ctx
 * @param ... Additional parameters (optional) - any SpacetimeType
 * 
 * Example (Part 1 - pure function):
 * @code
 * // Return primitive
 * SPACETIMEDB_PROCEDURE(uint32_t, add_numbers, ProcedureContext ctx, uint32_t a, uint32_t b) {
 *     if (a == 0 && b == 0) {
 *         return Err("Cannot add two zeros");
 *     }
 *     return Ok(a + b);
 * }
 * 
 * // Return struct
 * struct ReturnStruct {
 *     uint32_t a;
 *     std::string b;
 * };
 * SPACETIMEDB_STRUCT(ReturnStruct, a, b)
 * 
 * SPACETIMEDB_PROCEDURE(ReturnStruct, make_struct, ProcedureContext ctx, uint32_t a, std::string b) {
 *     return Ok(ReturnStruct{a, b});
 * }
 * 
 * // Return Unit (no value)
 * SPACETIMEDB_PROCEDURE(Unit, do_something, ProcedureContext ctx) {
 *     // Part 1: Can only do computation, no database operations
 *     return Ok();  // Explicit Ok() required for Unit
 * }
 * 
 * // With parameters:
 * // SPACETIMEDB_PROCEDURE(uint32_t, calculate, ProcedureContext ctx, uint32_t x, uint32_t y) {
 * //     return Ok(x * y + 42);
 * // }
 * @endcode
 */
#define SPACETIMEDB_PROCEDURE(return_type, procedure_name, ctx_param, ...) \
    /* Validate return type at compile-time */ \
    static_assert(::SpacetimeDb::Internal::is_valid_procedure_return_type<return_type>::value, \
        "Procedure return type must be a SpacetimeType (implement Serializable trait)"); \
    \
    /* Forward declaration with optional parameters */ \
    return_type procedure_name(ctx_param __VA_OPT__(,) __VA_ARGS__); \
    \
    /* Preinit registration function */ \
    /* Procedures run at priority 50 to ensure views are registered first */ \
    __attribute__((export_name("__preinit__50_proc_" #procedure_name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_proc_, procedure_name)() { \
        /* Parse parameter names from the stringified parameter list */ \
        std::string param_list = #__VA_ARGS__; \
        std::vector<std::string> param_names = \
            SpacetimeDb::Internal::parseParameterNames(param_list); \
        \
        /* Register the procedure with the V9Builder system */ \
        /* Note: Procedures are always public (no is_public parameter) */ \
        ::SpacetimeDb::Internal::getV9Builder().RegisterProcedure( \
            #procedure_name, procedure_name, param_names); \
    } \
    \
    /* The actual procedure function definition */ \
    return_type procedure_name(ctx_param __VA_OPT__(,) __VA_ARGS__)
