#pragma once

/**
 * @file outbox.h
 * @brief Outbox tables for asynchronous inter-database communication (IDC).
 *
 * Each row of an outbox table is a call to one reducer in another database.
 * After the inserting transaction commits, SpacetimeDB delivers the row as a call
 * to the receiver's reducer, and later runs the outbox's on_result reducer, if any,
 * with the row and the call's result.
 *
 * @code
 * #include <spacetimedb.h>
 * #include <game_world/bindings.h>
 *
 * using namespace SpacetimeDB;
 * namespace gw = game_world;
 *
 * struct AdmitPlayerOutbox {
 *     uint64_t msg_id;
 *     gw::Identity world;
 *     std::string name;
 * };
 * SPACETIMEDB_STRUCT(AdmitPlayerOutbox, msg_id, world, name)
 * SPACETIMEDB_OUTBOX_TABLE(AdmitPlayerOutbox, admit_player_outbox, Private, gw::reducers::admit_player)
 * FIELD_PrimaryKeyAutoInc(admit_player_outbox, msg_id)
 * FIELD_Target(admit_player_outbox, world)
 *
 * SPACETIMEDB_REDUCER_ON_RESULT(admit_player_outbox, on_admit_player_result,
 *                               ReducerContext ctx, AdmitPlayerOutbox row, OutboxResult result) {
 *     if (result.is_err()) { ... }
 *     return Ok();
 * }
 * @endcode
 *
 * Rules for an outbox table, checked when the module is described:
 * - The primary key is a single `uint64_t` column declared with FIELD_PrimaryKeyAutoInc.
 * - Exactly one column is declared with FIELD_Target, and its type is the receiver
 *   bindings' `Identity`.
 * - For each receiver parameter, exactly one column has the same name and type.
 *   FIELD_Param maps a column with a different name to a parameter.
 * - Other columns are allowed. They aren't sent, but on_result can read them.
 *
 * Reducers can insert and read outbox rows, but can't update or delete them.
 */

#include <string>
#include <tuple>
#include <type_traits>
#include "spacetimedb/idc.h"
#include "spacetimedb/database.h"
#include "spacetimedb/readonly_field_accessors.h"
#include "spacetimedb/table_with_constraints.h"
#include "spacetimedb/reducer_macros.h"

namespace SpacetimeDB {

/// The result of an outbox call, as passed to an on_result reducer.
using OutboxResult = Result<std::monostate, std::string>;

/**
 * @brief Access to an outbox table from a reducer: insert, count and iterate.
 *
 * Outbox rows can't be updated or deleted. SpacetimeDB deletes each row once its
 * call has finished and its on_result reducer, if any, has run.
 */
template<typename T>
class OutboxTableAccessor {
public:
    explicit OutboxTableAccessor(const std::string& table_name) : table_(table_name) {}

    /// Insert a call. The returned row has its message ID filled in.
    T insert(const T& row) const { return table_.insert(row); }

    uint64_t count() const { return table_.count(); }
    auto begin() const { return table_.begin(); }
    auto end() const { return table_.end(); }

    T update(const T&) const = delete;
    uint32_t delete_by_value(const T&) const = delete;
    uint64_t clear() const = delete;

private:
    TableAccessor<T> table_;
};

namespace detail {

template<typename Tag>
using outbox_receiver_t = typename Tag::__outbox_receiver;

/// The receiver parameter type at `Index`, or `void` if there is none.
template<typename Handle, std::size_t Index, typename = void>
struct outbox_param_type {
    using type = void;
};

template<typename Handle, std::size_t Index>
struct outbox_param_type<Handle, Index,
                         std::enable_if_t<(Index < std::tuple_size_v<typename Handle::param_types>)>> {
    using type = std::tuple_element_t<Index, typename Handle::param_types>;
};

template<typename Tag, typename Fn>
struct on_result_signature {
    static constexpr bool ctx_ok = false;
    static constexpr bool row_ok = false;
    static constexpr bool result_ok = false;
};

template<typename Tag, typename R, typename Ctx, typename Row, typename Res>
struct on_result_signature<Tag, R (*)(Ctx, Row, Res)> {
    static constexpr bool ctx_ok = std::is_same_v<std::remove_cvref_t<Ctx>, ReducerContext>;
    static constexpr bool row_ok = std::is_same_v<std::remove_cvref_t<Row>, typename Tag::type>;
    static constexpr bool result_ok = std::is_same_v<std::remove_cvref_t<Res>, OutboxResult>;
};

} // namespace detail

} // namespace SpacetimeDB

// =============================================================================
// Outbox table declaration
// =============================================================================

#define SPACETIMEDB_OUTBOX_TABLE_IMPL(type, table_name, access_enum, receiver, canonical_name) \
    static_assert(::SpacetimeDB::RemoteReducerHandle<receiver>, \
        "The outbox receiver must be a reducer handle from generated receiver bindings, like game_world::reducers::admit_player."); \
    extern "C" __attribute__((export_name("__preinit__20_register_table_" #type "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__20_register_table_, SPACETIMEDB_PASTE(type, SPACETIMEDB_PASTE(_line_, __LINE__)))() { \
        bool is_public = (access_enum == SpacetimeDB::Internal::TableAccess::Public); \
        SpacetimeDB::Module::RegisterTable<type>(#table_name, is_public, false); \
        const char* _spacetimedb_canonical_name = (canonical_name); \
        if (_spacetimedb_canonical_name != nullptr) { \
            SpacetimeDB::Module::RegisterExplicitTableName(#table_name, _spacetimedb_canonical_name); \
        } \
        SpacetimeDB::Internal::getV10Builder().RegisterOutbox<type, receiver>(#table_name); \
    } \
    inline std::bool_constant<true> lookup_table_allowed(::SpacetimeDB::query_builder::detail::row_tag<type>); \
    template<> \
    struct ::SpacetimeDB::is_outbox_row<type> : std::true_type {}; \
    struct SPACETIMEDB_PASTE(table_name, _tag_type) : SpacetimeDB::TableTag<type> { \
        static constexpr const char* __table_name_internal = #table_name; \
        static constexpr bool __is_event_internal = false; \
        static constexpr bool __is_outbox_internal = true; \
        using __outbox_receiver = receiver; \
    }; \
    constexpr SPACETIMEDB_PASTE(table_name, _tag_type) table_name{};

#define SPACETIMEDB_OUTBOX_TABLE_4(type, table_name, access_enum, receiver) \
    SPACETIMEDB_OUTBOX_TABLE_IMPL(type, table_name, access_enum, receiver, nullptr)

#define SPACETIMEDB_OUTBOX_TABLE_5(type, table_name, access_enum, receiver, canonical_name) \
    SPACETIMEDB_OUTBOX_TABLE_IMPL(type, table_name, access_enum, receiver, canonical_name)

#define SPACETIMEDB_OUTBOX_TABLE_SELECT(_1, _2, _3, _4, _5, NAME, ...) NAME

/**
 * @brief Declare an outbox table whose rows call `receiver` in another database.
 *
 * Usage: SPACETIMEDB_OUTBOX_TABLE(Type, table_name, Private, game_world::reducers::admit_player)
 *        SPACETIMEDB_OUTBOX_TABLE(Type, table_name, Public, receiver, "canonical_name")
 *
 * Then declare the message ID and target columns:
 *   FIELD_PrimaryKeyAutoInc(table_name, msg_id)
 *   FIELD_Target(table_name, world)
 */
#define SPACETIMEDB_OUTBOX_TABLE(...) \
    SPACETIMEDB_OUTBOX_TABLE_SELECT(__VA_ARGS__, SPACETIMEDB_OUTBOX_TABLE_5, SPACETIMEDB_OUTBOX_TABLE_4, ~, ~, ~)(__VA_ARGS__)

// =============================================================================
// Outbox column annotations
// =============================================================================

/**
 * @brief Declare the column that holds the identity of the database to call.
 *
 * Its type must be the receiver bindings' `Identity`, like `game_world::Identity`.
 */
#define FIELD_Target(table_name, field_name) \
    static_assert(::SpacetimeDB::is_outbox_tag_v<SPACETIMEDB_PASTE(table_name, _tag_type)>, \
        "FIELD_Target applies only to outbox tables declared with SPACETIMEDB_OUTBOX_TABLE."); \
    static_assert(std::is_same_v< \
            decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name), \
            typename ::SpacetimeDB::detail::outbox_receiver_t<SPACETIMEDB_PASTE(table_name, _tag_type)>::identity_type>, \
        "The outbox target column '" #field_name "' must have the receiver bindings' Identity type."); \
    extern "C" __attribute__((export_name("__preinit__21_outbox_target_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_outbox_target_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV10Builder().SetOutboxTarget(#table_name, #field_name); \
    }

/**
 * @brief Supply receiver parameter `param_name` from column `field_name`.
 *
 * Without FIELD_Param, a column supplies the parameter with the same name.
 */
#define FIELD_Param(table_name, field_name, param_name) \
    static_assert(::SpacetimeDB::is_outbox_tag_v<SPACETIMEDB_PASTE(table_name, _tag_type)>, \
        "FIELD_Param applies only to outbox tables declared with SPACETIMEDB_OUTBOX_TABLE."); \
    static_assert(::SpacetimeDB::detail::remote_param_index< \
            ::SpacetimeDB::detail::outbox_receiver_t<SPACETIMEDB_PASTE(table_name, _tag_type)>>(#param_name) < \
            std::tuple_size_v<typename ::SpacetimeDB::detail::outbox_receiver_t<SPACETIMEDB_PASTE(table_name, _tag_type)>::param_types>, \
        "FIELD_Param(" #table_name ", " #field_name ", " #param_name "): the receiver reducer has no parameter named '" #param_name "'."); \
    static_assert(std::is_same_v< \
            decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name), \
            typename ::SpacetimeDB::detail::outbox_param_type< \
                ::SpacetimeDB::detail::outbox_receiver_t<SPACETIMEDB_PASTE(table_name, _tag_type)>, \
                ::SpacetimeDB::detail::remote_param_index< \
                    ::SpacetimeDB::detail::outbox_receiver_t<SPACETIMEDB_PASTE(table_name, _tag_type)>>(#param_name)>::type>, \
        "FIELD_Param(" #table_name ", " #field_name ", " #param_name "): the column's type doesn't match the receiver parameter's type."); \
    extern "C" __attribute__((export_name("__preinit__21_outbox_param_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_outbox_param_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV10Builder().AddOutboxParamAlias(#table_name, #field_name, #param_name); \
    }

// =============================================================================
// on_result reducers
// =============================================================================

/**
 * @brief Declare the reducer that SpacetimeDB runs with each finished call's result.
 *
 * It takes the outbox row and an OutboxResult, and runs once per row, in the same
 * transaction that deletes the row. Clients can't call it.
 *
 * Usage:
 * @code
 * SPACETIMEDB_REDUCER_ON_RESULT(admit_player_outbox, on_admit_player_result,
 *                               ReducerContext ctx, AdmitPlayerOutbox row, OutboxResult result) {
 *     return Ok();
 * }
 * @endcode
 */
#define SPACETIMEDB_REDUCER_ON_RESULT(outbox_table, name, ctx_param, ...) \
    SpacetimeDB::ReducerResult name(ctx_param, __VA_ARGS__); \
    static_assert(::SpacetimeDB::is_outbox_tag_v<SPACETIMEDB_PASTE(outbox_table, _tag_type)>, \
        "SPACETIMEDB_REDUCER_ON_RESULT requires an outbox table declared with SPACETIMEDB_OUTBOX_TABLE."); \
    static_assert(::SpacetimeDB::detail::on_result_signature<SPACETIMEDB_PASTE(outbox_table, _tag_type), decltype(&name)>::ctx_ok, \
        "The first parameter of an on_result reducer must be ReducerContext."); \
    static_assert(::SpacetimeDB::detail::on_result_signature<SPACETIMEDB_PASTE(outbox_table, _tag_type), decltype(&name)>::row_ok, \
        "The second parameter of an on_result reducer must be the outbox table's row type."); \
    static_assert(::SpacetimeDB::detail::on_result_signature<SPACETIMEDB_PASTE(outbox_table, _tag_type), decltype(&name)>::result_ok, \
        "The third parameter of an on_result reducer must be SpacetimeDB::OutboxResult."); \
    __attribute__((export_name("__preinit__30_reducer_" #name))) \
    extern "C" void CONCAT(_spacetimedb_preinit_register_, name)() { \
        /* Parameter names aren't visible to clients, since on_result reducers are private. */ \
        SpacetimeDB::Internal::getV10Builder().RegisterReducer(#name, name, {"row", "result"}); \
        SpacetimeDB::Internal::getV10Builder().SetOutboxOnResult(#outbox_table, #name); \
    } \
    SpacetimeDB::ReducerResult name(ctx_param, __VA_ARGS__)
