#pragma once

#include "spacetimedb/query_builder/expr.h"
#include "spacetimedb/query_builder/join.h"
#include "spacetimedb/query_builder/table.h"

#include <optional>
#include <type_traits>
#include <utility>
#include <vector>

namespace SpacetimeDB {

template<typename TRow>
using Query = query_builder::RawQuery<TRow>;

namespace detail {

template<typename TRow>
struct NamedQuerySourceTag {
    using type = TRow;
    const char* __table_name_internal;
};

struct NotAQuerySourceTag {};

template<typename T>
struct query_source_row_type {};

template<typename T>
concept HasQuerySourceRowType = requires {
    typename query_source_row_type<std::remove_cvref_t<T>>::type;
};

template<typename T>
using query_source_row_type_t = typename query_source_row_type<std::remove_cvref_t<T>>::type;

template<typename T>
    requires query_builder::QueryBuilderReturn<T>
struct query_source_row_type<T> {
    using type = query_builder::query_row_type_t<T>;
};

template<typename TRow>
struct query_source_row_type<std::vector<TRow>> {
    using type = TRow;
};

template<typename TRow>
struct query_source_row_type<std::optional<TRow>> {
    using type = TRow;
};

template<typename TReturn>
constexpr auto MakeQuerySourceTag(const char* source_name) {
    if constexpr (HasQuerySourceRowType<TReturn>) {
        return NamedQuerySourceTag<query_source_row_type_t<TReturn>>{source_name};
    } else {
        return NotAQuerySourceTag{};
    }
}

template<typename TSourceTag>
constexpr const char* GetQuerySourceName(const TSourceTag& tag) {
    return tag.__table_name_internal;
}

} // namespace detail

class QueryBuilder {
public:
    template<typename TRow, typename TCols, typename TIxCols>
    [[nodiscard]] constexpr query_builder::Table<TRow, TCols, TIxCols> table(const char* table_name, TCols cols, TIxCols ix_cols) const {
        return query_builder::Table<TRow, TCols, TIxCols>(table_name, std::move(cols), std::move(ix_cols));
    }

    template<typename TTableTag>
    [[nodiscard]] constexpr auto table(TTableTag tag) const
        -> query_builder::Table<
            typename std::remove_cvref_t<TTableTag>::type,
            decltype(query_builder::HasCols<typename std::remove_cvref_t<TTableTag>::type>::get(std::declval<const char*>())),
            decltype(query_builder::HasIxCols<typename std::remove_cvref_t<TTableTag>::type>::get(std::declval<const char*>()))> {
        using TRow = typename std::remove_cvref_t<TTableTag>::type;
        const char* table_name = detail::GetQuerySourceName(tag);
        return table<TRow>(
            table_name,
            query_builder::HasCols<TRow>::get(table_name),
            query_builder::HasIxCols<TRow>::get(table_name));
    }

    template<typename TTableTag>
    [[nodiscard]] constexpr auto operator[](TTableTag tag) const
        -> decltype(table(tag)) {
        return table(tag);
    }
};

} // namespace SpacetimeDB
