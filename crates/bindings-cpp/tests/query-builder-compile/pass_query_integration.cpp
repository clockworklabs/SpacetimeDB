#include <spacetimedb.h>

using namespace SpacetimeDB;

struct User {
    Identity identity;
    bool online;
};
SPACETIMEDB_STRUCT(User, identity, online)
SPACETIMEDB_TABLE(User, user, Public)
FIELD_PrimaryKey(user, identity)

struct UserMembership {
    uint64_t id;
    Identity user_identity;
};
SPACETIMEDB_STRUCT(UserMembership, id, user_identity)
SPACETIMEDB_TABLE(UserMembership, user_membership, Public)
FIELD_PrimaryKey(user_membership, id)
FIELD_Index(user_membership, user_identity)

struct AutoIncUser {
    uint64_t id;
    bool online;
};
SPACETIMEDB_STRUCT(AutoIncUser, id, online)
SPACETIMEDB_TABLE(AutoIncUser, auto_inc_user, Public)
FIELD_PrimaryKeyAutoInc(auto_inc_user, id)

struct AutoIncMembership {
    uint64_t id;
    uint64_t auto_inc_user_id;
};
SPACETIMEDB_STRUCT(AutoIncMembership, id, auto_inc_user_id)
SPACETIMEDB_TABLE(AutoIncMembership, auto_inc_membership, Public)
FIELD_PrimaryKey(auto_inc_membership, id)
FIELD_Index(auto_inc_membership, auto_inc_user_id)

SPACETIMEDB_CLIENT_VISIBILITY_FILTER(
    online_users_filter,
    QueryBuilder{}[user].where([](const auto& users) {
        return users.online;
    }))

SPACETIMEDB_VIEW(Query<User>, online_users, Public, AnonymousViewContext ctx) {
    return ctx.from[user].where([](const auto& users) {
        return users.online;
    });
}

SPACETIMEDB_VIEW(Query<User>, online_users_filter_alias, Public, AnonymousViewContext ctx) {
    return ctx.from[user].filter([](const auto& users) {
        return users.online;
    });
}

SPACETIMEDB_VIEW(std::optional<User>, first_online_user, Public, AnonymousViewContext ctx) {
    (void)ctx;
    return std::optional<User>(User{Identity{}, true});
}

SPACETIMEDB_VIEW(Query<User>, online_users_copy, Public, AnonymousViewContext ctx) {
    return ctx.from[online_users_view].where([](const auto& users) {
        return users.online;
    });
}

SPACETIMEDB_VIEW(Query<User>, first_online_user_copy, Public, AnonymousViewContext ctx) {
    return ctx.from[first_online_user_view].where([](const auto& users) {
        return users.online;
    });
}

SPACETIMEDB_VIEW(Query<User>, online_member_users, Public, AnonymousViewContext ctx) {
    return ctx.from[user_membership].right_semijoin(
        ctx.from[user],
        [](const auto& memberships, const auto& users) {
            return memberships.user_identity.eq(users.identity);
        })
        .where([](const auto& users) {
            return users.online;
        });
}

SPACETIMEDB_VIEW(Query<AutoIncUser>, online_auto_inc_users, Public, AnonymousViewContext ctx) {
    return ctx.from[auto_inc_membership].right_semijoin(
        ctx.from[auto_inc_user],
        [](const auto& memberships, const auto& users) {
            return memberships.auto_inc_user_id.eq(users.id);
        })
        .where([](const auto& users) {
            return users.online;
        });
}
