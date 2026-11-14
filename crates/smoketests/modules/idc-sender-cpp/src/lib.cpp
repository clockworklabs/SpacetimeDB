// An IDC sender. Mirrors idc-sender (Rust) and idc-sender-ts, with extra reducers
// for rollback and on_result failure.
#include <spacetimedb.h>
#include <game_world/bindings.h>
#include <auth/bindings.h>

using namespace SpacetimeDB;
namespace gw = game_world;

constexpr uint32_t MODULE_VERSION = 1;

// Calls to this database also work, so two senders can call each other.
struct PingLog {
    uint64_t id;
    Identity sender;
    std::string body;
};
SPACETIMEDB_STRUCT(PingLog, id, sender, body)
SPACETIMEDB_TABLE(PingLog, ping_log, Private)
FIELD_PrimaryKeyAutoInc(ping_log, id)

SPACETIMEDB_REDUCER(receive_ping, ReducerContext ctx, std::string body) {
    ctx.db[ping_log].insert(PingLog{0, ctx.sender(), body});
    return Ok();
}

// A manual outbox with reordered columns, an aliased column, and other names for
// the message ID and target.
struct OutboundPing {
    std::string bodypo;
    uint64_t message_number;
    gw::Identity destination;
};
SPACETIMEDB_STRUCT(OutboundPing, bodypo, message_number, destination)
SPACETIMEDB_OUTBOX_TABLE(OutboundPing, outbound_ping, Private, gw::reducers::receive_ping)
FIELD_PrimaryKeyAutoInc(outbound_ping, message_number)
FIELD_Target(outbound_ping, destination)
FIELD_Param(outbound_ping, bodypo, body)

SPACETIMEDB_REDUCER(enqueue_ping, ReducerContext ctx, gw::Identity target, std::string body) {
    ctx.db[outbound_ping].insert(OutboundPing{body, 0, target});
    return Ok();
}

SPACETIMEDB_REDUCER(enqueue_pings, ReducerContext ctx, gw::Identity target, std::vector<std::string> bodies) {
    for (const auto& body : bodies) {
        ctx.db[outbound_ping].insert(OutboundPing{body, 0, target});
    }
    return Ok();
}

// The call is sent only if this transaction commits, so this sends nothing.
SPACETIMEDB_REDUCER(enqueue_ping_then_fail, ReducerContext ctx, gw::Identity target, std::string body) {
    ctx.db[outbound_ping].insert(OutboundPing{body, 0, target});
    return Err("rolled back after enqueueing " + body);
}

struct PingResult {
    uint64_t id;
    uint64_t callback_order;
    uint32_t module_version;
    std::string body;
    bool succeeded;
    std::optional<std::string> error;
};
SPACETIMEDB_STRUCT(PingResult, id, callback_order, module_version, body, succeeded, error)
SPACETIMEDB_TABLE(PingResult, ping_result, Private)
FIELD_PrimaryKey(ping_result, id)
FIELD_AutoInc(ping_result, callback_order)

SPACETIMEDB_REDUCER_ON_RESULT(outbound_ping, on_ping_result,
                              ReducerContext ctx, OutboundPing row, OutboxResult result) {
    ctx.db[ping_result].insert(PingResult{
        row.message_number,
        0,
        MODULE_VERSION,
        row.bodypo,
        result.is_ok(),
        result.is_err() ? std::optional<std::string>(result.unwrap_err()) : std::nullopt,
    });
    // A failing on_result rolls back its own changes, but the row is still deleted.
    if (row.bodypo == "fail-callback") {
        return Err("on_result failed for " + row.bodypo);
    }
    return Ok();
}

// A generated outbox.
SPACETIMEDB_OUTBOX_FOR(gw::reducers::receive_detailed_ping, outbound_detailed_ping)
using OutboundDetailedPing = gw::reducers::receive_detailed_ping::outbox_row;

SPACETIMEDB_REDUCER(enqueue_detailed_ping, ReducerContext ctx, gw::Identity target, std::string body) {
    gw::PingDetails details{
        {"nested", "idc"},
        {
            gw::PingCheckpoint{1, gw::PingState::Ready()},
            gw::PingCheckpoint{2, gw::PingState::Deferred(std::string("waiting"))},
            gw::PingCheckpoint{3, gw::PingState::Deferred(std::nullopt)},
        },
        std::string("optional-note"),
    };
    ctx.db[outbound_detailed_ping].insert(OutboundDetailedPing{0, target, body, details});
    return Ok();
}

struct DetailedPingResult {
    uint64_t id;
    std::string body;
    gw::PingDetails details;
};
SPACETIMEDB_STRUCT(DetailedPingResult, id, body, details)
SPACETIMEDB_TABLE(DetailedPingResult, detailed_ping_result, Private)
FIELD_PrimaryKey(detailed_ping_result, id)

SPACETIMEDB_REDUCER_ON_RESULT(outbound_detailed_ping, on_detailed_ping_result,
                              ReducerContext ctx, OutboundDetailedPing row, OutboxResult result) {
    if (result.is_err()) {
        return Err(result.unwrap_err());
    }
    ctx.db[detailed_ping_result].insert(DetailedPingResult{row.msg_id, row.body, row.details});
    return Ok();
}

// A second dependency on the same receiver module. Its generated types are distinct
// from game_world's and from this module's own PingDetails.
struct OutboundAuthDetails {
    uint64_t msg_id;
    auth::Identity target;
    std::string body;
    auth::PingDetails details;
};
SPACETIMEDB_STRUCT(OutboundAuthDetails, msg_id, target, body, details)
SPACETIMEDB_OUTBOX_TABLE(OutboundAuthDetails, outbound_auth_details, Private, auth::reducers::receive_detailed_ping)
FIELD_PrimaryKeyAutoInc(outbound_auth_details, msg_id)
FIELD_Target(outbound_auth_details, target)

struct PingDetails {
    std::string source;
};
SPACETIMEDB_STRUCT(PingDetails, source)

struct SenderDetails {
    uint64_t id;
    PingDetails details;
};
SPACETIMEDB_STRUCT(SenderDetails, id, details)
SPACETIMEDB_TABLE(SenderDetails, sender_details, Private)
FIELD_PrimaryKey(sender_details, id)

SPACETIMEDB_REDUCER(enqueue_two_dependencies, ReducerContext ctx, gw::Identity world, auth::Identity auth_target) {
    ctx.db[outbound_detailed_ping].insert(
        OutboundDetailedPing{0, world, "world-dependency", gw::PingDetails{{"world"}, {}, std::nullopt}});
    ctx.db[outbound_auth_details].insert(
        OutboundAuthDetails{0, auth_target, "auth-dependency", auth::PingDetails{{"auth"}, {}, std::nullopt}});
    ctx.db[sender_details].insert(SenderDetails{1, PingDetails{"sender"}});
    return Ok();
}
