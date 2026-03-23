#pragma once

#include "QueryBuilder/expr.h"
#include "QueryBuilder/join.h"
#include "QueryBuilder/table.h"

#include <utility>

namespace SpacetimeDB {

template<typename TRow>
using Query = query_builder::RawQuery<TRow>;

class QueryBuilder {
public:
    template<typename TRow, typename TCols, typename TIxCols>
    [[nodiscard]] constexpr query_builder::Table<TRow, TCols, TIxCols> table(const char* table_name, TCols cols, TIxCols ix_cols) const {
        return query_builder::Table<TRow, TCols, TIxCols>(table_name, std::move(cols), std::move(ix_cols));
    }
};

} // namespace SpacetimeDB
