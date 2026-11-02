use spacetimedb_smoketests::Smoketest;

#[test]
fn test_named_json_call_accepts_exported_canonical_parameter_names() {
    use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10;
    use spacetimedb_lib::sats::serde::SerdeWrapper;
    use spacetimedb_lib::AlgebraicType;

    let test = Smoketest::builder()
        .module_code(
            r#"#![allow(non_snake_case)]
use spacetimedb::ReducerContext;

#[derive(spacetimedb::SpacetimeType)]
pub struct Message {
    Item: u64,
}

#[spacetimedb::reducer]
pub fn receive(_ctx: &ReducerContext, MessageBody: Message) {
    spacetimedb::log::info!("json-received:{}", MessageBody.Item);
}
"#,
        )
        .build();
    let identity = test.database_identity.as_ref().unwrap();
    let schema = test
        .api_call("GET", &format!("/v1/database/{identity}/schema?version=10"))
        .unwrap();
    assert!(schema.is_success(), "schema request failed: {}", schema.text().unwrap());
    let SerdeWrapper(raw): SerdeWrapper<RawModuleDefV10> = serde_json::from_slice(&schema.body).unwrap();
    let reducer = raw
        .reducers()
        .unwrap()
        .iter()
        .find(|reducer| reducer.source_name.as_ref() == "receive")
        .unwrap();
    let param = &reducer.params.elements[0];
    let parameter_name = param.name.as_deref().unwrap();
    assert_eq!(parameter_name, "message_body");
    let AlgebraicType::Ref(message_ref) = param.algebraic_type else {
        panic!("expected a named Message type");
    };
    let item_name = raw.typespace().unwrap()[message_ref].as_product().unwrap().elements[0]
        .name
        .as_deref()
        .unwrap();
    assert_eq!(item_name, "item");

    let path = format!("/v1/database/{identity}/call/receive");
    // Existing source-name requests and positional JSON both work, including a
    // nested object whose field name comes from the exported canonical typespace.
    for args in [
        serde_json::json!({ "MessageBody": { (item_name): 1 } }),
        serde_json::json!([{ (item_name): 2 }]),
    ] {
        let response = test.api_call_json("POST", &path, &args.to_string()).unwrap();
        assert!(
            response.is_success(),
            "control call {args} failed: {}",
            response.text().unwrap()
        );
    }

    // A schema-driven caller uses the advertised parameter name, not the source name.
    let args = serde_json::json!({ (parameter_name): { (item_name): 3 } });
    let response = test.api_call_json("POST", &path, &args.to_string()).unwrap();
    assert!(
        response.is_success(),
        "named JSON arguments built from the exported schema should be accepted: HTTP {}, {}",
        response.status_code,
        response.text().unwrap()
    );
    assert!(test
        .logs(100)
        .unwrap()
        .iter()
        .any(|line| line.contains("json-received:3")));
}

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

/// Tests that both reducers and procedures that take a [spacetimedb::Identity]
/// argument can be called using any of the various encodings for that type.
#[test]
fn test_call_reducer_procedure_with_identity_argument() {
    let test = Smoketest::builder()
        .precompiled_module("call-reducer-procedure-identity-arg")
        .build();

    let identity = test.database_identity.as_ref().unwrap();
    let identity_tuple_encoding = format!("[\"0x{identity}\"]");

    let identity_encodings = [
        identity,
        &format!("0x{identity}"),
        &identity_tuple_encoding,
        &format!("{{\"__identity__\": \"0x{identity}\"}}"),
    ];

    for argument in identity_encodings {
        let msg = test.call("return_my_identity", &[argument]).unwrap();
        assert_eq!(msg.trim(), &identity_tuple_encoding);

        test.call("say_my_identity", &[argument]).unwrap();
    }

    let logs = test.logs(100).unwrap();
    let log_string = format!("Hello, {identity}!");
    assert_eq!(logs.iter().filter(|l| l.contains(&log_string)).count(), 4);
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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );
}

/// Check calling into a database with many reducers/procedures raises error with listing
#[test]
fn test_call_many_errors() {
    let test = Smoketest::builder().precompiled_module("call-many").build();

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
        stderr.trim().starts_with(&expected),
        "Expected stderr to start with expected message.\nExpected:\n{}\n\nActual stderr:\n{}",
        expected,
        stderr.trim()
    );
}
