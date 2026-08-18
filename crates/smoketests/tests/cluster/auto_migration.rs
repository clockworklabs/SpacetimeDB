use spacetimedb_smoketests::Smoketest;

/// Tests that a module with invalid schema changes cannot be published without -c or a migration.
#[test]
fn test_reject_schema_changes() {
    let mut test = Smoketest::builder().precompiled_module("auto-migration-simple").build();

    // Try to update with incompatible schema (adding column without default)
    test.use_precompiled_module("auto-migration-incompatible");
    let result = test.publish().current_database().unwrap().run();

    assert!(
        result.is_err(),
        "Expected publish to fail with incompatible schema change"
    );
}

/// Tests uploading a module with a schema change that should not require clearing the database.
#[test]
fn test_add_table_auto_migration() {
    let mut test = Smoketest::builder()
        .precompiled_module("auto-migration-add-table-initial")
        .build();

    let sub = test
        .subscribe(&["select * from person"])
        .expect_rows(4)
        .background()
        .unwrap();

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
    test.use_precompiled_module("auto-migration-add-table-updated");
    test.publish().current_database().unwrap().run().unwrap();

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

/// Verify schema upgrades that add columns with defaults (twice).
#[test]
fn test_add_table_columns() {
    const NUM_SUBSCRIBERS: usize = 20;

    let mut test = Smoketest::builder().precompiled_module("auto-migration-simple").build();

    // Subscribe to person table changes multiple times to simulate active clients
    let mut subs = Vec::with_capacity(NUM_SUBSCRIBERS);
    for _ in 0..NUM_SUBSCRIBERS {
        // The migration below should disconnect all existing subscribers.
        subs.push(test.subscribe(&["select * from person"]).background().unwrap());
    }

    // Insert under initial schema
    test.call("add_person", &["Robert"]).unwrap();

    // First upgrade: add age & mass columns
    test.use_precompiled_module("auto-migration-add-columns");
    let identity = test.database_identity.clone().unwrap();
    test.publish().name(&identity).break_clients(true).run().unwrap();
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
    test.use_precompiled_module("auto-migration-add-columns-again");
    test.publish().name(&identity).break_clients(true).run().unwrap();
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
    let mut test = Smoketest::builder()
        .precompiled_module("auto-migration-with-primary-key")
        .build();

    // Step 1: Publish with primary key.
    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Step 2: Remove primary key. Should succeed.
    test.use_precompiled_module("auto-migration-without-primary-key");
    test.publish()
        .name(&identity)
        .break_clients(true)
        .run()
        .expect("Removing primary key should succeed");

    // Step 3: Trivial change (add a reducer). This is where #3934 crashes.
    test.use_precompiled_module("auto-migration-without-primary-key-v2");
    test.publish()
        .name(&identity)
        .break_clients(true)
        .run()
        .expect("Publish after PK removal should succeed (issue #3934)");
}

#[test]
fn automigrate_reschema_event_table_arbitrarily() {
    let mut test = Smoketest::builder()
        .precompiled_module("auto-migration-event-table-before")
        .build();

    // Step 1: publish with event table.
    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Step 2: Reschema event table. Should work fine, even though we'd reject this change for a non-event table.
    test.use_precompiled_module("auto-migration-event-table-after");
    test.publish()
        .name(&identity)
        .break_clients(true)
        .run()
        .expect("Changing schema of event table should succeed");

    // Step 3: Reschema event table right back. Should still work fine.
    test.use_precompiled_module("auto-migration-event-table-before");
    test.publish()
        .name(&identity)
        .break_clients(true)
        .run()
        .expect("Changing schema of event table should succeed");
}
