#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_no_connection_id, HandlerContext ctx, HttpRequest request) {
    auto conn_id = ctx.connection_id();
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(conn_id.to_hex_string()),
    };
}
