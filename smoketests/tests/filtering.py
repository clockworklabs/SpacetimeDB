from .. import Smoketest

class Filtering(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, Identity, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[unique]
    id: i32,

    name: String,

    #[unique]
    nick: String,
}

#[spacetimedb::reducer]
pub fn insert_person(ctx: &ReducerContext, id: i32, name: String, nick: String) {
    ctx.db.person().insert(Person { id, name, nick} );
}

#[spacetimedb::reducer]
pub fn insert_person_twice(ctx: &ReducerContext, id: i32, name: String, nick: String) {
    // We'd like to avoid an error due to a set-semantic error.
    let name2 = format!("{name}2");
    ctx.db.person().insert(Person { id, name, nick: nick.clone()} );
    match ctx.db.person().try_insert(Person { id, name: name2, nick: nick.clone()}) {
        Ok(_) => {},
        Err(_) => {
            log::info!("UNIQUE CONSTRAINT VIOLATION ERROR: id = {}, nick = {}", id, nick)
        }
    }
}

#[spacetimedb::reducer]
pub fn delete_person(ctx: &ReducerContext, id: i32) {
    ctx.db.person().id().delete(&id);
}

#[spacetimedb::reducer]
pub fn find_person(ctx: &ReducerContext, id: i32) {
    match ctx.db.person().id().find(&id) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", id, person.name),
        None => log::info!("UNIQUE NOT FOUND: id {}", id),
    }
}

#[spacetimedb::reducer]
pub fn find_person_read_only(ctx: &ReducerContext, id: i32) {
    let ctx = ctx.as_read_only();
    match ctx.db.person().id().find(&id) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", id, person.name),
        None => log::info!("UNIQUE NOT FOUND: id {}", id),
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_name(ctx: &ReducerContext, name: String) {
    for person in ctx.db.person().iter().filter(|p| p.name == name) {
        log::info!("UNIQUE FOUND: id {}: {} aka {}", person.id, person.name, person.nick);
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_nick(ctx: &ReducerContext, nick: String) {
    match ctx.db.person().nick().find(&nick) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", person.id, person.nick),
        None => log::info!("UNIQUE NOT FOUND: nick {}", nick),
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_nick_read_only(ctx: &ReducerContext, nick: String) {
    let ctx = ctx.as_read_only();
    match ctx.db.person().nick().find(&nick) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", person.id, person.nick),
        None => log::info!("UNIQUE NOT FOUND: nick {}", nick),
    }
}

#[spacetimedb::table(name = nonunique_person)]
pub struct NonuniquePerson {
    #[index(btree)]
    id: i32,
    name: String,
    is_human: bool,
}

#[spacetimedb::reducer]
pub fn insert_nonunique_person(ctx: &ReducerContext, id: i32, name: String, is_human: bool) {
    ctx.db.nonunique_person().insert(NonuniquePerson { id, name, is_human } );
}

#[spacetimedb::reducer]
pub fn find_nonunique_person(ctx: &ReducerContext, id: i32) {
    for person in ctx.db.nonunique_person().id().filter(&id) {
        log::info!("NONUNIQUE FOUND: id {}: {}", id, person.name)
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_person_read_only(ctx: &ReducerContext, id: i32) {
    let ctx = ctx.as_read_only();
    for person in ctx.db.nonunique_person().id().filter(&id) {
        log::info!("NONUNIQUE FOUND: id {}: {}", id, person.name)
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_humans(ctx: &ReducerContext) {
    for person in ctx.db.nonunique_person().iter().filter(|p| p.is_human) {
        log::info!("HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_non_humans(ctx: &ReducerContext) {
    for person in ctx.db.nonunique_person().iter().filter(|p| !p.is_human) {
        log::info!("NON-HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

// Ensure that [Identity] is filterable and a legal unique column.
#[spacetimedb::table(name = identified_person)]
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

#[spacetimedb::reducer]
fn insert_identified_person(ctx: &ReducerContext, id_number: u64, name: String) {
    let identity = identify(id_number);
    ctx.db.identified_person().insert(IdentifiedPerson { identity, name });
}

#[spacetimedb::reducer]
fn find_identified_person(ctx: &ReducerContext, id_number: u64) {
    let identity = identify(id_number);
    match ctx.db.identified_person().identity().find(&identity) {
        Some(person) => log::info!("IDENTIFIED FOUND: {}", person.name),
        None => log::info!("IDENTIFIED NOT FOUND"),
    }
}

// Ensure that indices on non-unique columns behave as we expect.
#[spacetimedb::table(name = indexed_person)]
struct IndexedPerson {
    #[unique]
    id: i32,
    given_name: String,
    #[index(btree)]
    surname: String,
}

#[spacetimedb::reducer]
fn insert_indexed_person(ctx: &ReducerContext, id: i32, given_name: String, surname: String) {
    ctx.db.indexed_person().insert(IndexedPerson { id, given_name, surname });
}

#[spacetimedb::reducer]
fn delete_indexed_person(ctx: &ReducerContext, id: i32) {
    ctx.db.indexed_person().id().delete(&id);
}

#[spacetimedb::reducer]
fn find_indexed_people(ctx: &ReducerContext, surname: String) {
    for person in ctx.db.indexed_person().surname().filter(&surname) {
        log::info!("INDEXED FOUND: id {}: {}, {}", person.id, person.surname, person.given_name);
    }
}

#[spacetimedb::reducer]
fn find_indexed_people_read_only(ctx: &ReducerContext, surname: String) {
    let ctx = ctx.as_read_only();
    for person in ctx.db.indexed_person().surname().filter(&surname) {
        log::info!("INDEXED FOUND: id {}: {}, {}", person.id, person.surname, person.given_name);
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
        self.call("find_person_read_only", 43)
        self.assertIn("UNIQUE NOT FOUND: id 43", self.logs(2))

        # Find a person by nickname.
        self.call("find_person_by_nick", "al")
        self.assertIn("UNIQUE FOUND: id 23: al", self.logs(2))
        self.call("find_person_by_nick_read_only", "al")
        self.assertIn("UNIQUE FOUND: id 23: al", self.logs(2))

        # Remove a person, and then fail to find them.
        self.call("delete_person", 23)
        self.call("find_person", 23)
        self.assertIn("UNIQUE NOT FOUND: id 23", self.logs(2))
        self.call("find_person_read_only", 23)
        self.assertIn("UNIQUE NOT FOUND: id 23", self.logs(2))
        # Also fail by nickname
        self.call("find_person_by_nick", "al")
        self.assertIn("UNIQUE NOT FOUND: nick al", self.logs(2))
        self.call("find_person_by_nick_read_only", "al")
        self.assertIn("UNIQUE NOT FOUND: nick al", self.logs(2))

        # Add some nonunique people.
        self.call("insert_nonunique_person", 23, "Alice", True)
        self.call("insert_nonunique_person", 42, "Bob", True)

        # Find a nonunique person who is there.
        self.call("find_nonunique_person", 23)
        self.assertIn('NONUNIQUE FOUND: id 23: Alice', self.logs(2))
        self.call("find_nonunique_person_read_only", 23)
        self.assertIn('NONUNIQUE FOUND: id 23: Alice', self.logs(2))

        # Fail to find a nonunique person who is not there.
        self.call("find_nonunique_person", 43)
        self.assertNotIn("NONUNIQUE NOT FOUND: id 43", self.logs(2))
        self.call("find_nonunique_person_read_only", 43)
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
        self.call("find_nonunique_person_read_only", 23)
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
        self.call("find_person_read_only", 104)
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
        self.call("find_indexed_people_read_only", "Bond")
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

        # Inserting into a table with unique constraints fails
        # when the second row has the same value in the constrained columns as the first row.
        # In this case, the table has `#[unique] id` and `#[unique] nick` but not `#[unique] name`.
        self.call("insert_person_twice", 23, "Alice", "al")
        self.assertIn('UNIQUE CONSTRAINT VIOLATION ERROR: id = 23, nick = al', self.logs(2))
