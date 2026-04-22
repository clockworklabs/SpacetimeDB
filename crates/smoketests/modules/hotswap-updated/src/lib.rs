use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::table(accessor = pet, public)]
pub struct Pet {
    #[primary_key]
    species: String,
}

#[spacetimedb::reducer]
pub fn add_pet(ctx: &ReducerContext, species: String) {
    ctx.db.pet().insert(Pet { species });
}
