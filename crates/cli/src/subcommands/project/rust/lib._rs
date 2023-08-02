use spacetimedb::{spacetimedb, ReducerContext};

#[spacetimedb(table)]
pub struct Person {
    name: String
}

#[spacetimedb(init)]
pub fn init() {
    // Called when the module is initially published
}

#[spacetimedb(connect)]
pub fn identity_connected(_ctx: ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb(disconnect)]
pub fn identity_disconnected(_ctx: ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
