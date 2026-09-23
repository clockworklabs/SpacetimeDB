#include "spacetimedb.h"
#include <algorithm>

using namespace SpacetimeDB;

namespace {

HttpResponse bytes_response(uint16_t status_code, std::vector<uint8_t> body) {
    return HttpResponse{status_code, HttpVersion::Http11, {}, HttpBody{std::move(body)}};
}

HttpResponse text_response(uint16_t status_code, const std::string& body) {
    return HttpResponse{status_code, HttpVersion::Http11, {}, HttpBody::from_string(body)};
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(reverse_bytes, HandlerContext ctx, HttpRequest request) {
    std::vector<uint8_t> reversed = request.body.to_bytes();
    std::reverse(reversed.begin(), reversed.end());
    return bytes_response(200, std::move(reversed));
}

SPACETIMEDB_HTTP_HANDLER(reverse_words, HandlerContext ctx, HttpRequest request) {
    const std::vector<uint8_t> bytes = request.body.to_bytes();
    std::string body(bytes.begin(), bytes.end());
    if (body.find(static_cast<char>(0x80)) != std::string::npos) {
        return text_response(400, "request body must be valid UTF-8");
    }

    std::vector<std::string> words;
    size_t start = 0;
    while (true) {
        size_t pos = body.find(' ', start);
        words.push_back(body.substr(start, pos == std::string::npos ? std::string::npos : pos - start));
        if (pos == std::string::npos) {
            break;
        }
        start = pos + 1;
    }
    std::reverse(words.begin(), words.end());

    std::string reversed;
    for (size_t i = 0; i < words.size(); ++i) {
        if (i != 0) {
            reversed += " ";
        }
        reversed += words[i];
    }

    return text_response(200, reversed);
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .post("/reverse-bytes", reverse_bytes)
        .post("/reverse-words", reverse_words);
}
