use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(accessor = person)]
pub struct Person {
    #[index(btree)]
    name: String,
    #[default(0)]
    age: u16,
    #[default(19)]
    mass: u16,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person {
        name,
        age: 70,
        mass: 180,
    });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    log::info!("FIRST_UPDATE: client disconnected");
}
