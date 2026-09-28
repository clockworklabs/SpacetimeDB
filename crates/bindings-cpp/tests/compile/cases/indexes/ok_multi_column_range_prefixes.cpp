#include "spacetimedb.h"

#include <cstdint>
#include <tuple>

using namespace SpacetimeDB;

struct MultiRangeRow {
    uint32_t a;
    uint32_t b;
    uint32_t c;
    uint32_t d;
    uint32_t e;
    uint32_t f;
};

SPACETIMEDB_STRUCT(MultiRangeRow, a, b, c, d, e, f)
SPACETIMEDB_TABLE(MultiRangeRow, multi_range_row, Public)
FIELD_NamedMultiColumnIndex(multi_range_row, by_all, a, b, c, d, e, f)

SPACETIMEDB_REDUCER(check_multi_column_range_prefixes, ReducerContext ctx) {
    auto q1 = ctx.db[multi_range_row_by_all].filter(range_from(uint32_t(1)));
    auto q2 = ctx.db[multi_range_row_by_all].filter(uint32_t(1));

    auto q3 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), range(uint32_t(2), uint32_t(3))));
    auto q4 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2)));

    auto q5 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), range_inclusive(uint32_t(3), uint32_t(4))));
    auto q6 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3)));

    auto q7 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), range_to(uint32_t(4))));
    auto q8 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), uint32_t(4)));

    auto q9 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), uint32_t(4), range_to_inclusive(uint32_t(5))));
    auto q10 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), uint32_t(4), uint32_t(5)));

    auto q11 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), uint32_t(4), uint32_t(5), range_full<uint32_t>()));
    auto q12 = ctx.db[multi_range_row_by_all].filter(
        std::make_tuple(uint32_t(1), uint32_t(2), uint32_t(3), uint32_t(4), uint32_t(5), uint32_t(6)));

    (void)q1;
    (void)q2;
    (void)q3;
    (void)q4;
    (void)q5;
    (void)q6;
    (void)q7;
    (void)q8;
    (void)q9;
    (void)q10;
    (void)q11;
    (void)q12;

    return Ok();
}
