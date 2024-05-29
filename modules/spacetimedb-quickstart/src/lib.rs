use spacetimedb::{println, query};

#[spacetimedb::table(public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
    age: u8,
}

#[spacetimedb::reducer]
pub fn add(name: String, age: u8) {
    Person::insert(Person { id: 0, name, age }).unwrap();
}

#[spacetimedb::reducer]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}

#[spacetimedb::reducer]
pub fn list_over_age(age: u8) {
    for person in query!(|person: Person| person.age >= age) {
        println!("{} has age {} >= {}", person.name, person.age, age);
    }
}
