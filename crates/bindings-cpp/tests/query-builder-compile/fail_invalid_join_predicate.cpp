#include <spacetimedb.h>

using namespace SpacetimeDB;

template<typename TRow>
auto TableFor(const char* table_name) {
    return QueryBuilder{}.table<TRow>(
        table_name,
        query_builder::HasCols<TRow>::get(table_name),
        query_builder::HasIxCols<TRow>::get(table_name));
}

struct User {
    uint64_t id;
};
SPACETIMEDB_STRUCT(User, id)
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, id)

struct Membership {
    uint64_t id;
    uint64_t user_id;
};
SPACETIMEDB_STRUCT(Membership, id, user_id)
SPACETIMEDB_TABLE(Membership, membership, Public)
FIELD_PrimaryKey(membership, id)
FIELD_Index(membership, user_id)

auto invalid_join = TableFor<User>("user").right_semijoin(
    TableFor<Membership>("membership"),
    [](const auto& users, const auto& memberships) {
        return users.id.eq(1ULL);
    });
