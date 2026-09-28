#include "spacetimedb.h"

using namespace SpacetimeDB;

struct TestRow {
    uint32_t value;
};
SPACETIMEDB_STRUCT(TestRow, value)
SPACETIMEDB_TABLE(TestRow, test_row, Public)

SPACETIMEDB_HTTP_HANDLER(handler_no_db, HandlerContext ctx, HttpRequest request) {
    auto count = ctx.db[test_row].count();
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(std::to_string(count)),
    };
}
