use spacetimedb::{client_visibility_filter, Filter};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[client_visibility_filter]
// Bug: `Person` is the wrong table name, should be `person`.
const HIDE_PEOPLE_EXCEPT_ME: Filter = Filter::Sql("SELECT * FROM Person WHERE name = 'me'");
