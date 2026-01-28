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

/// Test deploying a module with a repeating reducer and checking it runs
#[test]
fn test_upload_module_2() {
    let test = Smoketest::builder().precompiled_module("upload-module-2").build();

    // Wait for the repeating reducer to run a few times
    std::thread::sleep(std::time::Duration::from_secs(2));
    let lines = test.logs(100).unwrap().iter().filter(|l| l.contains("Invoked")).count();

    // Wait more and check that count increased
    std::thread::sleep(std::time::Duration::from_secs(4));
    let new_lines = test.logs(100).unwrap().iter().filter(|l| l.contains("Invoked")).count();

    assert!(
        lines < new_lines,
        "Expected more invocations after waiting, got {} then {}",
        lines,
        new_lines
    );
}

/// Test hotswapping modules while a subscription is active
#[test]
fn test_hotswap_module() {
    let mut test = Smoketest::builder()
        .precompiled_module("hotswap-basic")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Publish initial module and subscribe to all
    test.publish_module_named(&name, false).unwrap();
    let sub = test.subscribe_background(&["SELECT * FROM *"], 2).unwrap();

    // Trigger event on the subscription
    test.call("add_person", &["Horst"]).unwrap();

    // Update the module (adds Pet table)
    test.use_precompiled_module("hotswap-updated");
    test.publish_module_named(&name, false).unwrap();

    // Assert that the module was updated
    test.call("add_pet", &["Turtle"]).unwrap();
    // And trigger another event on the subscription
    test.call("add_person", &["Cindy"]).unwrap();

    // Note that 'SELECT * FROM *' does NOT get refreshed to include the
    // new table (this is a known limitation).
    let updates = sub.collect().unwrap();

    // Check that we got updates for both person inserts
    assert_eq!(updates.len(), 2, "Expected 2 updates, got {:?}", updates);

    // First update should be Horst
    let first = &updates[0];
    assert!(
        first.get("person").is_some(),
        "Expected person table in first update: {:?}",
        first
    );
    let inserts = &first["person"]["inserts"];
    assert!(
        inserts.as_array().unwrap().iter().any(|r| r["name"] == "Horst"),
        "Expected Horst in first update: {:?}",
        first
    );

    // Second update should be Cindy
    let second = &updates[1];
    assert!(
        second.get("person").is_some(),
        "Expected person table in second update: {:?}",
        second
    );
    let inserts = &second["person"]["inserts"];
    assert!(
        inserts.as_array().unwrap().iter().any(|r| r["name"] == "Cindy"),
        "Expected Cindy in second update: {:?}",
        second
    );
}
