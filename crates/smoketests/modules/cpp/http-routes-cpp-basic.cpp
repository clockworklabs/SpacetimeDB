#include "spacetimedb.h"

using namespace SpacetimeDB;

struct Entry {
    uint64_t id;
    std::string value;
};
SPACETIMEDB_STRUCT(Entry, id, value)
SPACETIMEDB_TABLE(Entry, entry, Public)

namespace {

std::string header_value_utf8(const HttpRequest& request, const std::string& header_name) {
    for (const auto& header : request.headers) {
        if (header.name == header_name) {
            return std::string(header.value.begin(), header.value.end());
        }
    }
    return "";
}

HttpResponse text_response(uint16_t status_code, std::string body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        { HttpHeader{"content-type", "text/plain; charset=utf-8"} },
        HttpBody::from_string(body),
    };
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(get_simple, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "ok");
}

SPACETIMEDB_HTTP_HANDLER(post_insert, HandlerContext ctx, HttpRequest request) {
    ctx.with_tx([](TxContext& tx) {
        uint64_t id = tx.db[entry].count();
        tx.db[entry].insert(Entry{ id, "posted" });
    });
    return text_response(200, "inserted");
}

SPACETIMEDB_HTTP_HANDLER(get_count, HandlerContext ctx, HttpRequest request) {
    uint64_t count = ctx.with_tx([](TxContext& tx) -> uint64_t {
        return tx.db[entry].count();
    });
    return text_response(200, std::to_string(count));
}

SPACETIMEDB_HTTP_HANDLER(any_handler, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "any");
}

SPACETIMEDB_HTTP_HANDLER(header_echo, HandlerContext ctx, HttpRequest request) {
    return text_response(200, header_value_utf8(request, "x-echo"));
}

SPACETIMEDB_HTTP_HANDLER(set_response_header, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        { HttpHeader{"x-response", "set"} },
        HttpBody::from_string("header-set"),
    };
}

SPACETIMEDB_HTTP_HANDLER(body_handler, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "non-empty");
}

SPACETIMEDB_HTTP_HANDLER(teapot, HandlerContext ctx, HttpRequest request) {
    return text_response(418, "teapot");
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("/get", get_simple)
        .post("/post", post_insert)
        .get("/count", get_count)
        .any("/any", any_handler)
        .get("/header", header_echo)
        .get("/set-header", set_response_header)
        .get("/body", body_handler)
        .get("/teapot", teapot);
}
