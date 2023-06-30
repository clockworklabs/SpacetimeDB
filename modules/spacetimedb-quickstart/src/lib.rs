use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(table)]
pub struct _Private {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}

#[spacetimedb(reducer)]
pub fn add_private(name: String) {
    _Private::insert(_Private { name });
}

#[spacetimedb(reducer)]
pub fn query_private() {
    for person in _Private::iter() {
        println!("Private, {}!", person.name);
    }
    println!("Private, World!");
}
