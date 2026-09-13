#include "test_harness.h"
#include "spacetimedb/procedure_context.h"

#include <algorithm>
#include <cstring>

using namespace SpacetimeDB;

namespace {
uint32_t auth_flags;
size_t flag_reads;
size_t jwt_reads;
size_t payload_offset;
std::string jwt_payload;

Identity verified_sender() {
    std::array<uint8_t, 32> bytes{};
    bytes[0] = 42;
    return Identity(bytes);
}

void reset_host(uint32_t flags, std::string payload = {}) {
    auth_flags = flags;
    flag_reads = jwt_reads = payload_offset = 0;
    jwt_payload = std::move(payload);
}
}

extern "C" uint32_t get_call_auth_flags() {
    ++flag_reads;
    return auth_flags;
}

extern "C" Status get_jwt(const uint8_t*, BytesSource* out) {
    ++jwt_reads;
    payload_offset = 0;
    *out = BytesSource{jwt_payload.empty() ? 0u : 1u};
    return Status{0};
}

extern "C" Status env_get(const uint8_t* key, uint32_t key_len, BytesSource* out) {
    payload_offset = 0;
    const std::string name(reinterpret_cast<const char*>(key), key_len);
    if (name == "ERROR") return Status{1};
    *out = BytesSource{name == "MISSING" ? 0u : 1u};
    return Status{0};
}

extern "C" int16_t bytes_source_read(BytesSource, uint8_t* out, size_t* len) {
    *len = std::min(*len, jwt_payload.size() - payload_offset);
    std::memcpy(out, jwt_payload.data() + payload_offset, *len);
    payload_offset += *len;
    // Successful exhaustion can return the last bytes together with -1.
    return payload_offset == jwt_payload.size() ? -1 : 0;
}

extern "C" void identity(uint8_t* out) { std::memset(out, 0, 32); }
extern "C" Status procedure_start_mut_tx(int64_t* out) { *out = 0; return Status{0}; }
extern "C" Status procedure_commit_mut_tx() { return Status{0}; }
extern "C" Status procedure_abort_mut_tx() { return Status{0}; }
extern "C" void console_log(LogLevel, const uint8_t*, size_t, const uint8_t*, size_t,
                            uint32_t, const uint8_t*, size_t) {}

TEST_CASE(authority_without_connection_is_captured_from_host) {
    for (uint32_t flags : {0u, 1u}) {
        reset_host(flags);
        auto ctx = AuthCtx::from_connection_id_opt(std::nullopt, verified_sender());
        auth_flags = flags ^ 1;
        ASSERT_EQ(size_t{1}, flag_reads);
        ASSERT_EQ(flags == 1, ctx.is_internal());
        ASSERT_TRUE(!ctx.has_jwt());
        ASSERT_EQ(verified_sender(), ctx.get_caller_identity());
        ASSERT_EQ(size_t{0}, jwt_reads);
    }
}

TEST_CASE(internal_call_retains_lazy_jwt_and_verified_identity) {
    reset_host(1, R"({"iss":"other","sub":"other","identity":"untrusted"})");
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), verified_sender());
    auth_flags = 0;
    ASSERT_TRUE(ctx.is_internal());
    ASSERT_EQ(size_t{0}, jwt_reads);
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(verified_sender(), ctx.get_jwt()->get_identity());
    ASSERT_EQ(std::string("other"), ctx.get_jwt()->subject());
    ASSERT_EQ(size_t{1}, jwt_reads);
}

TEST_CASE(jwt_source_reads_all_chunks_including_final_exhausted_bytes) {
    reset_host(0, "{\"padding\":\"" + std::string(8192, 'x') + "\",\"sub\":\"last\"}");
    auto ctx = AuthCtx::from_connection_id(ConnectionId(5), verified_sender());
    ASSERT_TRUE(ctx.has_jwt());
    ASSERT_EQ(std::string("last"), ctx.get_jwt()->subject());
    ASSERT_EQ(jwt_payload.size(), payload_offset);
    ASSERT_EQ(size_t{1}, jwt_reads);
}

TEST_CASE(procedure_transactions_preserve_authority_connection_and_sender) {
    for (uint64_t connection : {0u, 5u}) {
        reset_host(1, R"({"sub":"worker"})");
        ProcedureContext ctx(verified_sender(), Timestamp::from_micros_since_epoch(0), ConnectionId(connection));
        auth_flags = 0;
        ctx.with_tx([&](TxContext& tx) {
            ASSERT_TRUE(tx.sender_auth().is_internal());
            ASSERT_EQ(verified_sender(), tx.sender());
            ASSERT_EQ(connection != 0, tx.connection_id.has_value());
            ASSERT_EQ(connection != 0, tx.sender_auth().has_jwt());
            if (connection) ASSERT_EQ(verified_sender(), tx.sender_auth().get_jwt()->get_identity());
        });
        ASSERT_EQ(size_t{1}, flag_reads);
    }
}

TEST_CASE(environment_preserves_missing_empty_and_all_chunks_without_caching) {
    Environment env;
    reset_host(0);
    ASSERT_TRUE(!env.get("MISSING").has_value());
    ASSERT_EQ(std::string{}, env.get("EMPTY").value());
    jwt_payload = std::string(8192, 'x');
    ASSERT_EQ(jwt_payload, env.get("LARGE").value());
    jwt_payload = std::string("a\0b", 3);
    ASSERT_EQ(jwt_payload, env.get("NUL").value());
    jwt_payload = "updated";
    ASSERT_EQ(jwt_payload, env.get("NUL").value());
    ProcedureContext procedure(verified_sender(), Timestamp::from_micros_since_epoch(0), ConnectionId(0));
    ASSERT_EQ(jwt_payload, procedure.env.get("VALUE").value());
    procedure.with_tx([&](TxContext& tx) { ASSERT_EQ(jwt_payload, tx.env.get("VALUE").value()); });
}

TEST_CASE(optional_reader_matches_canonical_bsatn_tags_and_preserves_following_bytes) {
    const std::vector<uint8_t> bytes{1, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 'a', 0, 'b', 42};
    bsatn::Reader reader(bytes.data(), bytes.size());
    ASSERT_TRUE(!bsatn::deserialize<std::optional<std::string>>(reader).has_value());
    ASSERT_EQ(std::string{}, bsatn::deserialize<std::optional<std::string>>(reader).value());
    ASSERT_EQ(std::string("a\0b", 3), bsatn::deserialize<std::optional<std::string>>(reader).value());
    ASSERT_EQ(uint8_t{42}, reader.read_u8());
}
