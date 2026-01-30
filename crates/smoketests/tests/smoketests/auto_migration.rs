use spacetimedb_smoketests::Smoketest;

const MODULE_CODE_SIMPLE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {}", prefix, person.name);
    }
}
"#;

const MODULE_CODE_UPDATED_INCOMPATIBLE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
    age: u128,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {}", prefix, person.name);
    }
}
"#;

/// Tests that a module with invalid schema changes cannot be published without -c or a migration.
#[test]
fn test_reject_schema_changes() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE_SIMPLE).build();

    // Try to update with incompatible schema (adding column without default)
    test.write_module_code(MODULE_CODE_UPDATED_INCOMPATIBLE).unwrap();
    let result = test.publish_module_clear(false);

    assert!(
        result.is_err(),
        "Expected publish to fail with incompatible schema change"
    );
}

const MODULE_CODE_INIT: &str = r#"
use spacetimedb::{log, ReducerContext, Table, SpacetimeType};
use PersonKind::*;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
    kind: PersonKind,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String, kind: String) {
    let kind = kind_from_string(kind);
    ctx.db.person().insert(Person { name, kind });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        let kind = kind_to_string(person.kind);
        log::info!("{prefix}: {} - {kind}", person.name);
    }
}

#[spacetimedb::table(name = point_mass)]
pub struct PointMass {
    mass: f64,
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(name = person_info)]
pub struct PersonInfo {
    #[primary_key]
    id: u64,
}

#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Student,
}

fn kind_from_string(_: String) -> PersonKind {
    Student
}

fn kind_to_string(Student: PersonKind) -> &'static str {
    "Student"
}
"#;

const MODULE_CODE_UPDATED: &str = r#"
use spacetimedb::{log, ReducerContext, Table, SpacetimeType};
use PersonKind::*;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
    kind: PersonKind,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String, kind: String) {
    let kind = kind_from_string(kind);
    ctx.db.person().insert(Person { name, kind });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        let kind = kind_to_string(person.kind);
        log::info!("{prefix}: {} - {kind}", person.name);
    }
}

#[spacetimedb::table(name = point_mass)]
pub struct PointMass {
    mass: f64,
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(name = person_info)]
pub struct PersonInfo {
    #[primary_key]
    #[auto_inc]
    id: u64,
}

#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Student,
    Professor,
}

fn kind_from_string(kind: String) -> PersonKind {
    match &*kind {
        "Student" => Student,
        "Professor" => Professor,
        _ => panic!(),
    }
}

fn kind_to_string(kind: PersonKind) -> &'static str {
    match kind {
        Student => "Student",
        Professor => "Professor",
    }
}

#[spacetimedb::table(name = book, public)]
pub struct Book {
    isbn: String,
}

#[spacetimedb::reducer]
pub fn add_book(ctx: &ReducerContext, isbn: String) {
    ctx.db.book().insert(Book { isbn });
}

#[spacetimedb::reducer]
pub fn print_books(ctx: &ReducerContext, prefix: String) {
    for book in ctx.db.book().iter() {
        log::info!("{}: {}", prefix, book.isbn);
    }
}
"#;

/// Tests uploading a module with a schema change that should not require clearing the database.
#[test]
fn test_add_table_auto_migration() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE_INIT).build();

    // Add initial data
    test.call("add_person", &["Robert", "Student"]).unwrap();
    test.call("add_person", &["Julie", "Student"]).unwrap();
    test.call("add_person", &["Samantha", "Student"]).unwrap();
    test.call("print_persons", &["BEFORE"]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("BEFORE: Samantha - Student")),
        "Expected Samantha in logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("BEFORE: Julie - Student")),
        "Expected Julie in logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("BEFORE: Robert - Student")),
        "Expected Robert in logs: {:?}",
        logs
    );

    // Update module without clearing database
    test.write_module_code(MODULE_CODE_UPDATED).unwrap();
    test.publish_module_clear(false).unwrap();

    // Add new data with updated schema
    test.call("add_person", &["Husserl", "Student"]).unwrap();
    test.call("add_person", &["Husserl", "Professor"]).unwrap();
    test.call("add_book", &["1234567890"]).unwrap();
    test.call("print_persons", &["AFTER_PERSON"]).unwrap();
    test.call("print_books", &["AFTER_BOOK"]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("AFTER_PERSON: Samantha - Student")),
        "Expected Samantha in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("AFTER_PERSON: Julie - Student")),
        "Expected Julie in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("AFTER_PERSON: Robert - Student")),
        "Expected Robert in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("AFTER_PERSON: Husserl - Professor")),
        "Expected Husserl Professor in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("AFTER_BOOK: 1234567890")),
        "Expected book ISBN in AFTER logs: {:?}",
        logs
    );
}

const MODULE_CODE_ADD_TABLE_COLUMNS_UPDATED: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(name = person)]
pub struct Person {
    #[index(btree)]
    name: String,
    #[default(0)]
    age: u16,
    #[default(19)]
    mass: u16,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70, mass: 180 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    log::info!("FIRST_UPDATE: client disconnected");
}
"#;

const MODULE_CODE_ADD_TABLE_COLUMNS_UPDATED_AGAIN: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
    age: u16,
    #[default(19)]
    mass: u16,
    #[default(160)]
    height: u32,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70, mass: 180, height: 72 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}
"#;

/// Verify schema upgrades that add columns with defaults (twice).
#[test]
fn test_add_table_columns() {
    const NUM_SUBSCRIBERS: usize = 20;

    let mut test = Smoketest::builder().module_code(MODULE_CODE_BASIC).build();

    // Subscribe to person table changes multiple times to simulate active clients
    let mut subs = Vec::with_capacity(NUM_SUBSCRIBERS);
    for _ in 0..NUM_SUBSCRIBERS {
        subs.push(test.subscribe_background(&["select * from person"], 5).unwrap());
    }

    // Insert under initial schema
    test.call("add_person", &["Robert"]).unwrap();

    // First upgrade: add age & mass columns
    test.write_module_code(MODULE_CODE_ADD_TABLE_COLUMNS_UPDATED).unwrap();
    let identity = test.database_identity.clone().unwrap();
    test.publish_module_with_options(&identity, false, true).unwrap();
    test.call("print_persons", &["FIRST_UPDATE"]).unwrap();

    let logs1 = test.logs(100).unwrap();
    assert!(
        logs1.iter().any(|l| l.contains("Disconnecting all users")),
        "Expected disconnect log in logs: {:?}",
        logs1
    );
    assert!(
        logs1
            .iter()
            .any(|l| l.contains("FIRST_UPDATE: Person { name: \"Robert\", age: 0, mass: 19 }")),
        "Expected migrated person with defaults in logs: {:?}",
        logs1
    );

    let disconnect_count = logs1
        .iter()
        .filter(|l| l.contains("FIRST_UPDATE: client disconnected"))
        .count();
    assert_eq!(
        disconnect_count,
        NUM_SUBSCRIBERS + 1,
        "Unexpected disconnect counts: {disconnect_count}"
    );

    // Insert new data under upgraded schema
    test.call("add_person", &["Robert2"]).unwrap();

    // Validate all subscribers were disconnected after first upgrade
    for (i, sub) in subs.into_iter().enumerate() {
        let rows = sub.collect().unwrap();
        assert_eq!(rows.len(), 2, "Subscriber {i} received unexpected rows: {rows:?}");
    }

    // Second upgrade
    test.write_module_code(MODULE_CODE_ADD_TABLE_COLUMNS_UPDATED_AGAIN)
        .unwrap();
    test.publish_module_with_options(&identity, false, true).unwrap();
    test.call("print_persons", &["UPDATE_2"]).unwrap();

    let logs2 = test.logs(100).unwrap();
    assert!(
        logs2
            .iter()
            .any(|l| { l.contains("UPDATE_2: Person { name: \"Robert2\", age: 70, mass: 180, height: 160 }") }),
        "Expected updated schema with default height in logs: {:?}",
        logs2
    );
}
