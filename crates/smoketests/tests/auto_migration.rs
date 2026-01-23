//! Tests translated from smoketests/tests/auto_migration.py

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
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_SIMPLE)
        .build();

    // Try to update with incompatible schema (adding column without default)
    test.write_module_code(MODULE_CODE_UPDATED_INCOMPATIBLE)
        .unwrap();
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
        logs.iter()
            .any(|l| l.contains("AFTER_PERSON: Samantha - Student")),
        "Expected Samantha in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|l| l.contains("AFTER_PERSON: Julie - Student")),
        "Expected Julie in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|l| l.contains("AFTER_PERSON: Robert - Student")),
        "Expected Robert in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|l| l.contains("AFTER_PERSON: Husserl - Professor")),
        "Expected Husserl Professor in AFTER logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("AFTER_BOOK: 1234567890")),
        "Expected book ISBN in AFTER logs: {:?}",
        logs
    );
}
