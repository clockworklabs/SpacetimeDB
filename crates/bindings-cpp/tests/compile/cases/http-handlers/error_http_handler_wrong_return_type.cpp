#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_wrong_return_type, HandlerContext ctx, HttpRequest request) {
    (void)ctx;
    (void)request;
    return 7u;
}
