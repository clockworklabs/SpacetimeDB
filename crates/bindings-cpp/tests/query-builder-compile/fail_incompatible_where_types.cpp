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

auto invalid_filter = TableFor<User>("user").where([](const auto& users) {
    return users.identity.eq(users.tenant_id);
});
