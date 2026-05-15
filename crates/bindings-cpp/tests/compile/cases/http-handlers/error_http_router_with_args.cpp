#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(hello_handler, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string("ok"),
    };
}

SPACETIMEDB_HTTP_ROUTER(register_http_routes, HandlerContext ctx) {
    return Router().get("/hello", hello_handler);
}
