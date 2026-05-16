#pragma once

#include "spacetimedb/bsatn/traits.h"
#include "spacetimedb/query_builder/expr.h"
#include <cstdio>
#include <concepts>
#include <string>
#include <type_traits>
#include <utility>

#ifndef SPACETIMEDB_QUERY_BUILDER_ENABLE_BSATN
#define SPACETIMEDB_QUERY_BUILDER_ENABLE_BSATN 1
#endif

namespace SpacetimeDB::query_builder {

template<typename T>
struct query_row_type;

template<typename TRow, typename TCols, typename TIxCols>
class Table;

template<typename TRow, typename TCols, typename TIxCols>
class FromWhere;

template<typename T>
struct HasCols;

template<typename T>
struct HasIxCols;

template<typename T>
struct CanBeLookupTable : std::false_type {};

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

template<typename T>
concept QueryLike = requires(const T& query) {
    { query.into_sql() } -> std::convertible_to<std::string>;
};

template<typename T>
concept QueryBuilderReturn = requires {
    typename query_row_type_t<T>;
} && QueryLike<std::remove_cvref_t<T>>;

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
inline constexpr bool can_be_lookup_row_v =
    CanBeLookupTable<TRow>::value || decltype(detail::adl_lookup_table_allowed<TRow>(0))::value;

template<typename T>
inline constexpr bool can_be_lookup_table_v = CanBeLookupTable<std::remove_cvref_t<T>>::value;

template<typename TRow, typename TCols, typename TIxCols>
struct CanBeLookupTable<Table<TRow, TCols, TIxCols>> : std::bool_constant<can_be_lookup_row_v<TRow>> {};

template<typename TRow, typename TCols, typename TIxCols>
class Table {
public:
    using row_type = TRow;
    using cols_type = TCols;
    using ix_cols_type = TIxCols;

    constexpr Table(const char* table_name, TCols cols, TIxCols ix_cols)
        : table_name_(table_name), cols_(std::move(cols)), ix_cols_(std::move(ix_cols)) {}

    [[nodiscard]] constexpr const char* name() const { return table_name_; }
    [[nodiscard]] constexpr const TCols& cols() const { return cols_; }
    [[nodiscard]] constexpr const TIxCols& ix_cols() const { return ix_cols_; }

    [[nodiscard]] RawQuery<TRow> build() const {
        std::string sql;
        sql.reserve(16 + std::char_traits<char>::length(table_name_));
        sql += "SELECT * FROM \"";
        sql += table_name_;
        sql += "\"";
        return RawQuery<TRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const { return build().into_sql(); }

    // `where` is the ergonomic entry point: it dispatches to `where_col` or
    // `where_ix` based on the predicate signature.
    template<typename TFn>
    [[nodiscard]] auto where(TFn&& predicate) const {
        if constexpr (std::is_invocable_v<TFn, const TCols&, const TIxCols&>) {
            return where_ix(std::forward<TFn>(predicate));
        } else {
            return where_col(std::forward<TFn>(predicate));
        }
    }

    template<typename TFn>
    [[nodiscard]] auto Where(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] auto filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] auto Filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto left_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const;

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto LeftSemijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
        return left_semijoin(right, std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto right_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const;

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto RightSemijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
        return right_semijoin(right, std::forward<TFn>(predicate));
    }

private:
    template<typename TFn>
    [[nodiscard]] auto where_col(TFn&& predicate) const {
        auto expr = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(cols_));
        return FromWhere<TRow, TCols, TIxCols>(*this, std::move(expr));
    }

    template<typename TFn>
    [[nodiscard]] auto where_ix(TFn&& predicate) const {
        auto expr = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(cols_, ix_cols_));
        return FromWhere<TRow, TCols, TIxCols>(*this, std::move(expr));
    }

    const char* table_name_;
    TCols cols_;
    TIxCols ix_cols_;
};

template<typename TRow, typename TCols, typename TIxCols>
class FromWhere {
public:
    using row_type = TRow;
    using cols_type = TCols;
    using ix_cols_type = TIxCols;

    constexpr FromWhere(Table<TRow, TCols, TIxCols> table, BoolExpr<TRow> expr)
        : table_(std::move(table)), expr_(std::move(expr)) {}

    [[nodiscard]] constexpr const char* table_name() const { return table_.name(); }
    [[nodiscard]] constexpr const Table<TRow, TCols, TIxCols>& table() const { return table_; }
    [[nodiscard]] const BoolExpr<TRow>& expr() const { return expr_; }

    [[nodiscard]] RawQuery<TRow> build() const {
        std::string predicate = expr_.format();
        std::string sql;
        sql.reserve(24 + std::char_traits<char>::length(table_.name()) + predicate.size());
        sql += "SELECT * FROM \"";
        sql += table_.name();
        sql += "\" WHERE ";
        sql += predicate;
        return RawQuery<TRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const { return build().into_sql(); }

    // `where` is the ergonomic entry point: it dispatches to `where_col` or
    // `where_ix` based on the predicate signature.
    template<typename TFn>
    [[nodiscard]] FromWhere where(TFn&& predicate) const {
        if constexpr (std::is_invocable_v<TFn, const TCols&, const TIxCols&>) {
            return where_ix(std::forward<TFn>(predicate));
        } else {
            return where_col(std::forward<TFn>(predicate));
        }
    }

    template<typename TFn>
    [[nodiscard]] FromWhere Where(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] FromWhere filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] FromWhere Filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto left_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const;

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto LeftSemijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
        return left_semijoin(right, std::forward<TFn>(predicate));
    }

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto right_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const;

    template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
    [[nodiscard]] auto RightSemijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
        return right_semijoin(right, std::forward<TFn>(predicate));
    }

private:
    template<typename TFn>
    [[nodiscard]] FromWhere where_col(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(table_.cols()));
        return FromWhere(table_, expr_.and_(extra));
    }

    template<typename TFn>
    [[nodiscard]] FromWhere where_ix(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(table_.cols(), table_.ix_cols()));
        return FromWhere(table_, expr_.and_(extra));
    }

    Table<TRow, TCols, TIxCols> table_;
    BoolExpr<TRow> expr_;
};

template<typename TRow>
struct query_row_type<RawQuery<TRow>> {
    using type = TRow;
};

template<typename TRow, typename TCols, typename TIxCols>
struct query_row_type<Table<TRow, TCols, TIxCols>> {
    using type = TRow;
};

template<typename TRow, typename TCols, typename TIxCols>
struct query_row_type<FromWhere<TRow, TCols, TIxCols>> {
    using type = TRow;
};

} // namespace SpacetimeDB::query_builder

#if SPACETIMEDB_QUERY_BUILDER_ENABLE_BSATN
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

} // namespace SpacetimeDB::bsatn
#endif
