#pragma once

#if 0
// bindings-cpp source include kept here for traceability:
// #include "spacetimedb/bsatn/traits.h"
#include "BSATN/Core/traits.h"
#endif
#include "QueryBuilder/expr.h"

#include <cstdio>
#include <concepts>
#include <string>
#include <type_traits>
#include <utility>

namespace SpacetimeDB::query_builder {

template<typename T>
struct query_row_type;

template<typename TRow>
class Table;

template<typename TRow>
class FromWhere;

template<typename TRow>
class LeftSemiJoin;

template<typename TRightRow, typename TLeftRow>
class RightSemiJoin;

template<typename T>
using query_row_type_t = typename query_row_type<std::remove_cvref_t<T>>::type;

template<typename TRow>
class RawQuery {
public:
    using row_type = TRow;

    explicit RawQuery(std::string sql)
        : sql_(std::move(sql)) {}

    template<typename TQuery>
        requires(!std::same_as<std::remove_cvref_t<TQuery>, RawQuery> &&
                 requires { typename query_row_type_t<TQuery>; } &&
                 std::same_as<query_row_type_t<TQuery>, TRow> &&
                 requires(TQuery&& query) { { std::forward<TQuery>(query).into_sql() } -> std::convertible_to<std::string>; })
    RawQuery(TQuery&& query)
        : sql_(std::forward<TQuery>(query).into_sql()) {}

    [[nodiscard]] const std::string& sql() const { return sql_; }
    [[nodiscard]] std::string into_sql() const { return sql_; }

private:
    std::string sql_;
};

template<typename TRow>
concept QueryLike = requires(const TRow& query) {
    { query.into_sql() } -> std::convertible_to<std::string>;
};

template<typename T>
concept QueryBuilderReturn = requires {
    typename query_row_type_t<T>;
} && QueryLike<std::remove_cvref_t<T>>;

template<typename TRow>
struct HasCols;

template<typename TRow>
struct HasIxCols;

template<typename TRow>
struct CanBeLookupTable : std::false_type {};

namespace detail {

template<typename TRow>
struct row_tag {};

inline std::false_type lookup_table_allowed(...);

template<typename TRow>
auto adl_lookup_table_allowed(int) -> decltype(lookup_table_allowed(row_tag<TRow>{}));

template<typename TRow>
std::false_type adl_lookup_table_allowed(...);

} // namespace detail

template<typename TRow>
inline constexpr bool can_be_lookup_table_v =
    CanBeLookupTable<TRow>::value || decltype(detail::adl_lookup_table_allowed<TRow>(0))::value;

template<typename TRow>
class Table {
public:
    using row_type = TRow;

    explicit constexpr Table(const char* table_name)
        : table_name_(table_name) {}

    [[nodiscard]] constexpr const char* name() const { return table_name_; }

    [[nodiscard]] RawQuery<TRow> build() const {
        std::string sql;
        sql.reserve(16 + std::char_traits<char>::length(table_name_));
        sql += "SELECT * FROM \"";
        sql += table_name_;
        sql += "\"";
        return RawQuery<TRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const {
        return build().into_sql();
    }

    template<typename TFn>
    [[nodiscard]] auto where(TFn&& predicate) const;
    template<typename TFn>
    [[nodiscard]] auto where_ix(TFn&& predicate) const;
    template<typename TFn>
    [[nodiscard]] auto Where(TFn&& predicate) const;

    template<typename TFn>
    [[nodiscard]] auto filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }
    template<typename TFn>
    [[nodiscard]] auto Filter(TFn&& predicate) const {
        return Where(std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TFn>
    [[nodiscard]] LeftSemiJoin<TRow> left_semijoin(const Table<TRightRow>& right, TFn&& predicate) const;
    template<typename TRightRow, typename TFn>
    [[nodiscard]] LeftSemiJoin<TRow> LeftSemijoin(const Table<TRightRow>& right, TFn&& predicate) const {
        return left_semijoin(right, std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TFn>
    [[nodiscard]] RightSemiJoin<TRightRow, TRow> right_semijoin(const Table<TRightRow>& right, TFn&& predicate) const;
    template<typename TRightRow, typename TFn>
    [[nodiscard]] RightSemiJoin<TRightRow, TRow> RightSemijoin(const Table<TRightRow>& right, TFn&& predicate) const {
        return right_semijoin(right, std::forward<TFn>(predicate));
    }

private:
    const char* table_name_;
};

template<typename TRow>
class FromWhere {
public:
    using row_type = TRow;

    constexpr FromWhere(const char* table_name, BoolExpr<TRow> expr)
        : table_name_(table_name), expr_(std::move(expr)) {}

    [[nodiscard]] constexpr const char* table_name() const { return table_name_; }
    [[nodiscard]] const BoolExpr<TRow>& expr() const { return expr_; }

    [[nodiscard]] RawQuery<TRow> build() const {
        std::string predicate = expr_.format();
        std::string sql;
        sql.reserve(24 + std::char_traits<char>::length(table_name_) + predicate.size());
        sql += "SELECT * FROM \"";
        sql += table_name_;
        sql += "\" WHERE ";
        sql += predicate;
        return RawQuery<TRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const {
        return build().into_sql();
    }

    template<typename TFn>
    [[nodiscard]] FromWhere where(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(HasCols<TRow>::get(table_name_)));
        return FromWhere(table_name_, expr_.and_(extra));
    }
    template<typename TFn>
    [[nodiscard]] FromWhere where_ix(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(HasCols<TRow>::get(table_name_), HasIxCols<TRow>::get(table_name_)));
        return FromWhere(table_name_, expr_.and_(extra));
    }
    template<typename TFn>
    [[nodiscard]] FromWhere Where(TFn&& predicate) const {
        if constexpr (std::is_invocable_v<TFn, decltype(HasCols<TRow>::get(table_name_)), decltype(HasIxCols<TRow>::get(table_name_))>) {
            return where_ix(std::forward<TFn>(predicate));
        } else {
            return where(std::forward<TFn>(predicate));
        }
    }

    template<typename TFn>
    [[nodiscard]] FromWhere filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }
    template<typename TFn>
    [[nodiscard]] FromWhere Filter(TFn&& predicate) const {
        return Where(std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TFn>
    [[nodiscard]] LeftSemiJoin<TRow> left_semijoin(const Table<TRightRow>& right, TFn&& predicate) const;
    template<typename TRightRow, typename TFn>
    [[nodiscard]] LeftSemiJoin<TRow> LeftSemijoin(const Table<TRightRow>& right, TFn&& predicate) const {
        return left_semijoin(right, std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TFn>
    [[nodiscard]] RightSemiJoin<TRightRow, TRow> right_semijoin(const Table<TRightRow>& right, TFn&& predicate) const;
    template<typename TRightRow, typename TFn>
    [[nodiscard]] RightSemiJoin<TRightRow, TRow> RightSemijoin(const Table<TRightRow>& right, TFn&& predicate) const {
        return right_semijoin(right, std::forward<TFn>(predicate));
    }

private:
    const char* table_name_;
    BoolExpr<TRow> expr_;
};

template<typename TRow>
template<typename TFn>
[[nodiscard]] auto Table<TRow>::where(TFn&& predicate) const {
    auto expr = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(HasCols<TRow>::get(table_name_)));
    return FromWhere<TRow>(table_name_, std::move(expr));
}

template<typename TRow>
template<typename TFn>
[[nodiscard]] auto Table<TRow>::where_ix(TFn&& predicate) const {
    auto expr = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(HasCols<TRow>::get(table_name_), HasIxCols<TRow>::get(table_name_)));
    return FromWhere<TRow>(table_name_, std::move(expr));
}

template<typename TRow>
template<typename TFn>
[[nodiscard]] auto Table<TRow>::Where(TFn&& predicate) const {
    if constexpr (std::is_invocable_v<TFn, decltype(HasCols<TRow>::get(table_name_)), decltype(HasIxCols<TRow>::get(table_name_))>) {
        return where_ix(std::forward<TFn>(predicate));
    } else {
        return where(std::forward<TFn>(predicate));
    }
}

template<typename TRow>
struct query_row_type<RawQuery<TRow>> {
    using type = TRow;
};

template<typename TRow>
struct query_row_type<Table<TRow>> {
    using type = TRow;
};

template<typename TRow>
struct query_row_type<FromWhere<TRow>> {
    using type = TRow;
};

} // namespace SpacetimeDB::query_builder

#if 0
// Intentionally disabled in Unreal v1.
// These bindings-cpp-only bsatn/algebraic_type hooks support module-side query-view
// metadata handling for RawQuery<TRow>. The Unreal client query builder reuses the SQL
// generation core but does not participate in bindings-cpp module view registration.
//
// Source of truth: jlarabie/cpp-query-builder
//   crates/bindings-cpp/include/spacetimedb/query_builder/table.h
namespace SpacetimeDB::bsatn {
template<typename TRow>
struct algebraic_type_of<::SpacetimeDB::query_builder::RawQuery<TRow>> {
    static AlgebraicType get() {
        std::vector<ProductTypeElement> elements;
        elements.emplace_back(std::string("__query__"), algebraic_type_of<TRow>::get());
        return AlgebraicType::make_product(std::make_unique<ProductType>(std::move(elements)));
    }
};

template<typename TRow>
struct bsatn_traits<::SpacetimeDB::query_builder::RawQuery<TRow>> {
    static void serialize(Writer&, const ::SpacetimeDB::query_builder::RawQuery<TRow>&) {
        std::fputs("SpacetimeDB bindings-cpp internal error: attempted to BSATN-serialize query_builder::RawQuery. "
                   "RawQuery is only valid as a view return type and should not be serialized directly.\n",
                   stderr);
        std::abort();
    }

    static ::SpacetimeDB::query_builder::RawQuery<TRow> deserialize(Reader&) {
        std::fputs("SpacetimeDB bindings-cpp internal error: attempted to BSATN-deserialize query_builder::RawQuery. "
                   "RawQuery should only appear in query-view metadata handling.\n",
                   stderr);
        std::abort();
    }

    static AlgebraicType algebraic_type() {
        return algebraic_type_of<::SpacetimeDB::query_builder::RawQuery<TRow>>::get();
    }
};
}
#endif
