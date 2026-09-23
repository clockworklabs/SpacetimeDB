use spacetimedb::{client_visibility_filter, Filter, Identity};

#[spacetimedb::table(accessor = user)]
pub struct User {
    identity: Identity,
}

#[client_visibility_filter]
const PERSON_FILTER: Filter = Filter::Sql("SELECT * FROM \"user\" WHERE identity = :sender");
