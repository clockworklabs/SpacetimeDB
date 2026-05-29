#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_no_request_arg, HandlerContext ctx) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string("ok"),
    };
}
