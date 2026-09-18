#include "test_harness.h"
#include "spacetimedb/environment.h"
#include "spacetimedb/bsatn/reader.h"
#include <algorithm>
#include <cstring>

using namespace SpacetimeDB;

namespace {
size_t payload_offset;
std::string payload;
}

extern "C" Status env_get(const uint8_t* key, uint32_t key_len, BytesSource* out) {
    payload_offset = 0;
    const std::string name(reinterpret_cast<const char*>(key), key_len);
    *out = BytesSource{name == "MISSING" ? 0u : 1u};
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
