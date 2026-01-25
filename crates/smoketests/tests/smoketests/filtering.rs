//! Filtering tests translated from smoketests/tests/filtering.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
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
"#;

/// Test filtering reducers
#[test]
fn test_filtering() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    test.call("insert_person", &["23", r#""Alice""#, r#""al""#]).unwrap();
    test.call("insert_person", &["42", r#""Bob""#, r#""bo""#]).unwrap();
    test.call("insert_person", &["64", r#""Bob""#, r#""b2""#]).unwrap();

    // Find a person who is there.
    test.call("find_person", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 23: Alice")),
        "Expected 'UNIQUE FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );

    // Find persons with the same name.
    test.call("find_person_by_name", &[r#""Bob""#]).unwrap();
    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 42: Bob aka bo")),
        "Expected 'UNIQUE FOUND: id 42: Bob aka bo' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 64: Bob aka b2")),
        "Expected 'UNIQUE FOUND: id 64: Bob aka b2' in logs, got: {:?}",
        logs
    );

    // Fail to find a person who is not there.
    test.call("find_person", &["43"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: id 43")),
        "Expected 'UNIQUE NOT FOUND: id 43' in logs, got: {:?}",
        logs
    );
    test.call("find_person_read_only", &["43"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: id 43")),
        "Expected 'UNIQUE NOT FOUND: id 43' in logs, got: {:?}",
        logs
    );

    // Find a person by nickname.
    test.call("find_person_by_nick", &[r#""al""#]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 23: al")),
        "Expected 'UNIQUE FOUND: id 23: al' in logs, got: {:?}",
        logs
    );
    test.call("find_person_by_nick_read_only", &[r#""al""#]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 23: al")),
        "Expected 'UNIQUE FOUND: id 23: al' in logs, got: {:?}",
        logs
    );

    // Remove a person, and then fail to find them.
    test.call("delete_person", &["23"]).unwrap();
    test.call("find_person", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: id 23")),
        "Expected 'UNIQUE NOT FOUND: id 23' in logs, got: {:?}",
        logs
    );
    test.call("find_person_read_only", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: id 23")),
        "Expected 'UNIQUE NOT FOUND: id 23' in logs, got: {:?}",
        logs
    );
    // Also fail by nickname
    test.call("find_person_by_nick", &[r#""al""#]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: nick al")),
        "Expected 'UNIQUE NOT FOUND: nick al' in logs, got: {:?}",
        logs
    );
    test.call("find_person_by_nick_read_only", &[r#""al""#]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE NOT FOUND: nick al")),
        "Expected 'UNIQUE NOT FOUND: nick al' in logs, got: {:?}",
        logs
    );

    // Add some nonunique people.
    test.call("insert_nonunique_person", &["23", r#""Alice""#, "true"])
        .unwrap();
    test.call("insert_nonunique_person", &["42", r#""Bob""#, "true"])
        .unwrap();

    // Find a nonunique person who is there.
    test.call("find_nonunique_person", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Alice")),
        "Expected 'NONUNIQUE FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );
    test.call("find_nonunique_person_read_only", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Alice")),
        "Expected 'NONUNIQUE FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );

    // Fail to find a nonunique person who is not there.
    test.call("find_nonunique_person", &["43"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        !logs.iter().any(|msg| msg.contains("NONUNIQUE NOT FOUND: id 43")),
        "Expected no 'NONUNIQUE NOT FOUND: id 43' in logs, got: {:?}",
        logs
    );
    test.call("find_nonunique_person_read_only", &["43"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        !logs.iter().any(|msg| msg.contains("NONUNIQUE NOT FOUND: id 43")),
        "Expected no 'NONUNIQUE NOT FOUND: id 43' in logs, got: {:?}",
        logs
    );

    // Insert a non-human, then find humans, then find non-humans
    test.call("insert_nonunique_person", &["64", r#""Jibbitty""#, "false"])
        .unwrap();
    test.call("find_nonunique_humans", &[]).unwrap();
    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("HUMAN FOUND: id 23: Alice")),
        "Expected 'HUMAN FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("HUMAN FOUND: id 42: Bob")),
        "Expected 'HUMAN FOUND: id 42: Bob' in logs, got: {:?}",
        logs
    );
    test.call("find_nonunique_non_humans", &[]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("NON-HUMAN FOUND: id 64: Jibbitty")),
        "Expected 'NON-HUMAN FOUND: id 64: Jibbitty' in logs, got: {:?}",
        logs
    );

    // Add another person with the same id, and find them both.
    test.call("insert_nonunique_person", &["23", r#""Claire""#, "true"])
        .unwrap();
    test.call("find_nonunique_person", &["23"]).unwrap();
    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Alice")),
        "Expected 'NONUNIQUE FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Claire")),
        "Expected 'NONUNIQUE FOUND: id 23: Claire' in logs, got: {:?}",
        logs
    );
    test.call("find_nonunique_person_read_only", &["23"]).unwrap();
    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Alice")),
        "Expected 'NONUNIQUE FOUND: id 23: Alice' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("NONUNIQUE FOUND: id 23: Claire")),
        "Expected 'NONUNIQUE FOUND: id 23: Claire' in logs, got: {:?}",
        logs
    );

    // Check for issues with things present in index but not DB
    test.call("insert_person", &["101", r#""Fee""#, r#""fee""#]).unwrap();
    test.call("insert_person", &["102", r#""Fi""#, r#""fi""#]).unwrap();
    test.call("insert_person", &["103", r#""Fo""#, r#""fo""#]).unwrap();
    test.call("insert_person", &["104", r#""Fum""#, r#""fum""#]).unwrap();
    test.call("delete_person", &["103"]).unwrap();
    test.call("find_person", &["104"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 104: Fum")),
        "Expected 'UNIQUE FOUND: id 104: Fum' in logs, got: {:?}",
        logs
    );
    test.call("find_person_read_only", &["104"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("UNIQUE FOUND: id 104: Fum")),
        "Expected 'UNIQUE FOUND: id 104: Fum' in logs, got: {:?}",
        logs
    );

    // As above, but for non-unique indices: check for consistency between index and DB
    test.call("insert_indexed_person", &["7", r#""James""#, r#""Bond""#])
        .unwrap();
    test.call("insert_indexed_person", &["79", r#""Gold""#, r#""Bond""#])
        .unwrap();
    test.call("insert_indexed_person", &["1", r#""Hydrogen""#, r#""Bond""#])
        .unwrap();
    test.call("insert_indexed_person", &["100", r#""Whiskey""#, r#""Bond""#])
        .unwrap();
    test.call("delete_indexed_person", &["100"]).unwrap();
    test.call("find_indexed_people", &[r#""Bond""#]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("INDEXED FOUND: id 7: Bond, James")),
        "Expected 'INDEXED FOUND: id 7: Bond, James' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("INDEXED FOUND: id 79: Bond, Gold")),
        "Expected 'INDEXED FOUND: id 79: Bond, Gold' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|msg| msg.contains("INDEXED FOUND: id 1: Bond, Hydrogen")),
        "Expected 'INDEXED FOUND: id 1: Bond, Hydrogen' in logs, got: {:?}",
        logs
    );
    assert!(
        !logs
            .iter()
            .any(|msg| msg.contains("INDEXED FOUND: id 100: Bond, Whiskey")),
        "Expected no 'INDEXED FOUND: id 100: Bond, Whiskey' in logs, got: {:?}",
        logs
    );
    test.call("find_indexed_people_read_only", &[r#""Bond""#]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("INDEXED FOUND: id 7: Bond, James")),
        "Expected 'INDEXED FOUND: id 7: Bond, James' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("INDEXED FOUND: id 79: Bond, Gold")),
        "Expected 'INDEXED FOUND: id 79: Bond, Gold' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|msg| msg.contains("INDEXED FOUND: id 1: Bond, Hydrogen")),
        "Expected 'INDEXED FOUND: id 1: Bond, Hydrogen' in logs, got: {:?}",
        logs
    );
    assert!(
        !logs
            .iter()
            .any(|msg| msg.contains("INDEXED FOUND: id 100: Bond, Whiskey")),
        "Expected no 'INDEXED FOUND: id 100: Bond, Whiskey' in logs, got: {:?}",
        logs
    );

    // Filter by Identity
    test.call("insert_identified_person", &["23", r#""Alice""#]).unwrap();
    test.call("find_identified_person", &["23"]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("IDENTIFIED FOUND: Alice")),
        "Expected 'IDENTIFIED FOUND: Alice' in logs, got: {:?}",
        logs
    );

    // Inserting into a table with unique constraints fails
    // when the second row has the same value in the constrained columns as the first row.
    // In this case, the table has `#[unique] id` and `#[unique] nick` but not `#[unique] name`.
    test.call("insert_person_twice", &["23", r#""Alice""#, r#""al""#])
        .unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter()
            .any(|msg| msg.contains("UNIQUE CONSTRAINT VIOLATION ERROR: id = 23, nick = al")),
        "Expected 'UNIQUE CONSTRAINT VIOLATION ERROR: id = 23, nick = al' in logs, got: {:?}",
        logs
    );
}
