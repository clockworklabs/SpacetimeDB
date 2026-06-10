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

auto invalid_filter = TableFor<User>("user").where([](const auto& users, const auto& ix) {
    return ix.identity.eq(users.identity);
});
