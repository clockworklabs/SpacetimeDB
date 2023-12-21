from .. import Smoketest

class Filtering(Smoketest):
    MODULE_CODE = """
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
"""

    # TODO: split this into multiple test functions
    def test_filtering(self):
        """Test filtering reducers"""

        self.call("insert_person", 23, "Alice", "al")
        self.call("insert_person", 42, "Bob", "bo")
        self.call("insert_person", 64, "Bob", "b2")

        # Find a person who is there.
        self.call("find_person", 23)
        self.assertIn("UNIQUE FOUND: id 23: Alice", self.logs(2))

        # Find persons with the same name.
        self.call("find_person_by_name", "Bob")
        logs = self.logs(4)
        self.assertIn("UNIQUE FOUND: id 42: Bob aka bo", logs)
        self.assertIn("UNIQUE FOUND: id 64: Bob aka b2", logs)

        # Fail to find a person who is not there.
        self.call("find_person", 43)
        self.assertIn("UNIQUE NOT FOUND: id 43", self.logs(2))

        # Find a person by nickname.
        self.call("find_person_by_nick", "al")
        self.assertIn("UNIQUE FOUND: id 23: al", self.logs(2))

        # Remove a person, and then fail to find them.
        self.call("delete_person", 23)
        self.call("find_person", 23)
        self.assertIn("UNIQUE NOT FOUND: id 23", self.logs(2))
        # Also fail by nickname
        self.call("find_person_by_nick", "al")
        self.assertIn("UNIQUE NOT FOUND: nick al", self.logs(2))

        # Add some nonunique people.
        self.call("insert_nonunique_person", 23, "Alice", True)
        self.call("insert_nonunique_person", 42, "Bob", True)

        # Find a nonunique person who is there.
        self.call("find_nonunique_person", 23)
        # run_test cargo run logs "$IDENT" 100
        self.assertIn('NONUNIQUE FOUND: id 23: Alice', self.logs(2))

        # Fail to find a nonunique person who is not there.
        self.call("find_nonunique_person", 43)
        self.assertNotIn("NONUNIQUE NOT FOUND: id 43", self.logs(2))

        # Insert a non-human, then find humans, then find non-humans
        self.call("insert_nonunique_person", 64, "Jibbitty", False)
        self.call("find_nonunique_humans")
        self.assertIn('HUMAN FOUND: id 23: Alice', self.logs(2))
        self.assertIn('HUMAN FOUND: id 42: Bob', self.logs(2))
        self.call("find_nonunique_non_humans")
        self.assertIn('NON-HUMAN FOUND: id 64: Jibbitty', self.logs(2))

        # Add another person with the same id, and find them both.
        self.call("insert_nonunique_person", 23, "Claire", True)
        self.call("find_nonunique_person", 23)
        self.assertIn('NONUNIQUE FOUND: id 23: Alice', self.logs(2))
        self.assertIn('NONUNIQUE FOUND: id 23: Claire', self.logs(2))

        # Check for issues with things present in index but not DB
        self.call("insert_person", 101, "Fee", "fee")
        self.call("insert_person", 102, "Fi", "fi")
        self.call("insert_person", 103, "Fo", "fo")
        self.call("insert_person", 104, "Fum", "fum")
        self.call("delete_person", 103)
        self.call("find_person", 104)
        self.assertIn('UNIQUE FOUND: id 104: Fum', self.logs(2))

        # As above, but for non-unique indices: check for consistency between index and DB
        self.call("insert_indexed_person", 7, "James", "Bond")
        self.call("insert_indexed_person", 79, "Gold", "Bond")
        self.call("insert_indexed_person", 1, "Hydrogen", "Bond")
        self.call("insert_indexed_person", 100, "Whiskey", "Bond")
        self.call("delete_indexed_person", 100)
        self.call("find_indexed_people", "Bond")
        logs = self.logs(10)
        self.assertIn('INDEXED FOUND: id 7: Bond, James', logs)
        self.assertIn('INDEXED FOUND: id 79: Bond, Gold', logs)
        self.assertIn('INDEXED FOUND: id 1: Bond, Hydrogen', logs)
        self.assertNotIn('INDEXED FOUND: id 100: Bond, Whiskey', logs)

        # Non-unique version; does not work yet, see db_delete codegen in SpacetimeDB\crates\bindings-macro\src\lib.rs
        # self.call("insert_nonunique_person", 101, "Fee")
        # self.call("insert_nonunique_person", 102, "Fi")
        # self.call("insert_nonunique_person", 103, "Fo")
        # self.call("insert_nonunique_person", 104, "Fum")
        # self.call("find_nonunique_person", 104)
        # self.assertIn('NONUNIQUE FOUND: id 104: Fum', self.logs(2))

        # Filter by Identity
        self.call("insert_identified_person", 23, "Alice")
        self.call("find_identified_person", 23)
        self.assertIn('IDENTIFIED FOUND: Alice', self.logs(2))

        # Insert row with unique columns twice should fail
        self.call("insert_person_twice", 23, "Alice", "al")
        self.assertIn('UNIQUE CONSTRAINT VIOLATION ERROR: id 23: Alice', self.logs(2))
