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

template<typename T>
struct is_ix_col : std::false_type {};

template<typename T>
struct is_ix_join_eq : std::false_type {};

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

    template<typename TOtherRow>
    [[nodiscard]] auto Eq(const IxCol<TOtherRow, TValue>& rhs) const {
        return eq(rhs);
    }

    // Keep mismatched indexed-column comparisons on a dedicated overload so they
    // fail here with a clear diagnostic instead of falling through to BoolExpr.
    template<typename TOtherRow, typename TOtherValue>
    [[nodiscard]] auto eq(const IxCol<TOtherRow, TOtherValue>&) const {
        static_assert(std::is_same_v<TValue, TOtherValue>, "Semijoin indexed equality requires both sides to have the same value type.");
        return IxJoinEq<TRow, TOtherRow, TValue>{};
    }

    template<typename TOtherRow, typename TOtherValue>
    [[nodiscard]] auto Eq(const IxCol<TOtherRow, TOtherValue>& rhs) const {
        return eq(rhs);
    }

    template<typename TRhs>
        requires(!is_ix_col<std::remove_cvref_t<TRhs>>::value)
    [[nodiscard]] BoolExpr<TRow> eq(const TRhs& rhs) const {
        return compare(BoolExpr<TRow>::Kind::Eq, rhs);
    }

    template<typename TRhs>
        requires(!is_ix_col<std::remove_cvref_t<TRhs>>::value)
    [[nodiscard]] BoolExpr<TRow> Eq(const TRhs& rhs) const { return eq(rhs); }

    [[nodiscard]] constexpr const ColumnRef<TRow>& column_ref() const { return column_; }

private:
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> compare(typename BoolExpr<TRow>::Kind kind, const TRhs& rhs) const {
        return BoolExpr<TRow>::compare(kind, detail::Operand<TRow>::column(column_), detail::to_operand<TRow>(rhs));
    }

    ColumnRef<TRow> column_;

    template<typename, typename>
    friend class IxCol;
};

template<typename TRow, typename TValue>
struct is_ix_col<IxCol<TRow, TValue>> : std::true_type {};

namespace detail {

template<typename, typename TRow, auto MemberPtr>
inline constexpr bool delayed_is_indexed_member_v = is_indexed_member_v<TRow, MemberPtr>;

} // namespace detail

template<typename TLeftRow, typename TRightRow, typename TValue>
struct IxJoinEq {
    ColumnRef<TLeftRow> lhs;
    ColumnRef<TRightRow> rhs;
};

template<typename TLeftRow, typename TRightRow, typename TValue>
struct is_ix_join_eq<IxJoinEq<TLeftRow, TRightRow, TValue>> : std::true_type {};

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols>
class LeftSemiJoin {
public:
    using row_type = TLeftRow;

    LeftSemiJoin(
        Table<TLeftRow, TLeftCols, TLeftIxCols> left,
        Table<TRightRow, TRightCols, TRightIxCols> right,
        ColumnRef<TLeftRow> left_join_ref,
        ColumnRef<TRightRow> right_join_ref,
        std::optional<BoolExpr<TLeftRow>> where_expr = std::nullopt)
        : left_(std::move(left))
        , right_(std::move(right))
        , left_join_ref_(left_join_ref)
        , right_join_ref_(right_join_ref)
        , where_expr_(std::move(where_expr)) {}

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin where_col(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TLeftRow>(std::forward<TFn>(predicate)(left_.cols()));
        return LeftSemiJoin(left_, right_, left_join_ref_, right_join_ref_, where_expr_ ? where_expr_->and_(extra) : std::optional<BoolExpr<TLeftRow>>(std::move(extra)));
    }

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin where_ix(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TLeftRow>(std::forward<TFn>(predicate)(left_.cols(), left_.ix_cols()));
        return LeftSemiJoin(left_, right_, left_join_ref_, right_join_ref_, where_expr_ ? where_expr_->and_(extra) : std::optional<BoolExpr<TLeftRow>>(std::move(extra)));
    }

    // `where` is the ergonomic entry point: it dispatches to `where_col` or
    // `where_ix` based on the predicate signature.
    template<typename TFn>
    [[nodiscard]] LeftSemiJoin where(TFn&& predicate) const {
        if constexpr (std::is_invocable_v<TFn, const TLeftCols&, const TLeftIxCols&>) {
            return where_ix(std::forward<TFn>(predicate));
        } else {
            return where_col(std::forward<TFn>(predicate));
        }
    }

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin Where(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] LeftSemiJoin Filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    [[nodiscard]] RawQuery<TLeftRow> build() const {
        std::string sql;
        sql.reserve(
            48 +
            (std::char_traits<char>::length(left_.name()) * 3) +
            std::char_traits<char>::length(right_.name()) * 2 +
            std::char_traits<char>::length(left_join_ref_.column_name()) +
            std::char_traits<char>::length(right_join_ref_.column_name()));
        sql += "SELECT \"";
        sql += left_.name();
        sql += "\".* FROM \"";
        sql += left_.name();
        sql += "\" JOIN \"";
        sql += right_.name();
        sql += "\" ON ";
        sql += left_join_ref_.format();
        sql += " = ";
        sql += right_join_ref_.format();
        if (where_expr_) {
            sql += " WHERE " + where_expr_->format();
        }
        return RawQuery<TLeftRow>(std::move(sql));
    }

    [[nodiscard]] std::string into_sql() const { return build().into_sql(); }

private:
    Table<TLeftRow, TLeftCols, TLeftIxCols> left_;
    Table<TRightRow, TRightCols, TRightIxCols> right_;
    ColumnRef<TLeftRow> left_join_ref_;
    ColumnRef<TRightRow> right_join_ref_;
    std::optional<BoolExpr<TLeftRow>> where_expr_;
};

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols>
class RightSemiJoin {
public:
    using row_type = TRightRow;

    RightSemiJoin(
        Table<TLeftRow, TLeftCols, TLeftIxCols> left,
        Table<TRightRow, TRightCols, TRightIxCols> right,
        ColumnRef<TLeftRow> left_join_ref,
        ColumnRef<TRightRow> right_join_ref,
        std::optional<BoolExpr<TLeftRow>> left_where_expr = std::nullopt,
        std::optional<BoolExpr<TRightRow>> right_where_expr = std::nullopt)
        : left_(std::move(left))
        , right_(std::move(right))
        , left_join_ref_(left_join_ref)
        , right_join_ref_(right_join_ref)
        , left_where_expr_(std::move(left_where_expr))
        , right_where_expr_(std::move(right_where_expr)) {}

    template<typename TFn>
    [[nodiscard]] RightSemiJoin where_col(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRightRow>(std::forward<TFn>(predicate)(right_.cols()));
        return RightSemiJoin(left_, right_, left_join_ref_, right_join_ref_, left_where_expr_, right_where_expr_ ? right_where_expr_->and_(extra) : std::optional<BoolExpr<TRightRow>>(std::move(extra)));
    }

    template<typename TFn>
    [[nodiscard]] RightSemiJoin where_ix(TFn&& predicate) const {
        auto extra = detail::make_bool_expr<TRightRow>(std::forward<TFn>(predicate)(right_.cols(), right_.ix_cols()));
        return RightSemiJoin(left_, right_, left_join_ref_, right_join_ref_, left_where_expr_, right_where_expr_ ? right_where_expr_->and_(extra) : std::optional<BoolExpr<TRightRow>>(std::move(extra)));
    }

    // `where` is the ergonomic entry point: it dispatches to `where_col` or
    // `where_ix` based on the predicate signature.
    template<typename TFn>
    [[nodiscard]] RightSemiJoin where(TFn&& predicate) const {
        if constexpr (std::is_invocable_v<TFn, const TRightCols&, const TRightIxCols&>) {
            return where_ix(std::forward<TFn>(predicate));
        } else {
            return where_col(std::forward<TFn>(predicate));
        }
    }

    template<typename TFn>
    [[nodiscard]] RightSemiJoin Where(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] RightSemiJoin filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    template<typename TFn>
    [[nodiscard]] RightSemiJoin Filter(TFn&& predicate) const {
        return where(std::forward<TFn>(predicate));
    }

    [[nodiscard]] RawQuery<TRightRow> build() const {
        std::string sql;
        sql.reserve(
            48 +
            (std::char_traits<char>::length(left_.name()) * 2) +
            (std::char_traits<char>::length(right_.name()) * 3) +
            std::char_traits<char>::length(left_join_ref_.column_name()) +
            std::char_traits<char>::length(right_join_ref_.column_name()));
        sql += "SELECT \"";
        sql += right_.name();
        sql += "\".* FROM \"";
        sql += left_.name();
        sql += "\" JOIN \"";
        sql += right_.name();
        sql += "\" ON ";
        sql += left_join_ref_.format();
        sql += " = ";
        sql += right_join_ref_.format();

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
    Table<TLeftRow, TLeftCols, TLeftIxCols> left_;
    Table<TRightRow, TRightCols, TRightIxCols> right_;
    ColumnRef<TLeftRow> left_join_ref_;
    ColumnRef<TRightRow> right_join_ref_;
    std::optional<BoolExpr<TLeftRow>> left_where_expr_;
    std::optional<BoolExpr<TRightRow>> right_where_expr_;
};

namespace detail {

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto left_semijoin_impl(const Table<TLeftRow, TLeftCols, TLeftIxCols>& left, const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<Table<TRightRow, TRightCols, TRightIxCols>>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    const auto join = std::forward<TFn>(predicate)(left.ix_cols(), right.ix_cols());
    using TJoin = std::remove_cvref_t<decltype(join)>;
    if constexpr (is_ix_join_eq<TJoin>::value) {
        return LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left, right, join.lhs, join.rhs);
    } else {
        static_assert(is_ix_join_eq<TJoin>::value, "Semijoin predicate must compare two indexed columns with eq().");
        return LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left, right, {}, {});
    }
}

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto left_semijoin_impl(const FromWhere<TLeftRow, TLeftCols, TLeftIxCols>& left, const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<Table<TRightRow, TRightCols, TRightIxCols>>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    const auto join = std::forward<TFn>(predicate)(left.table().ix_cols(), right.ix_cols());
    using TJoin = std::remove_cvref_t<decltype(join)>;
    if constexpr (is_ix_join_eq<TJoin>::value) {
        return LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left.table(), right, join.lhs, join.rhs, left.expr());
    } else {
        static_assert(is_ix_join_eq<TJoin>::value, "Semijoin predicate must compare two indexed columns with eq().");
        return LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left.table(), right, {}, {}, left.expr());
    }
}

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto right_semijoin_impl(const Table<TLeftRow, TLeftCols, TLeftIxCols>& left, const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<Table<TRightRow, TRightCols, TRightIxCols>>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    const auto join = std::forward<TFn>(predicate)(left.ix_cols(), right.ix_cols());
    using TJoin = std::remove_cvref_t<decltype(join)>;
    if constexpr (is_ix_join_eq<TJoin>::value) {
        return RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left, right, join.lhs, join.rhs);
    } else {
        static_assert(is_ix_join_eq<TJoin>::value, "Semijoin predicate must compare two indexed columns with eq().");
        return RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left, right, {}, {});
    }
}

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto right_semijoin_impl(const FromWhere<TLeftRow, TLeftCols, TLeftIxCols>& left, const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) {
    static_assert(can_be_lookup_table_v<Table<TRightRow, TRightCols, TRightIxCols>>, "Lookup side of a semijoin must opt in via CanBeLookupTable.");
    const auto join = std::forward<TFn>(predicate)(left.table().ix_cols(), right.ix_cols());
    using TJoin = std::remove_cvref_t<decltype(join)>;
    if constexpr (is_ix_join_eq<TJoin>::value) {
        return RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left.table(), right, join.lhs, join.rhs, left.expr());
    } else {
        static_assert(is_ix_join_eq<TJoin>::value, "Semijoin predicate must compare two indexed columns with eq().");
        return RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>(left.table(), right, {}, {}, left.expr());
    }
}

} // namespace detail

template<typename TRow, typename TCols, typename TIxCols>
template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto Table<TRow, TCols, TIxCols>::left_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
    return detail::left_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow, typename TCols, typename TIxCols>
template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto Table<TRow, TCols, TIxCols>::right_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
    return detail::right_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow, typename TCols, typename TIxCols>
template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto FromWhere<TRow, TCols, TIxCols>::left_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
    return detail::left_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TRow, typename TCols, typename TIxCols>
template<typename TRightRow, typename TRightCols, typename TRightIxCols, typename TFn>
[[nodiscard]] auto FromWhere<TRow, TCols, TIxCols>::right_semijoin(const Table<TRightRow, TRightCols, TRightIxCols>& right, TFn&& predicate) const {
    return detail::right_semijoin_impl(*this, right, std::forward<TFn>(predicate));
}

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols>
struct query_row_type<LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>> {
    using type = TLeftRow;
};

template<typename TLeftRow, typename TLeftCols, typename TLeftIxCols, typename TRightRow, typename TRightCols, typename TRightIxCols>
struct query_row_type<RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>> {
    using type = TRightRow;
};

} // namespace SpacetimeDB::query_builder
