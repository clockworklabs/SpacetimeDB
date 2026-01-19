#pragma once

#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/reducer_context.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/macros.h"

#include <string>
#include <vector>
#include <sstream>

// Note: parseParameterNames() is now in macros.h for reuse across reducers and views

/**
 * @brief Unified SPACETIMEDB_REDUCER macro for defining SpacetimeDB reducers
 * 
 * This macro provides a clean, consistent syntax for defining reducers with
 * automatic registration in the SpacetimeDB module system.
 * 
 * Reducers now return SpacetimeDb::ReducerResult to support Result-based
 * error handling matching Rust's Result<(), E> pattern.
 * 
 * @usage
 * ```cpp
 * // Reducer with no extra parameters
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx) {
 *     if (some_error) {
 *         return Err("Error message");
 *     }
 *     ctx.db.table<MyTable>("my_table").insert(MyTable{});
 *     return Ok();
 * }
 * 
 * // Reducer with parameters
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id, std::string name) {
 *     if (id == 0) {
 *         return Err("ID must be non-zero");
 *     }
 *     ctx.db.table<MyTable>("my_table").insert(MyTable{id, name});
 *     return Ok();
 * }
 * ```
 * 
 * @param name The name of the reducer function
 * @param ctx_param Must be ReducerContext ctx
 * @param ... Additional parameters (optional)
 * 
 * @details
 * The macro generates:
 * 1. A function declaration and definition with ReducerResult return type
 * 2. A preinit registration function that registers the reducer with SpacetimeDB
 * 
 * The first parameter must always be `ReducerContext ctx`. Additional parameters
 * can be any types that support BSATN serialization.
 */
#undef SPACETIMEDB_REDUCER
#define SPACETIMEDB_REDUCER(name, ctx_param, ...) \
    /* Forward declaration - returns ReducerResult */ \
    SpacetimeDb::ReducerResult name(ctx_param __VA_OPT__(,) __VA_ARGS__); \
    \
    /* Preinit registration function */ \
    /* This function is called during module initialization to register the reducer */ \
    __attribute__((export_name("__preinit__30_reducer_" #name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_, name)() { \
        /* Parse parameter names from the stringified parameter list */ \
        std::string param_list = #__VA_ARGS__; \
        std::vector<std::string> param_names = \
            SpacetimeDb::Internal::parseParameterNames(param_list); \
        /* Register the reducer with the unified V9Builder system */ \
        SpacetimeDb::Internal::getV9Builder().RegisterReducer(#name, name, param_names); \
    } \
    \
    /* The actual reducer function definition - returns ReducerResult */ \
    SpacetimeDb::ReducerResult name(ctx_param __VA_OPT__(,) __VA_ARGS__)

// -----------------------------------------------------------------------------
// Lifecycle Reducer Macros
// -----------------------------------------------------------------------------

// Use unified macro system from macro_helpers.h

/**
 * @brief Macro for defining an init reducer
 * 
 * Init reducers are called when the module is first initialized.
 * They require an explicit ReducerContext parameter and return ReducerResult.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_INIT(my_init, ReducerContext ctx) {
 *     ctx.db.table<MyTable>().insert({...});
 *     return Ok();
 * }
 * ```
 */
#ifdef SPACETIMEDB_INIT
#undef SPACETIMEDB_INIT
#endif
#define SPACETIMEDB_INIT(function_name, ctx_param) \
    SpacetimeDb::ReducerResult function_name(ctx_param); \
    __attribute__((export_name("__preinit__20_reducer_init"))) \
    extern "C" void CONCAT(_preinit_register_init_reducer_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::Init); \
    } \
    SpacetimeDb::ReducerResult function_name(ctx_param)

/**
 * @brief Macro for defining a client_connected reducer
 * 
 * Client connected reducers require an explicit ReducerContext parameter and return ReducerResult.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_CLIENT_CONNECTED(on_connect, ReducerContext ctx) {
 *     LOG_INFO("Client connected: " + ctx.sender.to_hex());
 *     return Ok();
 * }
 * ```
 */
#ifdef SPACETIMEDB_CLIENT_CONNECTED
#undef SPACETIMEDB_CLIENT_CONNECTED
#endif
#define SPACETIMEDB_CLIENT_CONNECTED(function_name, ctx_param) \
    SpacetimeDb::ReducerResult function_name(ctx_param); \
    __attribute__((export_name("__preinit__20_reducer_client_connected"))) \
    extern "C" void CONCAT(_preinit_register_client_connected_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::OnConnect); \
    } \
    SpacetimeDb::ReducerResult function_name(ctx_param)

/**
 * @brief Macro for defining a client_disconnected reducer
 * 
 * Client disconnected reducers require an explicit ReducerContext parameter and return ReducerResult.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect, ReducerContext ctx) {
 *     LOG_INFO("Client disconnected: " + ctx.sender.to_hex());
 *     return Ok();
 * }
 * ```
 */
#ifdef SPACETIMEDB_CLIENT_DISCONNECTED
#undef SPACETIMEDB_CLIENT_DISCONNECTED
#endif
#define SPACETIMEDB_CLIENT_DISCONNECTED(function_name, ctx_param) \
    SpacetimeDb::ReducerResult function_name(ctx_param); \
    __attribute__((export_name("__preinit__20_reducer_client_disconnected"))) \
    extern "C" void CONCAT(_preinit_register_client_disconnected_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::OnDisconnect); \
    } \
    SpacetimeDb::ReducerResult function_name(ctx_param)