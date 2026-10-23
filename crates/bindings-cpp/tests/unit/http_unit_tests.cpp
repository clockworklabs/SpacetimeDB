#include "test_harness.h"

#include "spacetimedb/http_convert.h"
#include "spacetimedb/handler_context.h"

#include <string>
#include <utility>
#include <vector>

using namespace SpacetimeDB;

TEST_CASE(request_from_wire_preserves_metadata_and_body) {
    wire::HttpRequest request;
    request.method = wire::HttpMethod{wire::HttpMethod::Tag::Post, ""};
    request.headers.entries = {
        wire::HttpHeaderPair{"content-type", std::vector<uint8_t>{'a','p','p','l','i','c','a','t','i','o','n','/','o','c','t','e','t','-','s','t','r','e','a','m'}},
        wire::HttpHeaderPair{"x-echo", std::vector<uint8_t>{'v','a','l','u','e'}},
    };
    request.timeout = std::nullopt;
    request.uri = "https://example.invalid/upload?x=1";
    request.version = wire::HttpVersion{wire::HttpVersion::Tag::Http2};

    HttpRequest converted = convert::from_wire(request, std::vector<uint8_t>{'p','a','y','l','o','a','d'});

    ASSERT_EQ(std::string("POST"), converted.method.value);
    ASSERT_EQ(std::string("https://example.invalid/upload?x=1"), converted.uri);
    ASSERT_EQ(HttpVersion::Http2, converted.version);
    ASSERT_EQ(static_cast<size_t>(2), converted.headers.size());
    ASSERT_EQ(std::string("content-type"), converted.headers[0].name);
    ASSERT_EQ(std::vector<uint8_t>({'a','p','p','l','i','c','a','t','i','o','n','/','o','c','t','e','t','-','s','t','r','e','a','m'}), converted.headers[0].value);
    ASSERT_EQ(std::string("x-echo"), converted.headers[1].name);
    ASSERT_EQ(std::vector<uint8_t>({'v','a','l','u','e'}), converted.headers[1].value);
    ASSERT_EQ(std::vector<uint8_t>({'p','a','y','l','o','a','d'}), converted.body.bytes);
}

TEST_CASE(response_into_wire_splits_metadata_and_body) {
    HttpResponse response{
        201,
        HttpVersion::Http11,
        {
            HttpHeader{"content-type", "text/plain"},
            HttpHeader{"x-result", "ok"},
        },
        HttpBody::from_string("created"),
    };

    auto [response_meta, response_body] = convert::to_wire_split(response);

    ASSERT_EQ(static_cast<uint16_t>(201), response_meta.code);
    ASSERT_EQ(wire::HttpVersion::Tag::Http11, response_meta.version.tag);
    ASSERT_EQ(static_cast<size_t>(2), response_meta.headers.entries.size());
    ASSERT_EQ(std::string("content-type"), response_meta.headers.entries[0].name);
    ASSERT_EQ(std::vector<uint8_t>({'t','e','x','t','/','p','l','a','i','n'}), response_meta.headers.entries[0].value);
    ASSERT_EQ(std::string("x-result"), response_meta.headers.entries[1].name);
    ASSERT_EQ(std::vector<uint8_t>({'o','k'}), response_meta.headers.entries[1].value);
    ASSERT_EQ(std::vector<uint8_t>({'c','r','e','a','t','e','d'}), response_body);
}

namespace {
size_t commit_attempts;
}

extern "C" Status procedure_start_mut_tx(int64_t* out) { *out = 0; return Status{0}; }
extern "C" Status procedure_commit_mut_tx() {
    return Status{static_cast<uint16_t>(commit_attempts++ == 0 ? 1 : 0)};
}
extern "C" Status procedure_abort_mut_tx() { return Status{0}; }

TEST_CASE(handler_transactions_are_external_without_jwt_even_on_retry) {
    HandlerContext handler;
    for (bool fallible : {false, true}) {
        commit_attempts = 0;
        size_t calls = 0;
        auto check = [&](TxContext& tx) {
            ++calls;
            ASSERT_TRUE(!tx.sender_auth().is_internal());
            ASSERT_TRUE(!tx.sender_auth().has_jwt());
            ASSERT_TRUE(!tx.sender_auth().get_jwt().has_value());
        };
        if (fallible) {
            ASSERT_TRUE(handler.try_with_tx([&](TxContext& tx) { check(tx); return true; }));
        } else {
            handler.with_tx(check);
        }
        ASSERT_EQ(size_t{2}, calls);
    }
}
