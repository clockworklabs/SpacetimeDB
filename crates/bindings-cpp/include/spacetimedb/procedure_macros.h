#pragma once

#include "spacetimedb/procedure_context.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/macros.h"  // For CONCAT
#include "spacetimedb/error_handling.h"
#include <string>
#include <vector>
#include <type_traits>

namespace SpacetimeDB {
namespace Internal {

/**
 * @brief Type trait to validate procedure return types
 * 
 * Procedures can return any SpacetimeType, unlike views which are restricted
 * to std::vector<T> or std::optional<T>. This includes primitives, structs,
 * enums, or any custom type that implements the SpacetimeType concept.
 * 
 */
template<typename T>
struct is_valid_procedure_return_type : std::integral_constant<bool, bsatn::Serializable<T>> {};

} // namespace Internal
} // namespace SpacetimeDB

/**
 * @brief Macro for defining SpacetimeDB procedures
 * 
 * Procedures are functions that can return arbitrary values (unlike reducers which return void).
 * They are always public (no access control like reducers).
 * 
 * Features:
 * - Pure computations with return values
 * - Database access via explicit transactions (ctx.WithTx() or ctx.TryWithTx())
 * - HTTP requests via ctx.http (when SPACETIMEDB_UNSTABLE_FEATURES enabled)
 * - UUID generation (ctx.new_uuid_v4(), ctx.new_uuid_v7())
 * - Return type directly
 * 
 * Key differences from reducers:
 * - NO direct db field (must use ctx.WithTx() for database operations)
 * - Has connection_id (procedures track which connection called them)
 * - Can return any SpacetimeType
 * 
 * @param return_type The return type - can be any SpacetimeType (primitive, struct, enum, etc.)
 * @param procedure_name The name of the procedure function
 * @param ctx_param Must be ProcedureContext ctx
 * @param ... Additional parameters (optional) - any SpacetimeType
 * 
 * Examples:
 * @code
 * // Pure computation
 * SPACETIMEDB_PROCEDURE(uint32_t, add_numbers, ProcedureContext ctx, uint32_t a, uint32_t b) {
 *     return a + b;
 * }
 * 
 * // With database transaction
 * SPACETIMEDB_PROCEDURE(Unit, insert_item, ProcedureContext ctx, Item item) {
 *     ctx.WithTx([&item](TxContext& tx) {
 *         tx.db[items].insert(item);
 *     });
 *     return Unit{};
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
 *     return ReturnStruct{a, b};
 * }
 * 
 * // UUID generation
 * SPACETIMEDB_PROCEDURE(Uuid, generate_uuid, ProcedureContext ctx) {
 *     return ctx.new_uuid_v7();
 * }
 * @endcode
 */
#define SPACETIMEDB_PROCEDURE(return_type, procedure_name, ctx_param, ...) \
    /* Validate return type at compile-time */ \
    static_assert(::SpacetimeDB::Internal::is_valid_procedure_return_type<return_type>::value, \
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
            SpacetimeDB::Internal::parseParameterNames(param_list); \
        \
        /* Register the procedure with the V9Builder system */ \
        /* Note: Procedures are always public (no is_public parameter) */ \
        ::SpacetimeDB::Internal::getV9Builder().RegisterProcedure( \
            #procedure_name, procedure_name, param_names); \
    } \
    \
    /* The actual procedure function definition */ \
    return_type procedure_name(ctx_param __VA_OPT__(,) __VA_ARGS__)
