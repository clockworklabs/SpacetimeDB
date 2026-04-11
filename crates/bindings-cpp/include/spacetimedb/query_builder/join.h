#pragma once

#include "spacetimedb/query_builder/table.h"
#include <optional>
#include <string>
#include <vector>

namespace SpacetimeDB::query_builder {

template<typename TLeftRow, typename TRightRow, typename TValue>
struct IxJoinEq;

template<typename TRow, auto MemberPtr>
struct member_tag {};

inline std::false_type indexed_member_lookup(...);

template<typename TRow, auto MemberPtr>
inline constexpr bool is_indexed_member_v = decltype(indexed_member_lookup(member_tag<TRow, MemberPtr>{}))::value;

template<typename TRow, typename TValue>
class IxCol {
public:
    constexpr IxCol() = default;

    constexpr IxCol(const char* table_name, const char* column_name)
        : column_(table_name, column_name) {}

    template<typename TOtherRow>
    [[nodiscard]] auto eq(const IxCol<TOtherRow, TValue>& rhs) const {
        return IxJoinEq<TRow, TOtherRow, TValue>{column_, rhs.column_};
    }

private:
    ColumnRef<TRow> column_;

    template<typename, typename>
    friend class IxCol;
};

namespace detail {

template<typename, typename TRow, auto MemberPtr>
inline constexpr bool delayed_is_indexed_member_v = is_indexed_member_v<TRow, MemberPtr>;

template<typename TRow, typename TValue, auto MemberPtr>
class MaybeIxCol {
public:
    constexpr MaybeIxCol() = default;
    constexpr MaybeIxCol(const char* table_name, const char* column_name)
        : column_(table_name, column_name) {}

    template<typename TOtherRow, auto TOtherMemberPtr>
    [[nodiscard]] auto eq(const MaybeIxCol<TOtherRow, TValue, TOtherMemberPtr>& rhs) const {
        static_assert(
            is_indexed_member_v<TRow, MemberPtr> && is_indexed_member_v<TOtherRow, TOtherMemberPtr>,
            "Semijoin predicates may only use single-column indexed fields.");
        return IxJoinEq<TRow, TOtherRow, TValue>{column_, rhs.column_};
    }

private:
    ColumnRef<TRow> column_{};

    template<typename, typename, auto>
    friend class MaybeIxCol;
};

template<typename TRow, typename TValue, auto MemberPtr>
using ix_col_member_t = MaybeIxCol<TRow, TValue, MemberPtr>;
// HasIxCols currently exposes all fields through MaybeIxCol so table/view macros can stay uniform.
// Non-indexed fields are rejected when .eq() is used in a semijoin predicate.

} // namespace detail

template<typename TLeftRow, typename TRightRow, typename TValue>
struct IxJoinEq {
    ColumnRef<TLeftRow> lhs;
    ColumnRef<TRightRow> rhs;
};

template<typename TRow>
class LeftSemiJoin {
public:
    using row_type = TRow;

    LeftSemiJoin(ColumnRef<TRow> lhs, const char* right_table, const char* right_column, std::optional<BoolExpr<TRow>> where_expr = std::nullopt)
        : lhs_(lhs), right_table_(right_table), right_column_(right_column), where_expr_(std::move(where_expr)) {}

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin where(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRow>(std::forward<TFn>(predicate)(HasCols<TRow>::get(lhs_.table_name())));
        return LeftSemiJoin(lhs_, right_table_, right_column_, where_expr_ ? where_expr_->and_(extra) : std::optional<BoolExpr<TRow>>(std::move(extra)));
    }

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    [[nodiscard]] RawQuery<TRow> build() const {
        std::string sql;
        sql.reserve(
            48 +
            (std::char_traits<char>::length(lhs_.table_name()) * 3) +
            std::char_traits<char>::length(right_table_) * 2 +
            std::char_traits<char>::length(lhs_.column_name()) +
            std::char_traits<char>::length(right_column_));
        sql += "SELECT \"";
        sql += lhs_.table_name();
        sql += "\".* FROM \"";
        sql += lhs_.table_name();
        sql += "\" JOIN \"";
        sql += right_table_;
        sql += "\" ON \"";
        sql += lhs_.table_name();
        sql += "\".\"";
        sql += lhs_.column_name();
        sql += "\" = \"";
        sql += right_table_;
        sql += "\".\"";
        sql += right_column_;
        sql += "\"";
        if (where_expr_) {
            sql += " WHERE " + where_expr_->format();
        }
        return RawQuery<TRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const { return build().into_sql(); }

private:
    ColumnRef<TRow> lhs_;
    const char* right_table_;
    const char* right_column_;
    std::optional<BoolExpr<TRow>> where_expr_;
};

template<typename TRightRow, typename TLeftRow>
class RightSemiJoin {
public:
    using row_type = TRightRow;

    RightSemiJoin(
        ColumnRef<TLeftRow> lhs,
        ColumnRef<TRightRow> rhs,
        std::optional<BoolExpr<TLeftRow>> left_where_expr = std::nullopt,
        std::optional<BoolExpr<TRightRow>> right_where_expr = std::nullopt)
        : lhs_(lhs), rhs_(rhs), left_where_expr_(std::move(left_where_expr)), right_where_expr_(std::move(right_where_expr)) {}

    template<typename TFn>
    [[nodiscard]] RightSemiJoin where(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRightRow>(std::forward<TFn>(predicate)(HasCols<TRightRow>::get(rhs_.table_name())));
        return RightSemiJoin(lhs_, rhs_, left_where_expr_, right_where_expr_ ? right_where_expr_->and_(extra) : std::optional<BoolExpr<TRightRow>>(std::move(extra)));
    }

    template<typename TFn>
    [[nodiscard]] RightSemiJoin filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    [[nodiscard]] RawQuery<TRightRow> build() const {
        std::string sql;
        sql.reserve(
            48 +
            (std::char_traits<char>::length(lhs_.table_name()) * 2) +
            (std::char_traits<char>::length(rhs_.table_name()) * 3) +
            std::char_traits<char>::length(lhs_.column_name()) +
            std::char_traits<char>::length(rhs_.column_name()));
        sql += "SELECT \"";
        sql += rhs_.table_name();
        sql += "\".* FROM \"";
        sql += lhs_.table_name();
        sql += "\" JOIN \"";
        sql += rhs_.table_name();
        sql += "\" ON \"";
        sql += lhs_.table_name();
        sql += "\".\"";
        sql += lhs_.column_name();
        sql += "\" = \"";
        sql += rhs_.table_name();
        sql += "\".\"";
        sql += rhs_.column_name();
        sql += "\"";

        if (left_where_expr_ && right_where_expr_) {
            sql += " WHERE ";
            sql += left_where_expr_->format();
            sql += " AND ";
            sql += right_where_expr_->format();
        } else if (left_where_expr_) {
            sql += " WHERE ";
            sql += left_where_expr_->format();
        } else if (right_where_expr_) {
            sql += " WHERE ";
            sql += right_where_expr_->format();
        }

        return RawQuery<TRightRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const { return build().into_sql(); }

private:
    ColumnRef<TLeftRow> lhs_;
    ColumnRef<TRightRow> rhs_;
    std::optional<BoolExpr<TLeftRow>> left_where_expr_;
    std::optional<BoolExpr<TRightRow>> right_where_expr_;
};

namespace detail {

template<typename TLeftRow, typename TRightRow, typename TFn>
[[nodiscard]] LeftSemiJoin<TLeftRow> left_semijoin_impl(const Table<TLeftRow>& left, const Table<TRightRow>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<TRightRow>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    static_assert(requires { HasIxCols<TLeftRow>::get(left.name()); }, "Left side of a semijoin must provide HasIxCols.");
    static_assert(requires { HasIxCols<TRightRow>::get(right.name()); }, "Lookup side of a semijoin must provide HasIxCols.");
    const auto join = std::forward<TFn>(predicate)(HasIxCols<TLeftRow>::get(left.name()), HasIxCols<TRightRow>::get(right.name()));
    return LeftSemiJoin<TLeftRow>(join.lhs, right.name(), join.rhs.column_name());
}

template<typename TLeftRow, typename TRightRow, typename TFn>
[[nodiscard]] LeftSemiJoin<TLeftRow> left_semijoin_impl(const FromWhere<TLeftRow>& left, const Table<TRightRow>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<TRightRow>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    static_assert(requires { HasIxCols<TLeftRow>::get(left.table_name()); }, "Left side of a semijoin must provide HasIxCols.");
    static_assert(requires { HasIxCols<TRightRow>::get(right.name()); }, "Lookup side of a semijoin must provide HasIxCols.");
    const auto join = std::forward<TFn>(predicate)(HasIxCols<TLeftRow>::get(left.table_name()), HasIxCols<TRightRow>::get(right.name()));
    return LeftSemiJoin<TLeftRow>(join.lhs, right.name(), join.rhs.column_name(), left.expr());
}

template<typename TLeftRow, typename TRightRow, typename TFn>
[[nodiscard]] RightSemiJoin<TRightRow, TLeftRow> right_semijoin_impl(const Table<TLeftRow>& left, const Table<TRightRow>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<TRightRow>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    static_assert(requires { HasIxCols<TLeftRow>::get(left.name()); }, "Left side of a semijoin must provide HasIxCols.");
    static_assert(requires { HasIxCols<TRightRow>::get(right.name()); }, "Lookup side of a semijoin must provide HasIxCols.");
    const auto join = std::forward<TFn>(predicate)(HasIxCols<TLeftRow>::get(left.name()), HasIxCols<TRightRow>::get(right.name()));
    return RightSemiJoin<TRightRow, TLeftRow>(join.lhs, join.rhs);
}

template<typename TLeftRow, typename TRightRow, typename TFn>
[[nodiscard]] RightSemiJoin<TRightRow, TLeftRow> right_semijoin_impl(const FromWhere<TLeftRow>& left, const Table<TRightRow>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<TRightRow>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    static_assert(requires { HasIxCols<TLeftRow>::get(left.table_name()); }, "Left side of a semijoin must provide HasIxCols.");
    static_assert(requires { HasIxCols<TRightRow>::get(right.name()); }, "Lookup side of a semijoin must provide HasIxCols.");
    const auto join = std::forward<TFn>(predicate)(HasIxCols<TLeftRow>::get(left.table_name()), HasIxCols<TRightRow>::get(right.name()));
    return RightSemiJoin<TRightRow, TLeftRow>(join.lhs, join.rhs, left.expr());
}

} // namespace detail

template<typename TRow>
template<typename TRightRow, typename TFn>
[[nodiscard]] LeftSemiJoin<TRow> Table<TRow>::left_semijoin(const Table<TRightRow>& right, TFn&& predicate) const {
    return detail::left_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow>
template<typename TRightRow, typename TFn>
[[nodiscard]] RightSemiJoin<TRightRow, TRow> Table<TRow>::right_semijoin(const Table<TRightRow>& right, TFn&& predicate) const {
    return detail::right_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow>
template<typename TRightRow, typename TFn>
[[nodiscard]] LeftSemiJoin<TRow> FromWhere<TRow>::left_semijoin(const Table<TRightRow>& right, TFn&& predicate) const {
    return detail::left_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow>
template<typename TRightRow, typename TFn>
[[nodiscard]] RightSemiJoin<TRightRow, TRow> FromWhere<TRow>::right_semijoin(const Table<TRightRow>& right, TFn&& predicate) const {
    return detail::right_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow>
struct query_row_type<LeftSemiJoin<TRow>> {
    using type = TRow;
};

template<typename TRightRow, typename TLeftRow>
struct query_row_type<RightSemiJoin<TRightRow, TLeftRow>> {
    using type = TRightRow;
};

} // namespace SpacetimeDB::query_builder
