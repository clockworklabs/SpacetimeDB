//! Tests translated from smoketests/tests/modules.py

use spacetimedb_smoketests::Smoketest;

/// Test publishing a module without the --delete-data option
#[test]
fn test_module_update() {
    let mut test = Smoketest::builder()
        .precompiled_module("modules-basic")
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

    // Changing an existing table isn't (adds age column to Person)
    test.use_precompiled_module("modules-breaking");
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
    test.use_precompiled_module("modules-add-table");
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
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

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
