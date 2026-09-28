#include "spacetimedb.h"

using namespace SpacetimeDB;

struct Data {
    uint64_t id;
    std::vector<uint8_t> body;
};
SPACETIMEDB_STRUCT(Data, id, body)
SPACETIMEDB_TABLE(Data, data, Public)
FIELD_PrimaryKeyAutoInc(data, id)

namespace {

HttpResponse bytes_response(uint16_t status_code, std::vector<uint8_t> body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        {},
        HttpBody{std::move(body)},
    };
}

HttpResponse text_response(uint16_t status_code, std::string body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(body),
    };
}

std::string query_value(const std::string& uri, const std::string& key) {
    std::string needle = "?" + key + "=";
    size_t pos = uri.find(needle);
    if (pos == std::string::npos) {
        needle = "&" + key + "=";
        pos = uri.find(needle);
    }
    if (pos == std::string::npos) {
        return "";
    }
    pos += needle.size();
    size_t end = uri.find('&', pos);
    return uri.substr(pos, end == std::string::npos ? std::string::npos : end - pos);
}

bool try_parse_u64(const std::string& text, uint64_t& value) {
    if (text.empty()) {
        return false;
    }
    uint64_t result = 0;
    for (char c : text) {
        if (c < '0' || c > '9') {
            return false;
        }
        result = (result * 10) + static_cast<uint64_t>(c - '0');
    }
    value = result;
    return true;
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(insert, HandlerContext ctx, HttpRequest request) {
    std::vector<uint8_t> body = request.body.to_bytes();
    uint64_t id = ctx.with_tx([&](TxContext& tx) -> uint64_t {
        return tx.db[data].insert(Data{0, body}).id;
    });
    return text_response(200, std::to_string(id));
}

SPACETIMEDB_HTTP_HANDLER(retrieve, HandlerContext ctx, HttpRequest request) {
    uint64_t id = 0;
    if (!try_parse_u64(query_value(request.uri, "id"), id)) {
        return text_response(500, "invalid id");
    }

    auto body = ctx.with_tx([&](TxContext& tx) -> std::optional<std::vector<uint8_t>> {
        auto row = tx.db[data_id].find(id);
        if (row.has_value()) {
            return row->body;
        }
        return std::nullopt;
    });

    if (body.has_value()) {
        return bytes_response(200, std::move(body.value()));
    }
    return bytes_response(404, {});
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router().post("/insert", insert).get("/retrieve", retrieve);
}
