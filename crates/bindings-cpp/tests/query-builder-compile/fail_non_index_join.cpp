#include <spacetimedb.h>

using namespace SpacetimeDB;

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

auto invalid_join = QueryBuilder{}[user].right_semijoin(
    QueryBuilder{}[membership],
    [](const auto& users, const auto& memberships) {
        return users.tenant_id.eq(memberships.tenant_id);
    });
