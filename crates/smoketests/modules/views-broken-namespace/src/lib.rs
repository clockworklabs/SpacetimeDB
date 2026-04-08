use spacetimedb::ViewContext;

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::view(accessor = person, public)]
pub fn person(ctx: &ViewContext) -> Option<Person> {
    None
}
