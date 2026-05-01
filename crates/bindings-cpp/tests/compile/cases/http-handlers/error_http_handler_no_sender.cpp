#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_no_sender, HandlerContext ctx, HttpRequest request) {
    (void)request;
    auto sender = ctx.sender();
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(sender.to_hex_string()),
    };
}
