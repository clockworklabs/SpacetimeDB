use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
    Person::insert(Person { name })
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
