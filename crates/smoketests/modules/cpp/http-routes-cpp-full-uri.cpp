#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(echo_uri, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(request.uri),
    };
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router().get("/echo-uri", echo_uri);
}
