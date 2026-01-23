//! Tests translated from smoketests/tests/new_user_flow.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"#;

/// Test the entirety of the new user flow.
#[test]
fn test_new_user_flow() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Create a new identity and publish
    test.new_identity().unwrap();
    test.publish_module().unwrap();

    // Calling our database
    test.call("say_hello", &[]).unwrap();
    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs: {:?}",
        logs
    );

    // Calling functions with arguments
    test.call("add", &["Tyler"]).unwrap();
    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(5).unwrap();
    let hello_world_count = logs.iter().filter(|l| l.contains("Hello, World!")).count();
    let hello_tyler_count = logs.iter().filter(|l| l.contains("Hello, Tyler!")).count();

    assert_eq!(hello_world_count, 2, "Expected 2 'Hello, World!' in logs");
    assert_eq!(hello_tyler_count, 1, "Expected 1 'Hello, Tyler!' in logs");

    // Query via SQL
    test.assert_sql(
        "SELECT * FROM person",
        r#" name
---------
 "Tyler""#,
    );
}
