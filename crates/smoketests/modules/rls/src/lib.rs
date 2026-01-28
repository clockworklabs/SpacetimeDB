use spacetimedb::{Identity, ReducerContext, Table};

#[spacetimedb::table(name = users, public)]
pub struct Users {
    name: String,
    identity: Identity,
}

#[spacetimedb::client_visibility_filter]
const USER_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql(
    "SELECT * FROM users WHERE identity = :sender"
);

#[spacetimedb::reducer]
pub fn add_user(ctx: &ReducerContext, name: String) {
    ctx.db.users().insert(Users { name, identity: ctx.sender });
}
