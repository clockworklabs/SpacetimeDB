#pragma once

#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/reducer_context.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/macros.h"

#include <string>
#include <vector>
#include <sstream>

namespace SpacetimeDb {
namespace Internal {

/**
 * @brief Helper function to parse parameter names from stringified parameter list
 * 
 * This is used internally by the SPACETIMEDB_REDUCER macro to extract parameter names
 * from the stringified function signature.
 * 
 * @param param_list The stringified parameter list (e.g., "ReducerContext ctx, uint32_t id, std::string name")
 * @return Vector of parameter names (excluding the first ReducerContext parameter)
 */
inline std::vector<std::string> parseReducerParameterNames(const std::string& param_list) {
    std::vector<std::string> param_names;
    
    // Split by comma
    std::istringstream stream(param_list);
    std::string param;
    bool first = true;
    
    while (std::getline(stream, param, ',')) {
        // Skip the first parameter (ReducerContext)
        if (first) {
            first = false;
            continue;
        }
        
        // Trim whitespace
        param.erase(0, param.find_first_not_of(" \t\n\r"));
        param.erase(param.find_last_not_of(" \t\n\r") + 1);
        
        // Extract the parameter name (last word before end or default value)
        // Handle cases like:
        // - "int x"
        // - "const std::string& name"
        // - "std::vector<int> values"
        // - "MyType* ptr"
        // - "int x = 5"
        
        // Remove default value if present
        size_t eq_pos = param.find('=');
        std::string decl = (eq_pos != std::string::npos) ? param.substr(0, eq_pos) : param;
        
        // Trim trailing whitespace
        decl.erase(decl.find_last_not_of(" \t\n\r") + 1);
        
        // Find the last word (parameter name)
        size_t last_space = decl.find_last_of(" \t&*");
        if (last_space != std::string::npos && last_space + 1 < decl.length()) {
            std::string name = decl.substr(last_space + 1);
            if (!name.empty()) {
                param_names.push_back(name);
            }
        }
    }
    
    return param_names;
}

} // namespace Internal
} // namespace SpacetimeDb

/**
 * @brief Unified SPACETIMEDB_REDUCER macro for defining SpacetimeDB reducers
 * 
 * This macro provides a clean, consistent syntax for defining reducers with
 * automatic registration in the SpacetimeDB module system.
 * 
 * @usage
 * ```cpp
 * // Reducer with no extra parameters
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx) {
 *     ctx.db.table<MyTable>("my_table").insert(MyTable{});
 * }
 * 
 * // Reducer with parameters
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id, std::string name) {
 *     ctx.db.table<MyTable>("my_table").insert(MyTable{id, name});
 * }
 * ```
 * 
 * @param name The name of the reducer function
 * @param ... The full parameter list including ReducerContext as the first parameter
 * 
 * @details
 * The macro generates:
 * 1. A function declaration and definition with the provided signature
 * 2. A preinit registration function that registers the reducer with SpacetimeDB
 * 
 * The first parameter must always be `ReducerContext ctx`. Additional parameters
 * can be any types that support BSATN serialization.
 * 
 * @note This is a simplified implementation for the v2-cpp-library branch.
 * Full parameter deserialization support requires additional runtime work.
 */
#undef SPACETIMEDB_REDUCER
#define SPACETIMEDB_REDUCER(name, ...) \
    /* Forward declaration of the reducer function */ \
    void name(__VA_ARGS__); \
    \
    /* Preinit registration function */ \
    /* This function is called during module initialization to register the reducer */ \
    __attribute__((export_name("__preinit__30_reducer_" #name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_, name)() { \
        /* Parse parameter names from the stringified parameter list */ \
        std::string param_list = #__VA_ARGS__; \
        std::vector<std::string> param_names = \
            SpacetimeDb::Internal::parseReducerParameterNames(param_list); \
        /* Register the reducer with the unified V9Builder system */ \
        SpacetimeDb::Internal::getV9Builder().RegisterReducer(#name, name, param_names); \
    } \
    \
    /* The actual reducer function definition follows */ \
    void name(__VA_ARGS__)

// -----------------------------------------------------------------------------
// Lifecycle Reducer Macros
// -----------------------------------------------------------------------------

// Use unified macro system from macro_helpers.h

/**
 * @brief Macro for defining an init reducer
 * 
 * Init reducers are called when the module is first initialized.
 * They take only a ReducerContext parameter.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_INIT(my_init) {
 *     ctx.db.table<MyTable>().insert({...});
 * }
 * ```
 */
#ifdef SPACETIMEDB_INIT
#undef SPACETIMEDB_INIT
#endif
#define SPACETIMEDB_INIT(function_name) \
    void function_name(SpacetimeDb::ReducerContext ctx); \
    __attribute__((export_name("__preinit__20_reducer_init"))) \
    extern "C" void CONCAT(_preinit_register_init_reducer_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::Init); \
    } \
    void function_name(SpacetimeDb::ReducerContext ctx)

/**
 * @brief Macro for defining a client_connected reducer
 * 
 * Client connected reducers are called when a client connects to the module.
 * They receive the connecting client's Identity as a parameter.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_CLIENT_CONNECTED(on_connect) {
 *     LOG_INFO("Client connected: " + sender.to_hex());
 * }
 * ```
 */
#ifdef SPACETIMEDB_CLIENT_CONNECTED
#undef SPACETIMEDB_CLIENT_CONNECTED
#endif
#define SPACETIMEDB_CLIENT_CONNECTED(function_name) \
    void function_name(SpacetimeDb::ReducerContext ctx); \
    __attribute__((export_name("__preinit__20_reducer_client_connected"))) \
    extern "C" void CONCAT(_preinit_register_client_connected_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::OnConnect); \
    } \
    void function_name(SpacetimeDb::ReducerContext ctx)

/**
 * @brief Macro for defining a client_disconnected reducer
 * 
 * Client disconnected reducers are called when a client disconnects from the module.
 * They receive the disconnecting client's Identity as a parameter.
 * 
 * @usage
 * ```cpp
 * SPACETIMEDB_CLIENT_DISCONNECTED(on_disconnect) {
 *     LOG_INFO("Client disconnected: " + sender.to_hex());
 * }
 * ```
 */
#ifdef SPACETIMEDB_CLIENT_DISCONNECTED
#undef SPACETIMEDB_CLIENT_DISCONNECTED
#endif
#define SPACETIMEDB_CLIENT_DISCONNECTED(function_name) \
    void function_name(SpacetimeDb::ReducerContext ctx); \
    __attribute__((export_name("__preinit__20_reducer_client_disconnected"))) \
    extern "C" void CONCAT(_preinit_register_client_disconnected_, function_name)() { \
        ::SpacetimeDb::Internal::getV9Builder().RegisterLifecycleReducer(#function_name, function_name, ::SpacetimeDb::Internal::Lifecycle::OnDisconnect); \
    } \
    void function_name(SpacetimeDb::ReducerContext ctx)