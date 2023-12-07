use spacetimedb::{println, query, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
    age: u8,
}

#[spacetimedb(reducer)]
pub fn add(name: String, age: u8) {
    Person::insert(Person { name, age });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}

#[spacetimedb(reducer)]
pub fn list_over_age(age: u8) {
    for person in query!(|person: Person| person.age >= age) {
        println!("{} has age {} >= {}", person.name, person.age, age);
    }
}
