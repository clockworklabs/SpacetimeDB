#ifndef SPACETIMEDB_MACROS_H
#define SPACETIMEDB_MACROS_H

#include "spacetimedb/bsatn/bsatn.h"
#include "spacetimedb/reducer_context.h"
#include "spacetimedb/table.h"
#include "spacetimedb/schedule_reducer.h"
#include "spacetimedb/rls.h" // For future Row Level Security functionality
#include "spacetimedb/bsatn/traits.h"

#include <string>

// =============================================================================
// UNIFIED HELPER MACROS FOR SPACETIMEDB TYPE REGISTRATION
// =============================================================================

/**
 * @brief Common CONCAT macros - unified from multiple files
 * 
 * These eliminate duplication of CONCAT macro definitions across files.
 */
#ifndef SPACETIMEDB_CONCAT_IMPL
#define SPACETIMEDB_CONCAT_IMPL(a, b) a##b
#define SPACETIMEDB_CONCAT(a, b) SPACETIMEDB_CONCAT_IMPL(a, b)
#endif

/**
 * @brief Helper for stringifying values (used in export names)
 */
#ifndef SPACETIMEDB_STRINGIFY_IMPL
#define SPACETIMEDB_STRINGIFY_IMPL(x) #x  
#define SPACETIMEDB_STRINGIFY(x) SPACETIMEDB_STRINGIFY_IMPL(x)
#endif

/**
 * @brief Compatibility aliases for files that use different macro names
 * 
 * This provides backward compatibility while centralizing macro definitions.
 * Files can gradually migrate to the SPACETIMEDB_* prefixed versions.
 */

// Non-prefixed compatibility aliases
#ifndef CONCAT_IMPL
#define CONCAT_IMPL(a, b) SPACETIMEDB_CONCAT_IMPL(a, b)
#define CONCAT(a, b) SPACETIMEDB_CONCAT(a, b)
#endif

#ifndef STRINGIFY_IMPL  
#define STRINGIFY_IMPL(x) SPACETIMEDB_STRINGIFY_IMPL(x)
#define STRINGIFY(x) SPACETIMEDB_STRINGIFY(x)
#endif

// SPACETIMEDB_PASTE compatibility alias (some files use PASTE instead of CONCAT)
#ifndef SPACETIMEDB_PASTE_IMPL
#define SPACETIMEDB_PASTE_IMPL(a, b) SPACETIMEDB_CONCAT_IMPL(a, b)
#define SPACETIMEDB_PASTE(a, b) SPACETIMEDB_CONCAT(a, b)
#endif

// TypeRegistry system removed - use V9TypeRegistration instead
/*
// Register a C++ type name in the global TypeRegistry for reducer parameter lookup
// This replaces the old TypeNameRegistry approach with a unified one.
// Used by all macros that need reducer parameter support.
#define SPACETIMEDB_REGISTER_CPP_TYPE_NAME(Type) \
    namespace { \
        struct Type##_cpp_name_registrar { \
            Type##_cpp_name_registrar() { \
                ::SpacetimeDb::TypeRegistry::global_instance().register_cpp_type_name<Type>(#Type); \
            } \
        }; \
        static Type##_cpp_name_registrar Type##_cpp_name_registrar_instance; \
    }

// Register a type in the active TypeRegistry with proper naming
// This is the single registration point that eliminates triple registration.
// Only registers if a registry is available and not already registered.
#define SPACETIMEDB_REGISTER_TYPE_IN_REGISTRY(Type, algebraic_type) \
    do { \
        static bool Type##_v9_registered = false; \
        if (!Type##_v9_registered) { \
            Type##_v9_registered = true; \
            std::string type_name = #Type; \
            ::SpacetimeDb::Internal::getV9TypeRegistration().registerTypeByName(type_name, algebraic_type, &typeid(Type)); \
        } \
    } while(0)
*/

/**
 * @brief Updated registration using V9TypeRegistration system
 * 
 * This is the new registration point using the V9TypeRegistration system.
 */
#define SPACETIMEDB_REGISTER_TYPE_IN_V9(Type, algebraic_type) \
    do { \
        static bool Type##_v9_registered = false; \
        if (!Type##_v9_registered) { \
            Type##_v9_registered = true; \
            std::string type_name = #Type; \
            /* Register immediately with name and structure */ \
            ::SpacetimeDb::Internal::getV9TypeRegistration().registerTypeByName(type_name, algebraic_type, &typeid(Type)); \
        } \
    } while(0)

/**
 * @brief Complete type registration (registry + global name registration)
 * 
 * This combines both registry registration and global name registration
 * in a single, atomic operation. Use this in algebraic_type() methods.
 */
#define SPACETIMEDB_REGISTER_TYPE_COMPLETE(Type, algebraic_type) \
    SPACETIMEDB_REGISTER_TYPE_IN_V9(Type, algebraic_type)

/**
 * @brief Generate a default field_registrar specialization that does nothing
 * 
 * Most types don't need field registration (only table types do).
 * This provides a clean default implementation.
 */
#define SPACETIMEDB_GENERATE_EMPTY_FIELD_REGISTRAR(Type) \
    template<> \
    struct ::SpacetimeDb::field_registrar<Type> { \
        static void register_fields() { \
            /* Default: no field registration needed */ \
        } \
    };

/**
 * @brief Generate a field_registrar that actually registers field descriptors
 * 
 * This version is used by SPACETIMEDB_STRUCT to ensure fields
 * are properly registered for table types.
 */
#define SPACETIMEDB_GENERATE_FIELD_REGISTRAR_WITH_FIELDS(Type, ...) \
    template<> \
    struct ::SpacetimeDb::field_registrar<Type> { \
        static void register_fields() { \
            static bool registered = false; \
            if (registered) return; \
            registered = true; \
            SPACETIMEDB_REGISTER_FIELD_DESCRIPTORS(Type, __VA_ARGS__) \
        } \
    };

/**
 * @brief Generate the complete registration bundle for a type
 * 
 * This combines all the registration pieces that most macros need:
 * - BSATN algebraic_type_of specialization
 * - Empty field_registrar specialization  
 * 
 * Use this for simple types that don't need custom field registration.
 * Note: Type name registration is now handled automatically by V9TypeRegistration.
 */
#define SPACETIMEDB_GENERATE_TYPE_REGISTRATION_BUNDLE(Type) \
    template<> \
    struct ::SpacetimeDb::bsatn::algebraic_type_of<Type> { \
        static ::SpacetimeDb::bsatn::AlgebraicType get() { \
            return ::SpacetimeDb::bsatn::bsatn_traits<Type>::algebraic_type(); \
        } \
    }; \
    SPACETIMEDB_GENERATE_EMPTY_FIELD_REGISTRAR(Type)

/**
 * @brief Generate the complete registration bundle with field registration
 * 
 * This version includes actual field registration for table types.
 * Note: Type name registration is now handled automatically by V9TypeRegistration.
 */
#define SPACETIMEDB_GENERATE_TYPE_REGISTRATION_BUNDLE_WITH_FIELDS(Type, ...) \
    template<> \
    struct ::SpacetimeDb::bsatn::algebraic_type_of<Type> { \
        static ::SpacetimeDb::bsatn::AlgebraicType get() { \
            return ::SpacetimeDb::bsatn::bsatn_traits<Type>::algebraic_type(); \
        } \
    }; \
    SPACETIMEDB_GENERATE_FIELD_REGISTRAR_WITH_FIELDS(Type, __VA_ARGS__)

/**
 * @brief Helper to check if a type is already registered (for optimization)
 * 
 * This can be used to avoid expensive operations if a type is already registered.
 */
#define SPACETIMEDB_IS_TYPE_REGISTERED(Type) \
    ([]() { \
        static bool Type##_checked = false; \
        return Type##_checked; \
    }())

/**
 * @brief Mark a type as registered (for optimization)
 */
#define SPACETIMEDB_MARK_TYPE_REGISTERED(Type) \
    do { \
        static bool Type##_checked = false; \
        Type##_checked = true; \
    } while(0)

/**
 * @brief Unified preinit function generator
 * 
 * This consolidates the pattern used by all table/reducer registration macros:
 * - Generates unique export name with priority, category, name, and line number
 * - Creates the preinit function with proper C linkage
 * 
 * @param priority Registration order (20 for tables, 30 for reducers)
 * @param category Type of registration (register_table, reducer, etc.)
 * @param name Specific name of the item being registered
 * @param registration_body The actual registration code
 */
#define SPACETIMEDB_GENERATE_PREINIT_FUNCTION(priority, category, name, registration_body) \
    __attribute__((export_name("__preinit__" #priority "_" #category "_" #name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    extern "C" void SPACETIMEDB_CONCAT(__preinit__, SPACETIMEDB_CONCAT(priority, SPACETIMEDB_CONCAT(_, SPACETIMEDB_CONCAT(category, SPACETIMEDB_CONCAT(_, SPACETIMEDB_CONCAT(name, SPACETIMEDB_CONCAT(_line_, __LINE__)))))))() { \
        registration_body \
    }

/**
 * @brief Unified table registration pattern
 * 
 * Consolidates the common table registration logic used by SPACETIMEDB_TABLE macro.
 * 
 * @param type The table struct type
 * @param name_str Table name string  
 * @param access_enum Public/Private access
 * @param constraint_list Vector initializer for field constraints
 */
// SPACETIMEDB_REGISTER_TABLE_TYPE macro removed - replaced by V9Builder system

/**
 * @brief Unified reducer registration pattern
 * 
 * Consolidates the common reducer registration logic used by SPACETIMEDB_REDUCER macro.
 * 
 * @param function_name Name of the reducer function
 * @param function_signature Full parameter list for the function
 */
#define SPACETIMEDB_REGISTER_REDUCER_FUNCTION(function_name, function_signature) \
    void function_name function_signature; \
    SPACETIMEDB_GENERATE_PREINIT_FUNCTION(30, reducer, function_name, \
        SpacetimeDb::Internal::register_reducer_func_with_params(std::string(#function_name), function_name, #function_signature); \
    ) \
    void function_name function_signature

/**
 * @brief Unified lifecycle reducer registration
 * 
 * Consolidates the pattern used by SPACETIMEDB_INIT, SPACETIMEDB_CLIENT_CONNECTED, etc.
 * 
 * @param lifecycle_type Type of lifecycle (init, client_connected, client_disconnected)
 * @param function_name Name of the reducer function  
 * @param function_signature Full parameter list for the function
 * @param register_call The specific registration call for this lifecycle type
 */
#define SPACETIMEDB_REGISTER_LIFECYCLE_REDUCER(lifecycle_type, function_name, function_signature, register_call) \
    void function_name function_signature; \
    SPACETIMEDB_GENERATE_PREINIT_FUNCTION(20, reducer, lifecycle_type, register_call) \
    void function_name function_signature

// =============================================================================
// VARIADIC ARGUMENT PROCESSING UTILITIES
// =============================================================================

// Consolidated variadic macro system (supports up to 50 arguments)
// Used by SPACETIMEDB_STRUCT and SPACETIMEDB_VARIANT_ENUM
#define SPACETIMEDB_GET_MACRO(_1,_2,_3,_4,_5,_6,_7,_8,_9,_10,_11,_12,_13,_14,_15,_16,_17,_18,_19,_20,_21,_22,_23,_24,_25,_26,_27,_28,_29,_30,_31,_32,_33,_34,_35,_36,_37,_38,_39,_40,_41,_42,_43,_44,_45,_46,_47,_48,_49,_50,NAME,...) NAME

#define SPACETIMEDB_FOR_EACH_ARG(MACRO, obj, extra, ...) \
    SPACETIMEDB_GET_MACRO(__VA_ARGS__, \
        SPACETIMEDB_FE_50, SPACETIMEDB_FE_49, SPACETIMEDB_FE_48, SPACETIMEDB_FE_47, SPACETIMEDB_FE_46, \
        SPACETIMEDB_FE_45, SPACETIMEDB_FE_44, SPACETIMEDB_FE_43, SPACETIMEDB_FE_42, SPACETIMEDB_FE_41, \
        SPACETIMEDB_FE_40, SPACETIMEDB_FE_39, SPACETIMEDB_FE_38, SPACETIMEDB_FE_37, SPACETIMEDB_FE_36, \
        SPACETIMEDB_FE_35, SPACETIMEDB_FE_34, SPACETIMEDB_FE_33, SPACETIMEDB_FE_32, SPACETIMEDB_FE_31, \
        SPACETIMEDB_FE_30, SPACETIMEDB_FE_29, SPACETIMEDB_FE_28, SPACETIMEDB_FE_27, SPACETIMEDB_FE_26, \
        SPACETIMEDB_FE_25, SPACETIMEDB_FE_24, SPACETIMEDB_FE_23, SPACETIMEDB_FE_22, SPACETIMEDB_FE_21, \
        SPACETIMEDB_FE_20, SPACETIMEDB_FE_19, SPACETIMEDB_FE_18, SPACETIMEDB_FE_17, SPACETIMEDB_FE_16, \
        SPACETIMEDB_FE_15, SPACETIMEDB_FE_14, SPACETIMEDB_FE_13, SPACETIMEDB_FE_12, SPACETIMEDB_FE_11, \
        SPACETIMEDB_FE_10, SPACETIMEDB_FE_9, SPACETIMEDB_FE_8, SPACETIMEDB_FE_7, SPACETIMEDB_FE_6, \
        SPACETIMEDB_FE_5, SPACETIMEDB_FE_4, SPACETIMEDB_FE_3, SPACETIMEDB_FE_2, SPACETIMEDB_FE_1) \
    (MACRO, obj, extra, __VA_ARGS__)

#define SPACETIMEDB_FE_1(MACRO, obj, extra, X) MACRO(obj, extra, X)
#define SPACETIMEDB_FE_2(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_1(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_3(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_2(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_4(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_3(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_5(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_4(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_6(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_5(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_7(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_6(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_8(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_7(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_9(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_8(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_10(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_9(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_11(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_10(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_12(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_11(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_13(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_12(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_14(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_13(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_15(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_14(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_16(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_15(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_17(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_16(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_18(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_17(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_19(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_18(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_20(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_19(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_21(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_20(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_22(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_21(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_23(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_22(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_24(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_23(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_25(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_24(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_26(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_25(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_27(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_26(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_28(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_27(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_29(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_28(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_30(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_29(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_31(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_30(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_32(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_31(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_33(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_32(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_34(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_33(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_35(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_34(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_36(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_35(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_37(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_36(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_38(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_37(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_39(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_38(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_40(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_39(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_41(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_40(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_42(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_41(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_43(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_42(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_44(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_43(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_45(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_44(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_46(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_45(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_47(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_46(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_48(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_47(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_49(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_48(MACRO, obj, extra, __VA_ARGS__)
#define SPACETIMEDB_FE_50(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) SPACETIMEDB_FE_49(MACRO, obj, extra, __VA_ARGS__)

// =============================================================================
// SPECIALIZED FOR_EACH MACROS FOR DIFFERENT USE CASES
// =============================================================================

/**
 * @brief FOR_EACH_VARIANT - specialized macro for enum variant processing
 * 
 * This adapts the unified SPACETIMEDB_FOR_EACH_ARG to use separator-based syntax
 * instead of obj-extra syntax. Used by enum_macro.h for variant processing.
 */
#define SPACETIMEDB_FOR_EACH_VARIANT(macro, sep, ...) \
    SPACETIMEDB_FOR_EACH_VARIANT_IMPL(macro, sep, ##__VA_ARGS__)

#define SPACETIMEDB_FOR_EACH_VARIANT_IMPL(macro, sep, ...) \
    SPACETIMEDB_GET_MACRO(__VA_ARGS__, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_50, SPACETIMEDB_FOR_EACH_VARIANT_FE_49, SPACETIMEDB_FOR_EACH_VARIANT_FE_48, SPACETIMEDB_FOR_EACH_VARIANT_FE_47, SPACETIMEDB_FOR_EACH_VARIANT_FE_46, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_45, SPACETIMEDB_FOR_EACH_VARIANT_FE_44, SPACETIMEDB_FOR_EACH_VARIANT_FE_43, SPACETIMEDB_FOR_EACH_VARIANT_FE_42, SPACETIMEDB_FOR_EACH_VARIANT_FE_41, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_40, SPACETIMEDB_FOR_EACH_VARIANT_FE_39, SPACETIMEDB_FOR_EACH_VARIANT_FE_38, SPACETIMEDB_FOR_EACH_VARIANT_FE_37, SPACETIMEDB_FOR_EACH_VARIANT_FE_36, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_35, SPACETIMEDB_FOR_EACH_VARIANT_FE_34, SPACETIMEDB_FOR_EACH_VARIANT_FE_33, SPACETIMEDB_FOR_EACH_VARIANT_FE_32, SPACETIMEDB_FOR_EACH_VARIANT_FE_31, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_30, SPACETIMEDB_FOR_EACH_VARIANT_FE_29, SPACETIMEDB_FOR_EACH_VARIANT_FE_28, SPACETIMEDB_FOR_EACH_VARIANT_FE_27, SPACETIMEDB_FOR_EACH_VARIANT_FE_26, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_25, SPACETIMEDB_FOR_EACH_VARIANT_FE_24, SPACETIMEDB_FOR_EACH_VARIANT_FE_23, SPACETIMEDB_FOR_EACH_VARIANT_FE_22, SPACETIMEDB_FOR_EACH_VARIANT_FE_21, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_20, SPACETIMEDB_FOR_EACH_VARIANT_FE_19, SPACETIMEDB_FOR_EACH_VARIANT_FE_18, SPACETIMEDB_FOR_EACH_VARIANT_FE_17, SPACETIMEDB_FOR_EACH_VARIANT_FE_16, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_15, SPACETIMEDB_FOR_EACH_VARIANT_FE_14, SPACETIMEDB_FOR_EACH_VARIANT_FE_13, SPACETIMEDB_FOR_EACH_VARIANT_FE_12, SPACETIMEDB_FOR_EACH_VARIANT_FE_11, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_10, SPACETIMEDB_FOR_EACH_VARIANT_FE_9, SPACETIMEDB_FOR_EACH_VARIANT_FE_8, SPACETIMEDB_FOR_EACH_VARIANT_FE_7, SPACETIMEDB_FOR_EACH_VARIANT_FE_6, \
        SPACETIMEDB_FOR_EACH_VARIANT_FE_5, SPACETIMEDB_FOR_EACH_VARIANT_FE_4, SPACETIMEDB_FOR_EACH_VARIANT_FE_3, SPACETIMEDB_FOR_EACH_VARIANT_FE_2, SPACETIMEDB_FOR_EACH_VARIANT_FE_1) \
    (macro, sep, ##__VA_ARGS__)

// Adapter macros that apply macro + separator pattern (instead of macro-obj-extra pattern)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_1(macro, sep, X) macro(X)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_2(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_1(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_3(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_2(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_4(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_3(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_5(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_4(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_6(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_5(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_7(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_6(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_8(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_7(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_9(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_8(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_10(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_9(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_11(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_10(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_12(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_11(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_13(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_12(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_14(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_13(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_15(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_14(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_16(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_15(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_17(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_16(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_18(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_17(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_19(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_18(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_20(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_19(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_21(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_20(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_22(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_21(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_23(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_22(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_24(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_23(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_25(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_24(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_26(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_25(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_27(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_26(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_28(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_27(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_29(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_28(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_30(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_29(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_31(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_30(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_32(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_31(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_33(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_32(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_34(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_33(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_35(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_34(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_36(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_35(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_37(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_36(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_38(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_37(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_39(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_38(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_40(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_39(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_41(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_40(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_42(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_41(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_43(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_42(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_44(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_43(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_45(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_44(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_46(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_45(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_47(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_46(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_48(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_47(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_49(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_48(macro, sep, __VA_ARGS__)
#define SPACETIMEDB_FOR_EACH_VARIANT_FE_50(macro, sep, X, ...) macro(X) sep() SPACETIMEDB_FOR_EACH_VARIANT_FE_49(macro, sep, __VA_ARGS__)

// Compatibility aliases for files that expect non-prefixed names
#define FOR_EACH_VARIANT SPACETIMEDB_FOR_EACH_VARIANT

// =============================================================================
// INTERNAL UTILITIES
// =============================================================================

// REMOVED: internal::get_table_id - unused dead code

// =============================================================================
// TYPE GENERATION AND REGISTRATION
// =============================================================================

// REMOVED: spacetimedb_generate_type<T>() - 67 lines of unused dead code

// =============================================================================
// TABLE MACROS - C# [SpacetimeDB.Table] Equivalent
// =============================================================================

// Enhanced table macro with optional scheduling support
// Usage: SPACETIMEDB_TABLE(MyStruct, "table_name", Public)
//    or: SPACETIMEDB_TABLE(MyStruct, "table_name", Private)
// REMOVED: SPACETIMEDB_TABLE is now defined in field_metadata.h
// #define SPACETIMEDB_TABLE(...) \
//     SPACETIMEDB_PASTE(SPACETIMEDB_TABLE_, SPACETIMEDB_NARGS(__VA_ARGS__))(__VA_ARGS__)

// All table macros have been moved to field_metadata.h to avoid duplicates.
// The SPACETIMEDB_TABLE macro in field_metadata.h supports:
//   - SPACETIMEDB_TABLE(Type, table_name, Public)
//   - SPACETIMEDB_TABLE(Type, table_name, Private)
//   - Multiple tables using the same struct type

// =============================================================================
// REDUCER MACROS - MOVED TO reducer_macros.h
// =============================================================================

// All reducer macros have been moved to reducer_macros.h
// This includes SPACETIMEDB_REDUCER, SPACETIMEDB_INIT, 
// SPACETIMEDB_CLIENT_CONNECTED, and SPACETIMEDB_CLIENT_DISCONNECTED




// =============================================================================
// CONSTRAINT AND FIELD ATTRIBUTES
// =============================================================================
// Use SPACETIMEDB_TABLE with constraint macros for field attributes:
//   SPACETIMEDB_TABLE(MyTable, "my_table", Public,
//       PrimaryKey(id), Unique(email), Index(name)
//   )

// =============================================================================
// VISIBILITY FILTER MACRO
// =============================================================================

/**
 * @brief Register a client visibility filter with the SpacetimeDB module system
 * 
 * This macro registers a SQL string as a client visibility filter.
 * 
 * @param filter_name The unique name for this filter
 * @param sql_query The SQL query string for the filter
 * 
 * Usage:
 *    SPACETIMEDB_CLIENT_VISIBILITY_FILTER(user_owns_data, "SELECT * FROM user_data WHERE owner_id = current_user_identity()")
 */
#define SPACETIMEDB_CLIENT_VISIBILITY_FILTER(filter_name, sql_query) \
    __attribute__((export_name("__preinit__25_register_row_level_security_" #filter_name))) \
    extern "C" void __register_client_visibility_filter_##filter_name() { \
        SpacetimeDb::Internal::getV9Builder().RegisterRowLevelSecurity(sql_query); \
    }

// =============================================================================
// STRUCT TYPE MACROS  
// =============================================================================

/**
 * @brief Enable BSATN serialization for unit struct types
 * 
 * Unit structs are structs with no fields, similar to std::monostate.
 * They serialize/deserialize as empty (0 bytes).
 * 
 * Usage:
 *   struct UnitType {};
 *   SPACETIMEDB_UNIT_STRUCT(UnitType)
 */
#define SPACETIMEDB_UNIT_STRUCT(Type) \
    struct Type { \
        static constexpr bool __is_unit_type__ = true; \
        Type() = default; \
        Type(std::monostate) {} \
        operator std::monostate() const { return std::monostate{}; } \
    }; \
    template<> \
    struct SpacetimeDb::bsatn::bsatn_traits<Type> { \
        static void serialize(SpacetimeDb::bsatn::Writer& w, const Type& v) { \
            /* Unit struct serializes as empty */ \
        } \
        static Type deserialize(SpacetimeDb::bsatn::Reader& r) { \
            return Type{}; \
        } \
        static SpacetimeDb::bsatn::AlgebraicType algebraic_type() { \
            return SpacetimeDb::Internal::LazyTypeRegistrar<Type>::getOrRegister( \
                []() -> SpacetimeDb::bsatn::AlgebraicType { \
                    return SpacetimeDb::bsatn::AlgebraicType::make_product( \
                        std::make_unique<SpacetimeDb::bsatn::ProductType>(std::vector<SpacetimeDb::bsatn::ProductTypeElement>{}) \
                    ); \
                }, \
                #Type \
            ); \
        } \
    }; \
    SPACETIMEDB_GENERATE_TYPE_REGISTRATION_BUNDLE(Type)

/**
 * @brief Enable BSATN serialization for struct types with fields
 * 
 * Generates complete serialization support for structs by serializing
 * each field in the order specified.
 * 
 * Usage:
 *   struct MyStruct {
 *       int32_t id;
 *       std::string name;
 *   };
 *   SPACETIMEDB_STRUCT(MyStruct, id, name)
 */
#define SPACETIMEDB_STRUCT(Type, ...) \
    template<> \
    struct SpacetimeDb::bsatn::bsatn_traits<Type> { \
        static void serialize(SpacetimeDb::bsatn::Writer& w, const Type& v) { \
            SPACETIMEDB_SERIALIZE_FIELDS(v, w, __VA_ARGS__) \
        } \
        static Type deserialize(SpacetimeDb::bsatn::Reader& r) { \
            Type v; \
            SPACETIMEDB_DESERIALIZE_FIELDS(v, r, __VA_ARGS__) \
            return v; \
        } \
        static SpacetimeDb::bsatn::AlgebraicType algebraic_type() { \
            fprintf(stderr, "[DEBUG] bsatn_traits<" #Type ">::algebraic_type() called\n"); \
            fflush(stderr); \
            return SpacetimeDb::Internal::LazyTypeRegistrar<Type>::getOrRegister( \
                []() -> SpacetimeDb::bsatn::AlgebraicType { \
                    fprintf(stderr, "[DEBUG] Building ProductType for " #Type "\n"); \
                    fflush(stderr); \
                    SpacetimeDb::bsatn::ProductTypeBuilder builder; \
                    SPACETIMEDB_REGISTER_FIELDS(Type, builder, __VA_ARGS__) \
                    fprintf(stderr, "[DEBUG] Finished building ProductType for " #Type "\n"); \
                    fflush(stderr); \
                    return SpacetimeDb::bsatn::AlgebraicType::make_product(builder.build()); \
                }, \
                #Type \
            ); \
        } \
    }; \
    SPACETIMEDB_GENERATE_TYPE_REGISTRATION_BUNDLE_WITH_FIELDS(Type, __VA_ARGS__)

// Field processing helper macros (used by SPACETIMEDB_STRUCT)
#define SPACETIMEDB_SERIALIZE_FIELD(obj, writer, field) \
    SpacetimeDb::bsatn::serialize(writer, obj.field);
    
#define SPACETIMEDB_DESERIALIZE_FIELD(obj, reader, field) \
    obj.field = SpacetimeDb::bsatn::deserialize<decltype(obj.field)>(reader);
    
#define SPACETIMEDB_REGISTER_FIELD(Type, builder, field) \
    builder.with_field<decltype(Type::field)>(#field);

#define SPACETIMEDB_SERIALIZE_FIELDS(obj, writer, ...) \
    SPACETIMEDB_FOR_EACH_ARG(SPACETIMEDB_SERIALIZE_FIELD, obj, writer, __VA_ARGS__)
    
#define SPACETIMEDB_DESERIALIZE_FIELDS(obj, reader, ...) \
    SPACETIMEDB_FOR_EACH_ARG(SPACETIMEDB_DESERIALIZE_FIELD, obj, reader, __VA_ARGS__)
    
#define SPACETIMEDB_REGISTER_FIELDS(Type, builder, ...) \
    SPACETIMEDB_FOR_EACH_ARG(SPACETIMEDB_REGISTER_FIELD, Type, builder, __VA_ARGS__)

// Field descriptor registration for runtime reflection
#define SPACETIMEDB_REGISTER_FIELD_DESCRIPTOR(Type, dummy, field) \
    { \
        ::SpacetimeDb::FieldDescriptor desc; \
        desc.name = #field; \
        desc.offset = offsetof(Type, field); \
        desc.size = sizeof(decltype(Type::field)); \
        desc.write_type = [](std::vector<uint8_t>& buf) { \
            ::SpacetimeDb::write_field_type<decltype(Type::field)>(buf); \
        }; \
        desc.get_algebraic_type = []() { \
            return ::SpacetimeDb::bsatn::bsatn_traits<decltype(Type::field)>::algebraic_type(); \
        }; \
        desc.serialize = [](std::vector<uint8_t>& buf, const void* obj) { \
            const Type* typed_obj = static_cast<const Type*>(obj); \
            ::SpacetimeDb::serialize_value(buf, typed_obj->field); \
        }; \
        desc.get_type_name = []() -> std::string { \
            return demangle_cpp_type_name(typeid(decltype(Type::field)).name()); \
        }; \
        ::SpacetimeDb::get_table_descriptors()[&typeid(Type)].fields.push_back(desc); \
    }

#define SPACETIMEDB_REGISTER_FIELD_DESCRIPTORS(Type, ...) \
    SPACETIMEDB_FOR_EACH_ARG(SPACETIMEDB_REGISTER_FIELD_DESCRIPTOR, Type, dummy, __VA_ARGS__)


#endif // SPACETIMEDB_MACROS_H