#include <spacetimedb.h>

using namespace SpacetimeDB;

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

auto invalid_event_lookup = QueryBuilder{}[user].right_semijoin(
    QueryBuilder{}[audit_event],
    [](const auto& users, const auto& events) {
        return users.identity.eq(events.identity);
    });
