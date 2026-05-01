#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_immutable_ctx, const HandlerContext& ctx, HttpRequest request) {
    (void)ctx;
    (void)request;
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string("ok"),
    };
}
