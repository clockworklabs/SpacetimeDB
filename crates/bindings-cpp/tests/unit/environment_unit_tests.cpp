#include "test_harness.h"
#include "spacetimedb.h"
#include "spacetimedb/environment.h"
#include "spacetimedb/bsatn/reader.h"
#include <algorithm>
#include <array>
#include <cstring>

using namespace SpacetimeDB;

namespace {
size_t payload_offset;
std::string payload;
// Nonzero, so that it differs from the `Identity{}` sender of HTTP handlers.
const std::array<uint8_t, 32> database_identity_bytes = [] {
    std::array<uint8_t, 32> bytes;
    bytes.fill(7);
    return bytes;
}();
}

extern "C" void identity(uint8_t* out) {
    std::memcpy(out, database_identity_bytes.data(), database_identity_bytes.size());
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

TEST_CASE(is_internal_is_true_exactly_when_the_sender_is_the_database) {
    const Identity database(database_identity_bytes);
    ASSERT_TRUE(AuthCtx::from_connection_id_opt(std::nullopt, database).is_internal());
    ASSERT_TRUE(AuthCtx::from_connection_id_opt(ConnectionId(5), database).is_internal());
    ASSERT_TRUE(!AuthCtx::from_connection_id_opt(std::nullopt, Identity{}).is_internal());
    ASSERT_TRUE(!AuthCtx::from_connection_id_opt(ConnectionId(5), Identity{}).is_internal());
    ASSERT_TRUE(!ReducerContext(Identity{}, std::nullopt, Timestamp{}).sender_auth().is_internal());
}

TEST_CASE(internal_invocations_have_no_jwt) {
    payload = R"({"sub":"container"})";
    const Identity database(database_identity_bytes);
    auto internal = AuthCtx::from_connection_id_opt(ConnectionId(5), database);
    ASSERT_TRUE(!internal.has_jwt());
    ASSERT_TRUE(!internal.get_jwt().has_value());
    auto external = AuthCtx::from_connection_id_opt(ConnectionId(5), Identity{});
    ASSERT_TRUE(external.has_jwt());
    ASSERT_EQ(std::string("container"), external.get_jwt()->subject());
}

TEST_CASE(caller_identity_is_the_sender_with_or_without_a_jwt) {
    const Identity database(database_identity_bytes);
    std::array<uint8_t, 32> client_bytes{};
    client_bytes.fill(3);
    const Identity client(client_bytes);

    // An HTTP handler transaction, or `init`, has a sender that is neither the database nor backed by a JWT.
    payload.clear();
    ASSERT_TRUE(Identity{} == ReducerContext(Identity{}, std::nullopt, Timestamp{}).sender_auth().get_caller_identity());
    ASSERT_TRUE(client == AuthCtx::from_connection_id_opt(std::nullopt, client).get_caller_identity());
    ASSERT_TRUE(client == AuthCtx::from_connection_id_opt(ConnectionId(5), client).get_caller_identity());

    payload = R"({"sub":"client"})";
    ASSERT_TRUE(client == AuthCtx::from_connection_id_opt(ConnectionId(5), client).get_caller_identity());
    ASSERT_TRUE(database == AuthCtx::from_connection_id_opt(std::nullopt, database).get_caller_identity());
    ASSERT_TRUE(database == AuthCtx::internal().get_caller_identity());
}
