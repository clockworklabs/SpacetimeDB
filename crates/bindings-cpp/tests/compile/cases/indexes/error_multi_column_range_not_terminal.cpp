#include "spacetimedb.h"

#include <cstdint>
#include <tuple>

using namespace SpacetimeDB;

struct InvalidRangeRow {
    uint32_t a;
    uint32_t b;
    uint32_t c;
};

SPACETIMEDB_STRUCT(InvalidRangeRow, a, b, c)
SPACETIMEDB_TABLE(InvalidRangeRow, invalid_range_row, Public)
FIELD_NamedMultiColumnIndex(invalid_range_row, by_all, a, b, c)

SPACETIMEDB_REDUCER(check_invalid_middle_range, ReducerContext ctx) {
    auto rows = ctx.db[invalid_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), range_from(uint32_t(2)), uint32_t(3)));
    (void)rows;
    return Ok();
}
