use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(accessor = person)]
pub struct Person {
    name: String,
    age: u16,
    #[default(19)]
    mass: u16,
    #[default(160)]
    height: u32,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person {
        name,
        age: 70,
        mass: 180,
        height: 72,
    });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}
