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
};
SPACETIMEDB_STRUCT(User, identity)
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, identity)

struct Membership {
    Identity membership_identity;
    uint64_t tenant_id;
};
SPACETIMEDB_STRUCT(Membership, membership_identity, tenant_id)
SPACETIMEDB_TABLE(Membership, membership, Public)
FIELD_PrimaryKey(membership, membership_identity)
FIELD_Index(membership, tenant_id)

auto invalid_join = TableFor<User>("user").right_semijoin(
    TableFor<Membership>("membership"),
    [](const auto& users, const auto& memberships) {
        return users.identity.eq(memberships.tenant_id);
    });
