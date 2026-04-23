use spacetimedb_smoketests::Smoketest;

/// Test that a locked database cannot be deleted.
#[test]
fn test_locked_database_cannot_be_deleted() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap();

    // Lock the database
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();

    // Try to delete — should fail
    let result = test.spacetime(&["delete", "--server", &test.server_url, identity, "--yes"]);
    assert!(
        result.is_err(),
        "Expected delete to fail on a locked database, but it succeeded"
    );
}

/// Test that a locked database cannot be reset with --delete-data.
#[test]
fn test_locked_database_cannot_be_reset() {
    let mut test = Smoketest::builder()
        .precompiled_module("modules-basic")
        .autopublish(false)
        .build();

    let name = format!("test-lock-reset-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    let identity = test.database_identity.as_ref().unwrap();

    // Lock the database
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();

    // Try to republish with --delete-data — should fail
    let result = test.publish_module_with_options(&name, true, false);
    assert!(
        result.is_err(),
        "Expected publish with --delete-data to fail on a locked database, but it succeeded"
    );
}

/// Test that unlocking a locked database allows deletion.
#[test]
fn test_unlock_allows_delete() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap();

    // Lock the database
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();

    // Verify delete is blocked
    let result = test.spacetime(&["delete", "--server", &test.server_url, identity, "--yes"]);
    assert!(result.is_err(), "Expected delete to fail while locked");

    // Unlock the database
    test.spacetime(&["unlock", "--server", &test.server_url, identity])
        .unwrap();

    // Now delete should succeed
    test.spacetime(&["delete", "--server", &test.server_url, identity, "--yes"])
        .unwrap();
}

/// Test that locking an already-locked database is idempotent.
#[test]
fn test_lock_is_idempotent() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap();

    // Lock twice — second lock should not error
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();
}

/// Test that unlocking an already-unlocked database is idempotent.
#[test]
fn test_unlock_is_idempotent() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap();

    // Unlock without ever locking — should not error
    test.spacetime(&["unlock", "--server", &test.server_url, identity])
        .unwrap();
}

/// Test that a non-owner cannot lock or unlock a database.
#[test]
fn test_non_owner_cannot_lock_or_unlock() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap().clone();

    // Switch to a new identity
    test.new_identity().unwrap();

    // Non-owner lock should fail
    let result = test.spacetime(&["lock", "--server", &test.server_url, &identity]);
    assert!(result.is_err(), "Expected non-owner lock to fail, but it succeeded");

    // Non-owner unlock should fail
    let result = test.spacetime(&["unlock", "--server", &test.server_url, &identity]);
    assert!(result.is_err(), "Expected non-owner unlock to fail, but it succeeded");
}

/// Test that publish without --delete-data still works on a locked database.
/// Lock only prevents deletion, not updates.
#[test]
fn test_locked_database_allows_publish() {
    let mut test = Smoketest::builder()
        .precompiled_module("modules-basic")
        .autopublish(false)
        .build();

    let name = format!("test-lock-publish-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    let identity = test.database_identity.as_ref().unwrap();

    // Lock the database
    test.spacetime(&["lock", "--server", &test.server_url, identity])
        .unwrap();

    // Republish without --delete-data — should succeed
    test.publish_module_clear(false).unwrap();
}
