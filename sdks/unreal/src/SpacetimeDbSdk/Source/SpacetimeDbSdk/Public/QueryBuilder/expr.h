#pragma once

#include "CoreMinimal.h"
#include "BSATN/Core/timestamp.h"
#include "BSATN/Core/types.h"
#include "Types/Builtins.h"

#include <cstdint>
#include <iomanip>
#include <locale>
#include <memory>
#include <limits>
#include <sstream>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <variant>
#include <vector>

namespace SpacetimeDB::query_builder {

template<typename TRow, typename TValue>
class Col;

template<typename TRow>
class ColumnRef {
public:
    constexpr ColumnRef()
        : table_name_(""), column_name_("") {}

    constexpr ColumnRef(const char* table_name, const char* column_name)
        : table_name_(table_name), column_name_(column_name) {}

    [[nodiscard]] std::string format() const {
        return "\"" + std::string(table_name_) + "\".\"" + std::string(column_name_) + "\"";
    }

    [[nodiscard]] constexpr const char* table_name() const { return table_name_; }
    [[nodiscard]] constexpr const char* column_name() const { return column_name_; }

private:
    const char* table_name_;
    const char* column_name_;
};

namespace detail {

inline std::string quote_string(std::string_view value) {
    std::string escaped;
    escaped.reserve(value.size() + 2);
    escaped.push_back('\'');
    for (char ch : value) {
        escaped.push_back(ch);
        if (ch == '\'') {
            escaped.push_back('\'');
        }
    }
    escaped.push_back('\'');
    return escaped;
}

inline std::string trim_timestamp_fraction(std::string value) {
    // Keep this in sync with the current Timestamp::to_string() UTC form.
    // If that representation changes away from a +00:00 / Z suffix, revisit this trimming logic.
    const std::size_t plus = value.rfind("+00:00");
    const std::size_t z = value.rfind('Z');
    const std::size_t dot = value.find('.');
    const std::size_t suffix = plus != std::string::npos ? plus : z;
    if (suffix == std::string::npos || dot == std::string::npos || dot > suffix) {
        return value;
    }

    std::size_t trim = suffix;
    while (trim > dot + 1 && value[trim - 1] == '0') {
        --trim;
    }
    if (trim == dot + 1) {
        value.erase(dot, suffix - dot);
    } else {
        value.erase(trim, suffix - trim);
    }
    return value;
}

inline std::string literal_sql(const std::string& value) { return quote_string(value); }
inline std::string literal_sql(std::string_view value) { return quote_string(value); }
inline std::string literal_sql(const char* value) { return quote_string(value == nullptr ? "" : value); }
inline std::string literal_sql(const TCHAR* value) {
    return quote_string(value == nullptr ? "" : TCHAR_TO_UTF8(value));
}
inline std::string literal_sql(const FString& value) {
    return quote_string(TCHAR_TO_UTF8(*value));
}
inline std::string literal_sql(bool value) { return value ? "TRUE" : "FALSE"; }
inline std::string literal_sql(const ::SpacetimeDb::Identity& value) { return "0x" + value.to_hex_string(); }
inline std::string literal_sql(const ::SpacetimeDb::ConnectionId& value) { return "0x" + value.to_string(); }
inline std::string literal_sql(const ::SpacetimeDb::Timestamp& value) { return quote_string(trim_timestamp_fraction(value.to_string())); }
inline std::string ensure_hex_prefix(std::string value) {
    if (value.rfind("0x", 0) == 0 || value.rfind("0X", 0) == 0) {
        return value;
    }
    return "0x" + value;
}
inline std::string literal_sql(const FSpacetimeDBIdentity& value) { return ensure_hex_prefix(std::string(TCHAR_TO_UTF8(*value.ToHex()))); }
inline std::string literal_sql(const FSpacetimeDBConnectionId& value) { return ensure_hex_prefix(std::string(TCHAR_TO_UTF8(*value.ToHex()))); }
inline std::string literal_sql(const FSpacetimeDBUuid& value) {
    return quote_string(TCHAR_TO_UTF8(*value.ToString()));
}
inline std::string literal_sql(const FSpacetimeDBTimestamp& value) {
    return quote_string(trim_timestamp_fraction(TCHAR_TO_UTF8(*value.ToString())));
}
std::string literal_sql(const FSpacetimeDBTimeDuration& value) = delete;
inline std::string literal_sql(const std::vector<uint8_t>& value) {
    std::ostringstream out;
    out << "0x" << std::hex << std::setfill('0');
    for (uint8_t byte : value) {
        out << std::setw(2) << static_cast<unsigned>(byte);
    }
    return out.str();
}
inline std::string literal_sql(const TArray<uint8>& value) {
    std::ostringstream out;
    out << "0x" << std::hex << std::setfill('0');
    for (uint8 byte : value) {
        out << std::setw(2) << static_cast<unsigned>(byte);
    }
    return out.str();
}
inline std::string literal_sql(const ::SpacetimeDb::u128& value) { return value.to_string(); }
inline std::string literal_sql(const ::SpacetimeDb::i128& value) { return value.to_string(); }
inline std::string literal_sql(const ::SpacetimeDb::u256& value) { return value.to_string(); }
inline std::string literal_sql(const ::SpacetimeDb::i256& value) { return value.to_string(); }
inline std::string literal_sql(const FSpacetimeDBUInt128& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBInt128& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBUInt256& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBInt256& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }

template<typename TFloat>
inline std::string format_floating_point(TFloat value) {
    std::ostringstream out;
    out.imbue(std::locale::classic());
    out << std::setprecision(std::numeric_limits<TFloat>::max_digits10);
    out << value;
    return out.str();
}

inline std::string literal_sql(float value) {
    return format_floating_point(value);
}

inline std::string literal_sql(double value) {
    return format_floating_point(value);
}

template<typename TValue>
std::string literal_sql(const TValue& value)
requires(std::is_integral_v<TValue> && !std::is_same_v<std::remove_cv_t<TValue>, bool>)
{
    return std::to_string(value);
}

template<typename TRow>
class Operand {
public:
    static Operand column(ColumnRef<TRow> column) { return Operand(std::move(column)); }
    static Operand literal(std::string sql) { return Operand(std::move(sql)); }

    [[nodiscard]] std::string format() const {
        return std::holds_alternative<ColumnRef<TRow>>(value_)
            ? std::get<ColumnRef<TRow>>(value_).format()
            : std::get<std::string>(value_);
    }

private:
    explicit Operand(ColumnRef<TRow> column) : value_(std::move(column)) {}
    explicit Operand(std::string sql) : value_(std::move(sql)) {}

    std::variant<ColumnRef<TRow>, std::string> value_;
};

template<typename TRow, typename TValue>
Operand<TRow> to_operand(const Col<TRow, TValue>& column);

template<typename TRow, typename TValue>
Operand<TRow> to_operand(const TValue& value) {
    return Operand<TRow>::literal(literal_sql(value));
}

} // namespace detail

template<typename TRow>
class BoolExpr {
public:
    enum class Kind {
        Eq,
        Ne,
        Gt,
        Lt,
        Gte,
        Lte,
        And,
        Or,
        Not,
    };

    static BoolExpr compare(Kind kind, detail::Operand<TRow> lhs, detail::Operand<TRow> rhs) {
        return BoolExpr(std::make_shared<Node>(kind, std::move(lhs), std::move(rhs)));
    }

    static BoolExpr always(bool value) {
        return compare(
            Kind::Eq,
            detail::Operand<TRow>::literal(value ? "TRUE" : "FALSE"),
            detail::Operand<TRow>::literal("TRUE"));
    }

    [[nodiscard]] std::string format() const {
        return format_node(root_);
    }

    [[nodiscard]] BoolExpr and_(const BoolExpr& other) const {
        return BoolExpr(std::make_shared<Node>(Kind::And, root_, other.root_));
    }
    [[nodiscard]] BoolExpr And(const BoolExpr& other) const { return and_(other); }

    [[nodiscard]] BoolExpr or_(const BoolExpr& other) const {
        return BoolExpr(std::make_shared<Node>(Kind::Or, root_, other.root_));
    }
    [[nodiscard]] BoolExpr Or(const BoolExpr& other) const { return or_(other); }

    [[nodiscard]] BoolExpr not_() const {
        return BoolExpr(std::make_shared<Node>(Kind::Not, root_, nullptr));
    }
    [[nodiscard]] BoolExpr Not() const { return not_(); }

private:
    struct Node;

    struct CompareData {
        detail::Operand<TRow> lhs;
        detail::Operand<TRow> rhs;
    };

    struct LogicData {
        std::shared_ptr<const Node> left;
        std::shared_ptr<const Node> right;
    };

    struct NotData {
        std::shared_ptr<const Node> child;
    };

    struct Node {
        Node(Kind kind_in, detail::Operand<TRow> lhs_in, detail::Operand<TRow> rhs_in)
            : kind(kind_in), data(CompareData{std::move(lhs_in), std::move(rhs_in)}) {}

        Node(Kind kind_in, std::shared_ptr<const Node> left_in, std::shared_ptr<const Node> right_in)
            : kind(kind_in),
              data(kind_in == Kind::Not
                       ? NodeData(NotData{std::move(left_in)})
                       : NodeData(LogicData{std::move(left_in), std::move(right_in)})) {}

        Kind kind;
        using NodeData = std::variant<CompareData, LogicData, NotData>;
        NodeData data;
    };

    explicit BoolExpr(std::shared_ptr<const Node> root)
        : root_(std::move(root)) {}

    static std::string format_node(const std::shared_ptr<const Node>& node) {
        switch (node->kind) {
            case Kind::Eq: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " = " + compare.rhs.format() + ")";
            }
            case Kind::Ne: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " <> " + compare.rhs.format() + ")";
            }
            case Kind::Gt: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " > " + compare.rhs.format() + ")";
            }
            case Kind::Lt: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " < " + compare.rhs.format() + ")";
            }
            case Kind::Gte: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " >= " + compare.rhs.format() + ")";
            }
            case Kind::Lte: {
                const auto& compare = std::get<CompareData>(node->data);
                return "(" + compare.lhs.format() + " <= " + compare.rhs.format() + ")";
            }
            case Kind::And: {
                const auto& logic = std::get<LogicData>(node->data);
                return "(" + format_node(logic.left) + " AND " + format_node(logic.right) + ")";
            }
            case Kind::Or: {
                const auto& logic = std::get<LogicData>(node->data);
                return "(" + format_node(logic.left) + " OR " + format_node(logic.right) + ")";
            }
            case Kind::Not: {
                const auto& not_data = std::get<NotData>(node->data);
                return "(NOT " + format_node(not_data.child) + ")";
            }
        }
        return {};
    }

    std::shared_ptr<const Node> root_;
};

namespace detail {

template<typename TRow>
BoolExpr<TRow> make_bool_expr(BoolExpr<TRow> expr) {
    return expr;
}

template<typename TRow>
BoolExpr<TRow> make_bool_expr(bool value) {
    return BoolExpr<TRow>::always(value);
}

template<typename TRow>
BoolExpr<TRow> make_bool_expr(const Col<TRow, bool>& column) {
    return column.eq(true);
}

} // namespace detail

template<typename TRow, typename TValue>
class Col {
public:
    constexpr Col() = default;

    constexpr Col(const char* table_name, const char* column_name)
        : column_(table_name, column_name) {}

    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> eq(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Eq, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Eq(const TRhs& rhs) const { return eq(rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> ne(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Ne, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Neq(const TRhs& rhs) const { return ne(rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> gt(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Gt, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Gt(const TRhs& rhs) const { return gt(rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> lt(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Lt, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Lt(const TRhs& rhs) const { return lt(rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> gte(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Gte, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Gte(const TRhs& rhs) const { return gte(rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> lte(const TRhs& rhs) const { return compare(BoolExpr<TRow>::Kind::Lte, rhs); }
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> Lte(const TRhs& rhs) const { return lte(rhs); }

    [[nodiscard]] constexpr const ColumnRef<TRow>& column_ref() const { return column_; }

private:
    template<typename TRhs>
    [[nodiscard]] BoolExpr<TRow> compare(typename BoolExpr<TRow>::Kind kind, const TRhs& rhs) const {
        return BoolExpr<TRow>::compare(kind, detail::to_operand<TRow>(*this), detail::to_operand<TRow>(rhs));
    }

    ColumnRef<TRow> column_;
};

namespace detail {

template<typename TRow, typename TValue>
Operand<TRow> to_operand(const Col<TRow, TValue>& column) {
    return Operand<TRow>::column(column.column_ref());
}

} // namespace detail

} // namespace SpacetimeDB::query_builder
