use spacetimedb_smoketests::{require_local_server, Smoketest};

const MODULE_CODE_SIMPLE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = person)]
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

#[spacetimedb::table(accessor = person)]
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

#[spacetimedb::table(accessor = person, public)]
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

#[spacetimedb::table(accessor = point_mass)]
pub struct PointMass {
    mass: f64,
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(accessor = person_info)]
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

#[spacetimedb::table(accessor = person, public)]
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

#[spacetimedb::table(accessor = point_mass)]
pub struct PointMass {
    mass: f64,
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(accessor = person_info)]
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

#[spacetimedb::table(accessor = book, public)]
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

    let sub = test.subscribe_background(&["select * from person"], 4).unwrap();

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

    let sub_updates = sub.collect().unwrap();
    assert_eq!(
        sub_updates.len(),
        4,
        "Expected 4 subscription updates, got {}: {:?}",
        sub_updates.len(),
        sub_updates
    );
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
#[spacetimedb::table(accessor = person)]
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
#[spacetimedb::table(accessor = person)]
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

    let mut test = Smoketest::builder().module_code(MODULE_CODE_SIMPLE).build();

    // Subscribe to person table changes multiple times to simulate active clients
    let mut subs = Vec::with_capacity(NUM_SUBSCRIBERS);
    for _ in 0..NUM_SUBSCRIBERS {
        // The migration below should disconnect all existing subscribers.
        subs.push(
            test.subscribe_background_until_closed(&["select * from person"])
                .unwrap(),
        );
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

    for sub in subs {
        // Ensure the background cli subprocess observes the disconnect and exits cleanly
        sub.collect().unwrap();
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

// --- Issue #3934: Removing a primary key breaks subsequent publishes ---

const MODULE_CODE_WITH_PK: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}
"#;

const MODULE_CODE_WITHOUT_PK: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}
"#;

const MODULE_CODE_WITHOUT_PK_V2: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn noop(_ctx: &ReducerContext) {}
"#;

/// Regression test for <https://github.com/clockworklabs/SpacetimeDB/issues/3934>.
///
/// Removing a `#[primary_key]` annotation and re-publishing succeeds,
/// but the stored schema retains the stale primary key. On the *next*
/// publish, `check_compatible` sees the mismatch and fails with:
///
///   "Primary key mismatch: self.primary_key: Some(ColId(0)), def.primary_key: None"
///
/// The fix adds a `ChangePrimaryKey` auto-migration step that updates
/// `table_primary_key` in `st_table`.
#[test]
fn test_remove_primary_key_issue_3934() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE_WITH_PK).build();

    // Step 1: Publish with primary key.
    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Step 2: Remove primary key. Should succeed.
    test.write_module_code(MODULE_CODE_WITHOUT_PK).unwrap();
    test.publish_module_with_options(&identity, false, true)
        .expect("Removing primary key should succeed");

    // Step 3: Trivial change (add a reducer). This is where #3934 crashes.
    test.write_module_code(MODULE_CODE_WITHOUT_PK_V2).unwrap();
    test.publish_module_with_options(&identity, false, true)
        .expect("Publish after PK removal should succeed (issue #3934)");
}

const MODULE_CODE_WITH_EVENT_TABLE_BEFORE: &str = r#"
use spacetimedb::{table, SpacetimeType};

#[derive(SpacetimeType)]
struct SomeProduct {
    a: u32,
    b: u64,
}

#[table(accessor = some_event, public, event)]
struct SomeEvent {
    foo: String,
    prod: SomeProduct,
}
"#;

const MODULE_CODE_WITH_EVENT_TABLE_AFTER: &str = r#"
use spacetimedb::{table, SpacetimeType};

#[derive(SpacetimeType)]
struct SomeProduct {
    a: u32,
    b: u64,
    c: u128,
}

#[table(accessor = some_event, public, event)]
struct SomeEvent {
    prod: SomeProduct,
}
"#;

#[test]
fn automigrate_reschema_event_table_arbitrarily() {
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_WITH_EVENT_TABLE_BEFORE)
        .build();

    // Step 1: publish with event table.
    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Step 2: Reschema event table. Should work fine, even though we'd reject this change for a non-event table.
    test.write_module_code(MODULE_CODE_WITH_EVENT_TABLE_AFTER).unwrap();
    test.publish_module_with_options(&identity, false, true)
        .expect("Changing schema of event table should succeed");

    // Step 3: Reschema event table right back. Should still work fine.
    test.write_module_code(MODULE_CODE_WITH_EVENT_TABLE_BEFORE).unwrap();
    test.publish_module_with_options(&identity, false, true)
        .expect("Changing schema of event table should succeed");
}

const MODULE_CODE_DROP_EVENT_TABLE_BEFORE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::table(accessor = some_event, public, event)]
pub struct SomeEvent {
    account_id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn emit_event(ctx: &ReducerContext) {
    ctx.db.some_event().insert(SomeEvent { account_id: 7, name: "alpha".to_string() });
}
"#;

const MODULE_CODE_DROP_EVENT_TABLE_AFTER: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}
"#;

/// Regression test: dropping an event table must not brick commitlog replay.
///
/// Dropping an event table deletes its `st_table`, `st_column` and `st_event_table` rows
/// in a single transaction. Replay applies deletes in ascending table id order,
/// so the `st_table` row is already gone when the `st_column` deletes are replayed,
/// while the `st_event_table` row is still present.
/// Replay therefore treated the dropped table as a live event table
/// and tried to refresh its layout, failing with
/// `Table with ID ... not found in st_table`
/// and permanently preventing the database from starting.
#[test]
fn automigrate_drop_event_table_replays_after_restart() {
    require_local_server!();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_DROP_EVENT_TABLE_BEFORE)
        .build();

    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Write some history, including an event row.
    test.call("add_person", &["Robert"]).unwrap();
    test.call("emit_event", &[]).unwrap();

    // Drop the event table.
    test.write_module_code(MODULE_CODE_DROP_EVENT_TABLE_AFTER).unwrap();
    test.publish_module_with_options(&identity, false, true)
        .expect("Dropping the event table should succeed");

    // Wait until data written after the drop is durable,
    // which implies the drop itself is durable too.
    test.call("add_person", &["Julie"]).unwrap();
    let output = test.sql_confirmed("SELECT * FROM person WHERE name = 'Julie'").unwrap();
    assert!(output.contains("Julie"), "Data not confirmed before restart: {output}");

    // Restarting forces a commitlog replay, which must replay the event table drop.
    test.restart_server();

    let output = test.sql("SELECT name FROM person").unwrap();
    assert!(output.contains("Robert"), "Expected 'Robert' after restart: {output}");
    assert!(output.contains("Julie"), "Expected 'Julie' after restart: {output}");

    // The database should still accept writes after replay.
    test.call("add_person", &["Samantha"]).unwrap();
    let output = test.sql("SELECT name FROM person WHERE name = 'Samantha'").unwrap();
    assert!(
        output.contains("Samantha"),
        "Expected 'Samantha' after restart: {output}"
    );
}
