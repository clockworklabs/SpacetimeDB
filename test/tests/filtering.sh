#!/bin/bashtable

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This tests filtering reducers."
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, Identity};

#[spacetimedb(table)]
pub struct Person {
    #[unique]
    id: i32,

    name: String,
    #[unique]
    nick: String,
}

#[spacetimedb(reducer)]
pub fn insert_person(id: i32, name: String, nick: String) {
    Person::insert(Person { id, name, nick} );
}

#[spacetimedb(reducer)]
pub fn insert_person_twice(id: i32, name: String, nick: String) {
    Person::insert(Person { id, name: name.clone(), nick: nick.clone()} );
    match Person::insert(Person { id, name: name.clone(), nick: nick.clone()}) {
        Ok(_) => {},
        Err(_) => {
            println!("UNIQUE CONSTRAINT VIOLATION ERROR: id {}: {}", id, name)
        }
    }
}

#[spacetimedb(reducer)]
pub fn delete_person(id: i32) {
    Person::delete_by_id(&id);
}

#[spacetimedb(reducer)]
pub fn find_person(id: i32) {
    match Person::filter_by_id(&id) {
        Some(person) => println!("UNIQUE FOUND: id {}: {}", id, person.name),
        None => println!("UNIQUE NOT FOUND: id {}", id),
    }
}

#[spacetimedb(reducer)]
pub fn find_person_by_name(name: String) {
    for person in Person::filter_by_name(&name) {
        println!("UNIQUE FOUND: id {}: {} aka {}", person.id, person.name, person.nick);
    }
}

#[spacetimedb(reducer)]
pub fn find_person_by_nick(nick: String) {
    match Person::filter_by_nick(&nick) {
        Some(person) => println!("UNIQUE FOUND: id {}: {}", person.id, person.nick),
        None => println!("UNIQUE NOT FOUND: nick {}", nick),
    }
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "by_id", id))]
pub struct NonuniquePerson {
    id: i32,
    name: String,
    is_human: bool,
}

#[spacetimedb(reducer)]
pub fn insert_nonunique_person(id: i32, name: String, is_human: bool) {
    NonuniquePerson::insert(NonuniquePerson { id, name, is_human } );
}

#[spacetimedb(reducer)]
pub fn find_nonunique_person(id: i32) {
    for person in NonuniquePerson::filter_by_id(&id) {
        println!("NONUNIQUE FOUND: id {}: {}", id, person.name)
    }
}

#[spacetimedb(reducer)]
pub fn find_nonunique_humans() {
    for person in NonuniquePerson::filter_by_is_human(&true) {
        println!("HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

#[spacetimedb(reducer)]
pub fn find_nonunique_non_humans() {
    for person in NonuniquePerson::filter_by_is_human(&false) {
        println!("NON-HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

// Ensure that [Identity] is filterable and a legal unique column.
#[spacetimedb(table)]
struct IdentifiedPerson {
    #[unique]
    identity: Identity,
    name: String,
}

fn identify(id_number: u64) -> Identity {
    let mut bytes = [0u8; 32];
    bytes[..8].clone_from_slice(&id_number.to_le_bytes());
    Identity::from_byte_array(bytes)
}

#[spacetimedb(reducer)]
fn insert_identified_person(id_number: u64, name: String) {
    let identity = identify(id_number);
    IdentifiedPerson::insert(IdentifiedPerson { identity, name });
}

#[spacetimedb(reducer)]
fn find_identified_person(id_number: u64) {
    let identity = identify(id_number);
    match IdentifiedPerson::filter_by_identity(&identity) {
        Some(person) => println!("IDENTIFIED FOUND: {}", person.name),
        None => println!("IDENTIFIED NOT FOUND"),
    }
}

// Ensure that indices on non-unique columns behave as we expect.
#[spacetimedb(table)]
#[spacetimedb(index(btree, name="person_surname", surname))]
struct IndexedPerson {
    #[unique]
    id: i32,
    given_name: String,
    surname: String,
}

#[spacetimedb(reducer)]
fn insert_indexed_person(id: i32, given_name: String, surname: String) {
    IndexedPerson::insert(IndexedPerson { id, given_name, surname });
}

#[spacetimedb(reducer)]
fn delete_indexed_person(id: i32) {
    IndexedPerson::delete_by_id(&id);
}

#[spacetimedb(reducer)]
fn find_indexed_people(surname: String) {
    for person in IndexedPerson::filter_by_surname(&surname) {
        println!("INDEXED FOUND: id {}: {}, {}", person.id, person.surname, person.given_name);
    }
}

EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

# Add some people.
run_test cargo run call "$IDENT" insert_person 23 Alice al
run_test cargo run call "$IDENT" insert_person 42 Bob bo
run_test cargo run call "$IDENT" insert_person 64 Bob b2

# Find a person who is there.
run_test cargo run call "$IDENT" find_person 23
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE FOUND: id 23: Alice' == "$(grep 'UNIQUE FOUND: id 23' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Find persons with the same name.
run_test cargo run call "$IDENT" find_person_by_name Bob
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE FOUND: id 42: Bob aka bo' == "$(grep 'UNIQUE FOUND: id 42' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' UNIQUE FOUND: id 64: Bob aka b2' == "$(grep 'UNIQUE FOUND: id 64' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Fail to find a person who is not there.
run_test cargo run call "$IDENT" find_person 43
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE NOT FOUND: id 43' == "$(grep 'UNIQUE NOT FOUND: id 43' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Find a person by nickname.
run_test cargo run call "$IDENT" find_person_by_nick al
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE FOUND: id 23: al' == "$(grep 'UNIQUE FOUND: id 23: al' "$TEST_OUT" | tail -n4 | cut -d: -f6-)" ]

# Remove a person, and then fail to find them.
run_test cargo run call "$IDENT" delete_person 23
run_test cargo run call "$IDENT" find_person 23
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE NOT FOUND: id 23' == "$(grep 'UNIQUE NOT FOUND: id 23' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
# Also fail by nickname
run_test cargo run call "$IDENT" find_person_by_nick al
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE NOT FOUND: nick al' == "$(grep 'UNIQUE NOT FOUND: nick al' "$TEST_OUT" | tail -n4 | cut -d: -f6-)" ]

# Add some nonunique people.
run_test cargo run call "$IDENT" insert_nonunique_person 23 Alice true
run_test cargo run call "$IDENT" insert_nonunique_person 42 Bob true

# Find a nonunique person who is there.
run_test cargo run call "$IDENT" find_nonunique_person 23
run_test cargo run logs "$IDENT" 100
[ ' NONUNIQUE FOUND: id 23: Alice' == "$(grep 'NONUNIQUE FOUND: id 23' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Fail to find a nonunique person who is not there.
run_test cargo run call "$IDENT" find_nonunique_person 43
run_test cargo run logs "$IDENT" 100
[ '' == "$(grep 'NONUNIQUE NOT FOUND: id 43' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Insert a non-human, then find humans, then find non-humans
run_test cargo run call "$IDENT" insert_nonunique_person 64 Jibbitty false
run_test cargo run call "$IDENT" find_nonunique_humans
run_test cargo run logs "$IDENT" 100
[ ' HUMAN FOUND: id 23: Alice' == "$(grep 'HUMAN FOUND: id 23' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' HUMAN FOUND: id 42: Bob' == "$(grep 'HUMAN FOUND: id 42' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
run_test cargo run call "$IDENT" find_nonunique_non_humans
run_test cargo run logs "$IDENT" 100
[ ' NON-HUMAN FOUND: id 64: Jibbitty' == "$(grep 'NON-HUMAN FOUND: id 64' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Add another person with the same id, and find them both.
run_test cargo run call "$IDENT" insert_nonunique_person 23 Claire true
run_test cargo run call "$IDENT" find_nonunique_person 23
run_test cargo run logs "$IDENT" 2
[ ' NONUNIQUE FOUND: id 23: Alice' == "$(grep 'NONUNIQUE FOUND: id 23: Alice' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
[ ' NONUNIQUE FOUND: id 23: Claire' == "$(grep 'NONUNIQUE FOUND: id 23: Claire' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Check for issues with things present in index but not DB
run_test cargo run call "$IDENT" insert_person 101 Fee fee
run_test cargo run call "$IDENT" insert_person 102 Fi "fi"
run_test cargo run call "$IDENT" insert_person 103 Fo fo
run_test cargo run call "$IDENT" insert_person 104 Fum fum
run_test cargo run call "$IDENT" delete_person 103
run_test cargo run call "$IDENT" find_person 104
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE FOUND: id 104: Fum' == "$(grep 'UNIQUE FOUND: id 104: Fum' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# As above, but for non-unique indices: check for consistency between index and DB
run_test cargo run call "$IDENT" insert_indexed_person 7 James Bond
run_test cargo run call "$IDENT" insert_indexed_person 79 Gold Bond
run_test cargo run call "$IDENT" insert_indexed_person 1 Hydrogen Bond
run_test cargo run call "$IDENT" insert_indexed_person 100 Whiskey Bond
run_test cargo run call "$IDENT" delete_indexed_person 100
run_test cargo run call "$IDENT" find_indexed_people Bond
run_test cargo run logs "$IDENT" 100
[ 1 == "$(grep -c 'INDEXED FOUND: id 7: Bond, James' "$TEST_OUT")" ]
[ 1 == "$(grep -c 'INDEXED FOUND: id 79: Bond, Gold' "$TEST_OUT")" ]
[ 1 == "$(grep -c 'INDEXED FOUND: id 1: Bond, Hydrogen' "$TEST_OUT")" ]
[ 0 == "$(grep -c 'INDEXED FOUND: id 100: Bond, Whiskey' "$TEST_OUT")" ]

# Non-unique version; does not work yet, see db_delete codegen in SpacetimeDB\crates\bindings-macro\src\lib.rs
# run_test cargo run call "$IDENT" insert_nonunique_person 101 Fee
# run_test cargo run call "$IDENT" insert_nonunique_person 102 "Fi"
# run_test cargo run call "$IDENT" insert_nonunique_person 103 Fo
# run_test cargo run call "$IDENT" insert_nonunique_person 104 Fum
# run_test cargo run call "$IDENT" find_nonunique_person 104
# run_test cargo run logs "$IDENT" 100
# [ ' NONUNIQUE FOUND: id 104: Fum' == "$(grep 'NONUNIQUE FOUND: id 104: Fum' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Filter by Identity
run_test cargo run call "$IDENT" insert_identified_person 23 Alice
run_test cargo run call "$IDENT" find_identified_person 23
run_test cargo run logs "$IDENT" 100
[ ' IDENTIFIED FOUND: Alice' == "$(grep 'IDENTIFIED FOUND: Alice' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]

# Insert row with unique columns twice should fail
run_test cargo run call "$IDENT" insert_person_twice 23 Alice al
run_test cargo run logs "$IDENT" 100
[ ' UNIQUE CONSTRAINT VIOLATION ERROR: id 23: Alice' == "$(grep 'UNIQUE CONSTRAINT VIOLATION ERROR: id 23: Alice' "$TEST_OUT" | tail -n 4 | cut -d: -f6-)" ]
