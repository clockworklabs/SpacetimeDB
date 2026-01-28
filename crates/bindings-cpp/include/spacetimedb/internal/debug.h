#pragma once

#include <cstdio>

/**
 * @file debug.h
 * @brief Conditional debugging macros for SpacetimeDB C++ bindings
 * 
 * This header provides conditional debugging output that can be enabled/disabled
 * at compile time to reduce runtime overhead and output clutter.
 */

// Control debug output with compile-time flag
#ifdef SPACETIMEDB_DEBUG
    #define STDB_DEBUG_ENABLED 1
#else
    #define STDB_DEBUG_ENABLED 0
#endif

/**
 * @brief Main debug macro - outputs to stderr with [STDB] prefix
 * 
 * Usage: STDB_DEBUG("Type %s registered with index %u", type_name.c_str(), index);
 * 
 * When SPACETIMEDB_DEBUG is not defined, this compiles to nothing (zero overhead).
 */
#if STDB_DEBUG_ENABLED
    #define STDB_DEBUG(fmt, ...) \
        fprintf(stderr, "[STDB] " fmt "\n", ##__VA_ARGS__)
#else
    #define STDB_DEBUG(fmt, ...) ((void)0)
#endif

/**
 * @brief Verbose debug macro for detailed tracing
 * 
 * Usage: STDB_VERBOSE("Processing field %zu: %s", i, field_name);
 * 
 * Even more detailed than STDB_DEBUG, only enabled with SPACETIMEDB_VERBOSE.
 */
#ifdef SPACETIMEDB_VERBOSE
    #define STDB_VERBOSE(fmt, ...) \
        fprintf(stderr, "[STDB:VERBOSE] " fmt "\n", ##__VA_ARGS__)
#else
    #define STDB_VERBOSE(fmt, ...) ((void)0)
#endif

/**
 * @brief Error output (always enabled)
 * 
 * Usage: STDB_ERROR("Failed to register type: %s", error_msg);
 * 
 * Always outputs regardless of debug flags.
 */
#define STDB_ERROR(fmt, ...) \
    fprintf(stderr, "[STDB:ERROR] " fmt "\n", ##__VA_ARGS__)

/**
 * @brief Warning output (always enabled)
 * 
 * Usage: STDB_WARN("Type %s already registered", type_name);
 * 
 * Always outputs regardless of debug flags.
 */
#define STDB_WARN(fmt, ...) \
    fprintf(stderr, "[STDB:WARN] " fmt "\n", ##__VA_ARGS__)

/**
 * @brief Conditional debug macro for specific subsystems
 * 
 * Usage: STDB_DEBUG_TYPE("Registered enum %s", enum_name);
 * 
 * Can be independently controlled with SPACETIMEDB_DEBUG_TYPE.
 */
#ifdef SPACETIMEDB_DEBUG_TYPE
    #define STDB_DEBUG_TYPE(fmt, ...) \
        fprintf(stderr, "[STDB:TYPE] " fmt "\n", ##__VA_ARGS__)
#else
    #define STDB_DEBUG_TYPE(fmt, ...) ((void)0)
#endif

// Usage example in code:
// #define SPACETIMEDB_DEBUG  // Enable general debugging
// #include "debug.h"
// 
// void registerType() {
//     STDB_DEBUG("Starting type registration");
//     STDB_VERBOSE("Detailed step-by-step trace");
//     STDB_ERROR("Something went wrong");
//     STDB_WARN("Non-fatal issue");
// }