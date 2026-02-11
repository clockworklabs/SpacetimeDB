use spacetimedb::ViewContext;

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::view(name = person, public)]
pub fn person(ctx: &ViewContext) -> Option<Person> {
    None
}
