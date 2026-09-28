#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(handler_wrong_return_type, HandlerContext ctx, HttpRequest request) {
    return 7u;
}
