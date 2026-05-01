use spacetimedb::{client_visibility_filter, Filter};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[client_visibility_filter]
const HIDE_PEOPLE_EXCEPT_ME: Filter = Filter::Sql("SELECT * FROM person WHERE name = 'me'");
