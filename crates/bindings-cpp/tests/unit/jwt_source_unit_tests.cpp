#include "test_harness.h"
#include "spacetimedb.h"

#include <algorithm>
#include <cstring>

using namespace SpacetimeDB;

namespace {
size_t jwt_reads;
size_t payload_offset;
std::string jwt_payload;

Identity verified_sender() {
    std::array<uint8_t, 32> bytes{};
    bytes[0] = 42;
    return Identity(bytes);
}

void reset_host(std::string payload) {
    jwt_reads = payload_offset = 0;
    jwt_payload = std::move(payload);
}
}

extern "C" Status get_jwt(const uint8_t*, BytesSource* out) {
    ++jwt_reads;
    payload_offset = 0;
    *out = BytesSource{jwt_payload.empty() ? 0u : 1u};
    return Status{0};
}

extern "C" int16_t bytes_source_read(BytesSource, uint8_t* out, size_t* len) {
    *len = std::min(*len, jwt_payload.size() - payload_offset);
    std::memcpy(out, jwt_payload.data() + payload_offset, *len);
    payload_offset += *len;
    // Successful exhaustion can return the last bytes together with -1.
    return payload_offset == jwt_payload.size() ? -1 : 0;
}

TEST_CASE(jwt_source_reads_all_chunks_including_final_exhausted_bytes) {
    reset_host("{\"padding\":\"" + std::string(8192, 'x') + "\",\"sub\":\"last\"}");
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), verified_sender());
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(std::string("last"), ctx.get_jwt()->subject());
    ASSERT_EQ(jwt_payload.size(), payload_offset);
    ASSERT_EQ(size_t{1}, jwt_reads);
}

TEST_CASE(jwt_source_keeps_final_bytes_from_a_single_read) {
    reset_host(R"({"sub":"short"})");
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), verified_sender());
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(std::string("short"), ctx.get_jwt()->subject());
    ASSERT_EQ(jwt_payload.size(), payload_offset);
}
