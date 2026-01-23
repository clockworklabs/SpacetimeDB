//! Tests translated from smoketests/tests/modules.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"#;

/// Breaking change: adds a new column to Person
const MODULE_CODE_BREAKING: &str = r#"
#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    age: u8,
}
"#;

/// Non-breaking change: adds a new table
const MODULE_CODE_ADD_TABLE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::table(name = pets)]
pub struct Pet {
    species: String,
}

#[spacetimedb::reducer]
pub fn are_we_updated_yet(ctx: &ReducerContext) {
    log::info!("MODULE UPDATED");
}
"#;

/// Test publishing a module without the --delete-data option
#[test]
fn test_module_update() {
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Initial publish
    test.publish_module_named(&name, false).unwrap();

    test.call("add", &["Robert"]).unwrap();
    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();
    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(logs.iter().any(|l| l.contains("Hello, Samantha!")));
    assert!(logs.iter().any(|l| l.contains("Hello, Julie!")));
    assert!(logs.iter().any(|l| l.contains("Hello, Robert!")));
    assert!(logs.iter().any(|l| l.contains("Hello, World!")));

    // Unchanged module is ok
    test.publish_module_named(&name, false).unwrap();

    // Changing an existing table isn't
    test.write_module_code(MODULE_CODE_BREAKING).unwrap();
    let result = test.publish_module_named(&name, false);
    assert!(result.is_err(), "Expected publish to fail with breaking change");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("manual migration") || err.contains("breaking"),
        "Expected migration error, got: {}",
        err
    );

    // Check that the old module is still running by calling say_hello
    test.call("say_hello", &[]).unwrap();

    // Adding a table is ok
    test.write_module_code(MODULE_CODE_ADD_TABLE).unwrap();
    test.publish_module_named(&name, false).unwrap();
    test.call("are_we_updated_yet", &[]).unwrap();

    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("MODULE UPDATED")),
        "Expected 'MODULE UPDATED' in logs, got: {:?}",
        logs
    );
}

/// Test uploading a basic module and calling some functions and checking logs
#[test]
fn test_upload_module() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    test.call("add", &["Robert"]).unwrap();
    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();
    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(logs.iter().any(|l| l.contains("Hello, Samantha!")));
    assert!(logs.iter().any(|l| l.contains("Hello, Julie!")));
    assert!(logs.iter().any(|l| l.contains("Hello, Robert!")));
    assert!(logs.iter().any(|l| l.contains("Hello, World!")));
}
