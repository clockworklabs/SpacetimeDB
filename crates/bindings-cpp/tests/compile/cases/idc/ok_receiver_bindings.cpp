// Generated receiver bindings for two dependencies compile side by side,
// and their types work as table columns and reducer arguments.
#include <spacetimedb.h>
#include <receiver/bindings.h>
#include <other/bindings.h>

using namespace SpacetimeDB;

namespace rx = receiver;

static_assert(RemoteReducerHandle<rx::reducers::receive_ping>);
static_assert(RemoteReducerHandle<rx::reducers::noop>);
static_assert(rx::reducers::receive_ping::name == "wire_ping");
static_assert(rx::reducers::receive_ping::param_names[0] == "body");
static_assert(std::is_same_v<rx::reducers::receive_ping::param_types, std::tuple<std::string>>);
static_assert(std::tuple_size_v<rx::reducers::noop::param_types> == 0);
static_assert(detail::remote_param_index<rx::reducers::receive_detailed_ping>("details") == 1);
static_assert(detail::remote_param_index<rx::reducers::receive_detailed_ping>("missing") == 2);

// Each dependency has its own identity type.
static_assert(!std::is_same_v<rx::Identity, other::Identity>);
static_assert(!std::is_convertible_v<Identity, rx::Identity>);
static_assert(std::is_constructible_v<rx::Identity, Identity>);
static_assert(!std::is_convertible_v<other::Identity, rx::Identity>);

// Same name as a receiver type: must stay a distinct type in this module's typespace.
struct PingDetails {
    std::string source;
};
SPACETIMEDB_STRUCT(PingDetails, source)

struct Peer {
    uint64_t id;
    rx::Identity world;
    rx::PingDetails details;
    other::PingDetails other_details;
    PingDetails local_details;
};
SPACETIMEDB_STRUCT(Peer, id, world, details, other_details, local_details)
SPACETIMEDB_TABLE(Peer, peer, Private)
FIELD_PrimaryKeyAutoInc(peer, id)
FIELD_Index(peer, world)

SPACETIMEDB_REDUCER(add_peer, ReducerContext ctx, rx::Identity world, std::string note) {
    rx::PingDetails details{
        {"labels"},
        {rx::PingCheckpoint{1, rx::PingState::Ready()}, rx::PingCheckpoint{2, rx::PingState::Deferred(note)}},
        std::nullopt,
        rx::Mood::Excited,
    };
    if (details.checkpoints[1].state.tag() != rx::PingState::Tag::Deferred) {
        return Err("unexpected tag");
    }
    ctx.db[peer].insert(Peer{0, world, details, other::PingDetails{}, PingDetails{"local"}});
    for (const auto& row : ctx.db[peer_world].filter(world)) {
        LOG_INFO("peer " + row.world.to_hex_string());
    }
    rx::Identity self{ctx.sender()};
    (void)self;
    return Ok();
}
