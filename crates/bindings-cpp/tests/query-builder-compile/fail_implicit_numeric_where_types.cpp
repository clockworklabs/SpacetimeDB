#include <spacetimedb.h>

using namespace SpacetimeDB;

template<typename TRow>
auto TableFor(const char* table_name) {
    return QueryBuilder{}.table<TRow>(
        table_name,
        query_builder::HasCols<TRow>::get(table_name),
        query_builder::HasIxCols<TRow>::get(table_name));
}

struct PlayerInfo {
    uint8_t age;
};
SPACETIMEDB_STRUCT(PlayerInfo, age)
SPACETIMEDB_TABLE(PlayerInfo, player_info, Public)

auto invalid_filter = TableFor<PlayerInfo>("player_info").where([](const auto& players) {
    return players.age.eq(4200);
});
