#include "spacetimedb.h"

using namespace SpacetimeDB;

namespace {

HttpResponse text_response(const std::string& body) {
    return HttpResponse{200, HttpVersion::Http11, {}, HttpBody::from_string(body)};
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(empty_root, HandlerContext ctx, HttpRequest request) {
    return text_response("empty");
}

SPACETIMEDB_HTTP_HANDLER(slash_root, HandlerContext ctx, HttpRequest request) {
    return text_response("slash");
}

SPACETIMEDB_HTTP_HANDLER(foo, HandlerContext ctx, HttpRequest request) {
    return text_response("foo");
}

SPACETIMEDB_HTTP_HANDLER(foo_slash, HandlerContext ctx, HttpRequest request) {
    return text_response("foo-slash");
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("", empty_root)
        .get("/", slash_root)
        .get("/foo", foo)
        .get("/foo/", foo_slash);
}
