//! Auto-increment tests translated from smoketests/tests/auto_inc.py
//!
//! This is a simplified version that tests representative integer types
//! rather than all 10 types in the Python version.

use spacetimedb_smoketests::Smoketest;

/// Generate module code for basic auto-increment test with a specific integer type
fn autoinc_basic_module_code(int_ty: &str) -> String {
    format!(
        r#"
#![allow(non_camel_case_types)]
use spacetimedb::{{log, ReducerContext, Table}};

#[spacetimedb::table(name = person_{int_ty})]
pub struct Person_{int_ty} {{
    #[auto_inc]
    key_col: {int_ty},
    name: String,
}}

#[spacetimedb::reducer]
pub fn add_{int_ty}(ctx: &ReducerContext, name: String, expected_value: {int_ty}) {{
    let value = ctx.db.person_{int_ty}().insert(Person_{int_ty} {{ key_col: 0, name }});
    assert_eq!(value.key_col, expected_value);
}}

#[spacetimedb::reducer]
pub fn say_hello_{int_ty}(ctx: &ReducerContext) {{
    for person in ctx.db.person_{int_ty}().iter() {{
        log::info!("Hello, {{}}:{{}}!", person.key_col, person.name);
    }}
    log::info!("Hello, World!");
}}
"#
    )
}

fn do_test_autoinc_basic(int_ty: &str) {
    let module_code = autoinc_basic_module_code(int_ty);
    let test = Smoketest::builder().module_code(&module_code).build();

    test.call(&format!("add_{}", int_ty), &[r#""Robert""#, "1"]).unwrap();
    test.call(&format!("add_{}", int_ty), &[r#""Julie""#, "2"]).unwrap();
    test.call(&format!("add_{}", int_ty), &[r#""Samantha""#, "3"]).unwrap();
    test.call(&format!("say_hello_{}", int_ty), &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
        "Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
        "Expected 'Hello, 2:Julie!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
        "Expected 'Hello, 1:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_u32() {
    do_test_autoinc_basic("u32");
}

#[test]
fn test_autoinc_u64() {
    do_test_autoinc_basic("u64");
}

#[test]
fn test_autoinc_i32() {
    do_test_autoinc_basic("i32");
}

#[test]
fn test_autoinc_i64() {
    do_test_autoinc_basic("i64");
}

/// Generate module code for auto-increment with unique constraint test
fn autoinc_unique_module_code(int_ty: &str) -> String {
    format!(
        r#"
#![allow(non_camel_case_types)]
use std::error::Error;
use spacetimedb::{{log, ReducerContext, Table}};

#[spacetimedb::table(name = person_{int_ty})]
pub struct Person_{int_ty} {{
    #[auto_inc]
    #[unique]
    key_col: {int_ty},
    #[unique]
    name: String,
}}

#[spacetimedb::reducer]
pub fn add_new_{int_ty}(ctx: &ReducerContext, name: String) -> Result<(), Box<dyn Error>> {{
    let value = ctx.db.person_{int_ty}().try_insert(Person_{int_ty} {{ key_col: 0, name }})?;
    log::info!("Assigned Value: {{}} -> {{}}", value.key_col, value.name);
    Ok(())
}}

#[spacetimedb::reducer]
pub fn update_{int_ty}(ctx: &ReducerContext, name: String, new_id: {int_ty}) {{
    ctx.db.person_{int_ty}().name().delete(&name);
    let _value = ctx.db.person_{int_ty}().insert(Person_{int_ty} {{ key_col: new_id, name }});
}}

#[spacetimedb::reducer]
pub fn say_hello_{int_ty}(ctx: &ReducerContext) {{
    for person in ctx.db.person_{int_ty}().iter() {{
        log::info!("Hello, {{}}:{{}}!", person.key_col, person.name);
    }}
    log::info!("Hello, World!");
}}
"#
    )
}

fn do_test_autoinc_unique(int_ty: &str) {
    let module_code = autoinc_unique_module_code(int_ty);
    let test = Smoketest::builder().module_code(&module_code).build();

    // Insert Robert with explicit id 2
    test.call(&format!("update_{}", int_ty), &[r#""Robert""#, "2"]).unwrap();

    // Auto-inc should assign id 1 to Success
    test.call(&format!("add_new_{}", int_ty), &[r#""Success""#]).unwrap();

    // Auto-inc tries to assign id 2, but Robert already has it - should fail
    let result = test.call(&format!("add_new_{}", int_ty), &[r#""Failure""#]);
    assert!(
        result.is_err(),
        "Expected add_new to fail due to unique constraint violation"
    );

    test.call(&format!("say_hello_{}", int_ty), &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Robert!")),
        "Expected 'Hello, 2:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Success!")),
        "Expected 'Hello, 1:Success!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_unique_u64() {
    do_test_autoinc_unique("u64");
}

#[test]
fn test_autoinc_unique_i64() {
    do_test_autoinc_unique("i64");
}
