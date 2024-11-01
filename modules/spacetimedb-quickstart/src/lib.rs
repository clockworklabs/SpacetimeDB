use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
    age: u8,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String, age: u8) {
    ctx.db.person().insert(Person { id: 0, name, age });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}

#[spacetimedb::reducer]
pub fn list_over_age(ctx: &ReducerContext, age: u8) {
    for person in ctx.db.person().iter().filter(|person| person.age >= age) {
        log::info!("{} has age {} >= {}", person.name, person.age, age);
    }
}
