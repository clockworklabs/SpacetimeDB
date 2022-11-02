use spacetimedb::{spacetimedb, println, Hash};

#[spacetimedb(table)]
pub struct Person {
    name: String
}

#[spacetimedb(reducer)]
pub fn add(_sender: Hash, _timestamp: u64, name: String) {
    Person::insert(Person { name })
}

#[spacetimedb(reducer)]
pub fn say_hello(_sender: Hash, _timestamp: u64) {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}