// An IDC receiver: ordinary reducers that other databases call through outboxes.
// Mirrors idc-receiver (Rust) and idc-receiver-ts.
#include <spacetimedb.h>

using namespace SpacetimeDB;

constexpr uint32_t MODULE_VERSION = 1;

SPACETIMEDB_ENUM(PingState, (Ready, Unit), (Deferred, std::optional<std::string>))

struct PingCheckpoint {
    uint32_t sequence;
    PingState state;
};
SPACETIMEDB_STRUCT(PingCheckpoint, sequence, state)

struct PingDetails {
    std::vector<std::string> labels;
    std::vector<PingCheckpoint> checkpoints;
    std::optional<std::string> note;
};
SPACETIMEDB_STRUCT(PingDetails, labels, checkpoints, note)

struct PingLog {
    uint64_t id;
    Identity sender;
    uint32_t module_version;
    std::string body;
};
SPACETIMEDB_STRUCT(PingLog, id, sender, module_version, body)
SPACETIMEDB_TABLE(PingLog, ping_log, Private)
FIELD_PrimaryKeyAutoInc(ping_log, id)

SPACETIMEDB_REDUCER(receive_ping, ReducerContext ctx, std::string body) {
    if (body == "reject") {
        return Err("rejected");
    }
    ctx.db[ping_log].insert(PingLog{0, ctx.sender(), MODULE_VERSION, body});
    return Ok();
}

struct DetailedPingLog {
    uint64_t id;
    std::string body;
    PingDetails details;
};
SPACETIMEDB_STRUCT(DetailedPingLog, id, body, details)
SPACETIMEDB_TABLE(DetailedPingLog, detailed_ping_log, Private)
FIELD_PrimaryKeyAutoInc(detailed_ping_log, id)

SPACETIMEDB_REDUCER(receive_detailed_ping, ReducerContext ctx, std::string body, PingDetails details) {
    ctx.db[detailed_ping_log].insert(DetailedPingLog{0, body, details});
    return Ok();
}
