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

struct AuditEvent {
    Identity identity;
};
SPACETIMEDB_STRUCT(AuditEvent, identity)
SPACETIMEDB_TABLE(AuditEvent, audit_event, Public, true)
FIELD_PrimaryKey(audit_event, identity)

auto invalid_event_lookup = TableFor<User>("user").right_semijoin(
    TableFor<AuditEvent>("audit_event"),
    [](const auto& users, const auto& events) {
        return users.identity.eq(events.identity);
    });
