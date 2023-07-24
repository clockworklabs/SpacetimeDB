#![allow(clippy::too_many_arguments)]
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
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
pub fn single_insert(name: String) {
    println!("inserting {}", name);
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn person_iterator() {
    for person in Person::iter() {
        std::hint::black_box(person);
    }
}

#[spacetimedb(reducer)]
pub fn multi_insert(count: u64, offset: u64) {
    let start = offset;
    let end = offset + count;
    for i in start..end {
        Person::insert(Person {
            name: format!("name {}", i),
        });
    }
}

#[spacetimedb(reducer)]
pub fn empty() {}

#[spacetimedb(reducer)]
pub fn a_lot_of_args(
    arg1: String,
    arg2: String,
    arg3: String,
    arg4: String,
    arg5: String,
    arg6: String,
    arg7: String,
    arg8: String,
    arg9: String,
    arg10: String,
    arg11: String,
    arg12: String,
    arg13: String,
    arg14: String,
    arg15: String,
    arg16: String,
    arg17: String,
    arg18: String,
    arg19: String,
    arg20: String,
    arg21: String,
    arg22: String,
    arg23: String,
    arg24: String,
    arg25: String,
    arg26: String,
    arg27: String,
    arg28: String,
    arg29: String,
    arg30: String,
    arg31: String,
    arg32: String,
) {
    println!("{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
             arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9, arg10,
             arg11, arg12, arg13, arg14, arg15, arg16, arg17, arg18, arg19, arg20,
             arg21, arg22, arg23, arg24, arg25, arg26, arg27, arg28, arg29, arg30,
             arg31, arg32);
}

fn xorshift(x: &mut u64) -> u64 {
    let old_x = *x;
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    old_x
}

#[spacetimedb(table)]
pub struct UniqueLocation {
    #[unique]
    id: u64,
    x: u64,
    y: u64,
}

#[spacetimedb(reducer)]
pub fn create_random_unique_locations(mut seed: u64, count: u64) {
    for _ in 0..count {
        let id = xorshift(&mut seed);
        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = UniqueLocation { id, x, y };
        UniqueLocation::insert(loc).unwrap();
    }
}

#[spacetimedb(reducer)]
pub fn create_sequential_unique_locations(mut seed: u64, start: u64, count: u64) {
    for id in start..start + count {
        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = UniqueLocation { id, x, y };
        UniqueLocation::insert(loc).unwrap();
    }
}

#[spacetimedb(reducer)]
pub fn find_unique_location(id: u64) {
    match UniqueLocation::filter_by_id(&id) {
        Some(loc) => println!("found UniqueLocation {id} at {} {}", loc.x, loc.y),
        None => println!("did not find UniqueLocation {id}"),
    }
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "id", id))]
pub struct NonuniqueLocation {
    id: u64,
    x: u64,
    y: u64,
}

#[spacetimedb(reducer)]
pub fn create_random_nonunique_locations(mut seed: u64, count: u64) {
    for _ in 0..count {
        // Create multiple locations with the same ID.
        let id = xorshift(&mut seed);

        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = NonuniqueLocation { id, x, y };
        NonuniqueLocation::insert(loc);

        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = NonuniqueLocation { id, x, y };
        NonuniqueLocation::insert(loc);
    }
}

#[spacetimedb(reducer)]
pub fn create_sequential_nonunique_locations(mut seed: u64, start: u64, count: u64) {
    for id in start..start + count {
        // Create multiple locations with the same ID.
        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = NonuniqueLocation { id, x, y };
        NonuniqueLocation::insert(loc);

        let x = xorshift(&mut seed);
        let y = xorshift(&mut seed);
        let loc = NonuniqueLocation { id, x, y };
        NonuniqueLocation::insert(loc);
    }
}

#[spacetimedb(reducer)]
pub fn find_nonunique_location(id: u64) {
    for loc in NonuniqueLocation::filter_by_id(&id) {
        println!("found NonuniqueLocation {id} at {} {}", loc.x, loc.y)
    }
}
