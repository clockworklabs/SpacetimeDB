#include "test_harness.h"
#include "spacetimedb.h"
#include "spacetimedb/environment.h"
#include "spacetimedb/bsatn/reader.h"
#include <algorithm>
#include <cstring>

using namespace SpacetimeDB;

namespace {
size_t payload_offset;
std::string payload;
const std::array<uint8_t, 32> database_bytes{0xdb};
}

extern "C" void identity(uint8_t* out) {
    std::memcpy(out, database_bytes.data(), database_bytes.size());
}

extern "C" Status env_get(const uint8_t* key, uint32_t key_len, BytesSource* out) {
    payload_offset = 0;
    const std::string name(reinterpret_cast<const char*>(key), key_len);
    *out = BytesSource{name == "MISSING" ? 0u : 1u};
    return Status{0};
}

extern "C" Status get_jwt(const uint8_t*, BytesSource* out) {
    payload_offset = 0;
    *out = BytesSource{payload.empty() ? 0u : 1u};
    return Status{0};
}

extern "C" int16_t bytes_source_read(BytesSource, uint8_t* out, size_t* len) {
    *len = std::min(*len, payload.size() - payload_offset);
    std::memcpy(out, payload.data() + payload_offset, *len);
    payload_offset += *len;
    return payload_offset == payload.size() ? -1 : 0;
}

extern "C" void console_log(LogLevel, const uint8_t*, size_t, const uint8_t*, size_t,
                            uint32_t, const uint8_t*, size_t) {}

TEST_CASE(environment_preserves_missing_empty_and_all_chunks_without_caching) {
    Environment env;
    ASSERT_TRUE(!env.get("MISSING").has_value());
    ASSERT_EQ(std::string{}, env.get("EMPTY").value());
    payload = std::string(8192, 'x');
    ASSERT_EQ(payload, env.get("LARGE").value());
    payload = std::string("a\0b", 3);
    ASSERT_EQ(payload, env.get("NUL").value());
    payload = "updated";
    ASSERT_EQ(payload, env.get("NUL").value());
}

TEST_CASE(optional_reader_matches_canonical_bsatn_tags_and_preserves_following_bytes) {
    const std::vector<uint8_t> bytes{1, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 'a', 0, 'b', 42};
    bsatn::Reader reader(bytes.data(), bytes.size());
    ASSERT_TRUE(!bsatn::deserialize<std::optional<std::string>>(reader).has_value());
    ASSERT_EQ(std::string{}, bsatn::deserialize<std::optional<std::string>>(reader).value());
    ASSERT_EQ(std::string("a\0b", 3), bsatn::deserialize<std::optional<std::string>>(reader).value());
    ASSERT_EQ(uint8_t{42}, reader.read_u8());
}

TEST_CASE(jwt_source_reads_all_chunks_including_final_exhausted_bytes) {
    payload = "{\"padding\":\"" + std::string(8192, 'x') + "\",\"sub\":\"last\"}";
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), Identity{});
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(std::string("last"), ctx.get_jwt()->subject());
    ASSERT_EQ(payload.size(), payload_offset);
}

TEST_CASE(jwt_source_keeps_final_bytes_from_a_single_read) {
    payload = R"({"sub":"short"})";
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), Identity{});
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(std::string("short"), ctx.get_jwt()->subject());
    ASSERT_EQ(payload.size(), payload_offset);
}

TEST_CASE(auth_ctx_is_internal_exactly_when_sender_is_the_database) {
    payload = R"({"sub":"connection"})";
    const Identity database(database_bytes);
    auto internal = AuthCtx::from_connection_id(ConnectionId(5), database);
    ASSERT_TRUE(internal.is_internal());
    ASSERT_TRUE(!internal.has_jwt());
    ASSERT_TRUE(!internal.get_jwt().has_value());
    ASSERT_TRUE(internal.get_caller_identity() == database);

    // Like init, whose sender is the owner and which has no ConnectionId.
    const Identity owner(std::array<uint8_t, 32>{1});
    auto init = AuthCtx::from_connection_id_opt(std::nullopt, owner);
    ASSERT_TRUE(!init.is_internal());
    ASSERT_TRUE(!init.has_jwt());
    ASSERT_TRUE(init.get_caller_identity() == owner);
}

TEST_CASE(handler_transactions_use_the_sender_and_the_jwt_read_when_the_handler_starts) {
    payload = R"({"sub":"handler"})";
    payload_offset = 0;
    const Identity sender(std::array<uint8_t, 32>{2});
    HandlerContext handler(sender, BytesSource{1}, Timestamp{});
    ASSERT_EQ(payload.size(), payload_offset);
    ASSERT_TRUE(handler.sender() == sender);
    for (int i = 0; i < 2; ++i) {
        handler.with_tx([&](TxContext& tx) {
            ASSERT_TRUE(tx.sender() == sender);
            ASSERT_TRUE(!tx.sender_auth().is_internal());
            ASSERT_EQ(std::string("handler"), tx.sender_auth().get_jwt()->subject());
            ASSERT_TRUE(tx.sender_auth().get_caller_identity() == sender);
        });
    }
}
