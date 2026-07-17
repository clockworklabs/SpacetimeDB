use spacetimedb::{Filter, Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = user_record, public)]
pub struct UserRecord {
    #[primary_key]
    identity: Identity,
    name: String,
}

#[spacetimedb::client_visibility_filter]
const USER_RECORD_FILTER: Filter =
    Filter::Sql("SELECT * FROM user_record WHERE identity = :sender");

#[spacetimedb::reducer]
pub fn register_self(ctx: &ReducerContext, name: String) {
    ctx.db.user_record().insert(UserRecord { identity: ctx.sender(), name });
}
