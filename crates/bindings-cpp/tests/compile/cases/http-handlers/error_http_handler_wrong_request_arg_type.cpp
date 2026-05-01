#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_wrong_request_arg_type, HandlerContext ctx, uint32_t request) {
    (void)ctx;
    (void)request;
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string("ok"),
    };
}
