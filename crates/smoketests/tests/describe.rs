//! Module description tests translated from smoketests/tests/describe.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
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

/// Check describing a module
#[test]
fn test_describe() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    let identity = test.database_identity.as_ref().unwrap();

    // Describe the whole module
    test.spacetime(&["describe", "--json", identity]).unwrap();

    // Describe a specific reducer
    test.spacetime(&["describe", "--json", identity, "reducer", "say_hello"])
        .unwrap();

    // Describe a specific table
    test.spacetime(&["describe", "--json", identity, "table", "person"])
        .unwrap();
}
