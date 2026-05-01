#include "spacetimedb.h"

using namespace SpacetimeDB;

#if defined(__clang__)
#pragma clang diagnostic error "-Wreturn-type"
#endif

SPACETIMEDB_HTTP_HANDLER(handler_no_return_type, HandlerContext ctx, HttpRequest request) {
    (void)ctx;
    (void)request;
}
