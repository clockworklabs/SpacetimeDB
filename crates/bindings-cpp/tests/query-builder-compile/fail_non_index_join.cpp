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
    Identity identity;
    uint64_t tenant_id;
};
SPACETIMEDB_STRUCT(User, identity, tenant_id)
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, identity)

struct Membership {
    Identity identity;
    uint64_t tenant_id;
};
SPACETIMEDB_STRUCT(Membership, identity, tenant_id)
SPACETIMEDB_TABLE(Membership, membership, Public)
FIELD_PrimaryKey(membership, identity)

auto invalid_join = TableFor<User>("user").right_semijoin(
    TableFor<Membership>("membership"),
    [](const auto& users, const auto& memberships) {
        return users.tenant_id.eq(memberships.tenant_id);
    });
