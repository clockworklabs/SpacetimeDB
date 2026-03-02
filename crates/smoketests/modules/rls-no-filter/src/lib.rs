use spacetimedb::{Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = users, public)]
pub struct Users {
    name: String,
    identity: Identity,
}

#[spacetimedb::reducer]
pub fn add_user(ctx: &ReducerContext, name: String) {
    ctx.db.users().insert(Users {
        name,
        identity: ctx.sender(),
    });
}
