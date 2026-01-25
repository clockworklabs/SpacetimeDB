//! Reducer/procedure call tests translated from smoketests/tests/call.py

use spacetimedb_smoketests::Smoketest;

/// Check calling a reducer (no return) and procedure (return)
#[test]
fn test_call_reducer_procedure() {
    let test = Smoketest::builder()
        .precompiled_module("call-reducer-procedure")
        .build();

    // Reducer returns empty
    let msg = test.call("say_hello", &[]).unwrap();
    assert_eq!(msg.trim(), "");

    // Procedure returns a value
    let msg = test.call("return_person", &[]).unwrap();
    assert_eq!(msg.trim(), r#"["World"]"#);
}

/// Check calling a non-existent reducer/procedure raises error
#[test]
fn test_call_errors() {
    let test = Smoketest::builder()
        .precompiled_module("call-reducer-procedure")
        .build();

    let identity = test.database_identity.as_ref().unwrap();

    // Non-existent reducer
    let output = test.call_output("non_existent_reducer", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent_reducer` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_hello

Here are some existing procedures:
- return_person"
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );

    // Non-existent procedure
    let output = test.call_output("non_existent_procedure", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent_procedure` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_hello

Here are some existing procedures:
- return_person"
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );

    // Similar name to reducer - should suggest similar
    let output = test.call_output("say_hell", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `say_hell` for database `{identity}` resolving to identity `{identity}`.

A reducer with a similar name exists: `say_hello`"
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );

    // Similar name to procedure - should suggest similar
    let output = test.call_output("return_perso", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `return_perso` for database `{identity}` resolving to identity `{identity}`.

A procedure with a similar name exists: `return_person`"
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );
}

/// Check calling into a database with no reducers/procedures raises error
#[test]
fn test_call_empty_errors() {
    let test = Smoketest::builder().precompiled_module("call-empty").build();

    let identity = test.database_identity.as_ref().unwrap();

    let output = test.call_output("non_existent", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent` for database `{identity}` resolving to identity `{identity}`.

The database has no reducers.

The database has no procedures."
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );
}

/// Generate module code with many reducers and procedures
fn generate_many_module_code() -> String {
    let mut code = String::from(
        r#"
use spacetimedb::{log, ProcedureContext, ReducerContext};
"#,
    );

    for i in 0..11 {
        code.push_str(&format!(
            r#"
#[spacetimedb::reducer]
pub fn say_reducer_{i}(_ctx: &ReducerContext) {{
    log::info!("Hello from reducer {i}!");
}}

#[spacetimedb::procedure]
pub fn say_procedure_{i}(_ctx: &mut ProcedureContext) {{
    log::info!("Hello from procedure {i}!");
}}
"#
        ));
    }

    code
}

/// Check calling into a database with many reducers/procedures raises error with listing
#[test]
fn test_call_many_errors() {
    let module_code = generate_many_module_code();
    let test = Smoketest::builder().module_code(&module_code).build();

    let identity = test.database_identity.as_ref().unwrap();

    let output = test.call_output("non_existent", &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    let expected = format!(
        "WARNING: This command is UNSTABLE and subject to breaking changes.

Error: No such reducer OR procedure `non_existent` for database `{identity}` resolving to identity `{identity}`.

Here are some existing reducers:
- say_reducer_0
- say_reducer_1
- say_reducer_2
- say_reducer_3
- say_reducer_4
- say_reducer_5
- say_reducer_6
- say_reducer_7
- say_reducer_8
- say_reducer_9
... (1 reducer not shown)

Here are some existing procedures:
- say_procedure_0
- say_procedure_1
- say_procedure_2
- say_procedure_3
- say_procedure_4
- say_procedure_5
- say_procedure_6
- say_procedure_7
- say_procedure_8
- say_procedure_9
... (1 procedure not shown)"
    );
    assert!(
        expected.contains(stderr.trim()),
        "Expected stderr to be contained in expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );
}
