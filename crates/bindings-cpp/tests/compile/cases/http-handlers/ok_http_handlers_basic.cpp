#include "spacetimedb.h"

#include <type_traits>

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(hello_handler, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string("ok"),
    };
}

SPACETIMEDB_HTTP_ROUTER(register_http_routes) {
    Router nested = Router()
        .get("/nested", hello_handler);

    Router merged = Router()
        .get("", hello_handler)
        .head("/health", hello_handler);

    return Router()
        .get("/hello", hello_handler)
        .delete_("/delete", hello_handler)
        .any("/", hello_handler)
        .merge(merged)
        .nest("/api", nested);
}

// Each deprecated name must stay an alias of the type that replaced it.
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
static_assert(std::is_same_v<AuthCtx, AuthContext>);
#pragma GCC diagnostic pop
