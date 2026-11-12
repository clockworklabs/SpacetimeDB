#pragma once
// Put SPACETIMEDB_ENV in a module declaration header selected by CMake's
// SPACETIMEDB_ENV_HEADER, before including any SDK context or umbrella header.
#ifndef SPACETIMEDB_ENV_HEADER_ACTIVE
#error "Configure SPACETIMEDB_ENV_HEADER before adding the SDK CMake directory"
#endif
#ifdef SPACETIMEDB_ENVIRONMENT_H
#error "The environment declaration header must precede every SDK include"
#endif
#define SPACETIMEDB_ENV_DECLARATION 1
#include <spacetimedb/environment.h>

#define STDB_ENV_CAT_I(a, b) a##b
#define STDB_ENV_CAT(a, b) STDB_ENV_CAT_I(a, b)
#define STDB_ENV_SECOND(a, b, ...) b
#define STDB_ENV_PROBE() ignored, 1
#define STDB_ENV_CHECK(...) STDB_ENV_SECOND(__VA_ARGS__, 0)
#define STDB_ENV_RESERVED(name) STDB_ENV_CHECK(STDB_ENV_CAT(STDB_ENV_RESERVED_, name))
#define STDB_ENV_KEEP_0(...) __VA_ARGS__
#define STDB_ENV_KEEP_1(...)
#define STDB_ENV_RESERVED_alignas STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_alignof STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_and STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_and_eq STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_asm STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_atomic_cancel STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_atomic_commit STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_atomic_noexcept STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_auto STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_bitand STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_bitor STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_bool STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_break STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_case STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_catch STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_char STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_char8_t STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_char16_t STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_char32_t STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_class STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_compl STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_concept STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_const STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_consteval STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_constexpr STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_constinit STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_const_cast STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_continue STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_co_await STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_co_return STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_co_yield STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_decltype STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_default STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_delete STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_do STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_double STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_dynamic_cast STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_else STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_enum STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_explicit STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_export STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_extern STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_false STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_float STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_for STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_friend STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_goto STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_if STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_inline STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_int STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_long STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_mutable STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_namespace STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_new STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_noexcept STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_not STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_not_eq STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_nullptr STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_operator STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_or STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_or_eq STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_private STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_protected STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_public STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_reflexpr STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_register STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_reinterpret_cast STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_requires STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_return STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_short STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_signed STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_sizeof STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_static STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_static_assert STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_static_cast STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_struct STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_switch STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_synchronized STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_template STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_this STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_thread_local STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_throw STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_true STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_try STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_typedef STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_typeid STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_typename STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_union STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_unsigned STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_using STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_virtual STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_void STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_volatile STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_wchar_t STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_while STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_xor STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_xor_eq STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_get STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_Environment STDB_ENV_PROBE()
#define STDB_ENV_RESERVED_EnvironmentBase STDB_ENV_PROBE()

#define STDB_ENV_PARENS ()
#define STDB_ENV_EVAL1(...) __VA_ARGS__
#define STDB_ENV_EVAL2(...) STDB_ENV_EVAL1(STDB_ENV_EVAL1(STDB_ENV_EVAL1(STDB_ENV_EVAL1(__VA_ARGS__))))
#define STDB_ENV_EVAL3(...) STDB_ENV_EVAL2(STDB_ENV_EVAL2(STDB_ENV_EVAL2(STDB_ENV_EVAL2(__VA_ARGS__))))
#define STDB_ENV_EVAL4(...) STDB_ENV_EVAL3(STDB_ENV_EVAL3(STDB_ENV_EVAL3(STDB_ENV_EVAL3(__VA_ARGS__))))
#define STDB_ENV_EVAL(...) STDB_ENV_EVAL4(STDB_ENV_EVAL4(STDB_ENV_EVAL4(STDB_ENV_EVAL4(__VA_ARGS__))))
#define STDB_ENV_EACH(macro, ...) __VA_OPT__(STDB_ENV_EVAL(STDB_ENV_EACH_I(macro, __VA_ARGS__)))
#define STDB_ENV_EACH_I(macro, tuple, ...) macro tuple __VA_OPT__(STDB_ENV_AGAIN STDB_ENV_PARENS (macro, __VA_ARGS__))
#define STDB_ENV_AGAIN() STDB_ENV_EACH_I
#define STDB_ENV_UNPAREN(...) __VA_ARGS__
#define STDB_ENV_MEMBER(name, type, ...) \
    STDB_ENV_CAT(STDB_ENV_KEEP_, STDB_ENV_RESERVED(name))( \
        type name() const { return ::SpacetimeDB::Internal::read_environment<type>(#name); } \
    )
#define STDB_ENV_METADATA(name, type, ...) \
    ::SpacetimeDB::Internal::declare_environment<type>(#name __VA_OPT__(, { STDB_ENV_UNPAREN __VA_ARGS__ })),

/// Declare the complete schema, never live values. One declaration per module.
/// Empty SPACETIMEDB_ENV() is supported. Reserved names retain generic get().
#define SPACETIMEDB_ENV(...) \
    namespace SpacetimeDB { \
    class Environment : public EnvironmentBase { \
    public: STDB_ENV_EACH(STDB_ENV_MEMBER, __VA_ARGS__) \
    }; \
    namespace Internal { \
    inline const bool environment_schema_registered = [] { \
        environment_declarations() = { STDB_ENV_EACH(STDB_ENV_METADATA, __VA_ARGS__) }; \
        validate_environment_declarations(); \
        return true; \
    }(); \
    } \
    }
