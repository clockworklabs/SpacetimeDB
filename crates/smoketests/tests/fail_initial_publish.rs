use spacetimedb_smoketests::Smoketest;

/// Module code with a bug: `Person` is the wrong table name, should be `person`
const MODULE_CODE_BROKEN: &str = r#"
use spacetimedb::{client_visibility_filter, Filter};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[client_visibility_filter]
// Bug: `Person` is the wrong table name, should be `person`.
const HIDE_PEOPLE_EXCEPT_ME: Filter = Filter::Sql("SELECT * FROM Person WHERE name = 'me'");
"#;

const FIXED_QUERY: &str = r#""sql": "SELECT * FROM person WHERE name = 'me'""#;

/// This tests that publishing an invalid module does not leave a broken entry in the control DB.
#[test]
fn test_fail_initial_publish() {
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_BROKEN)
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // First publish should fail due to broken module
    let result = test.publish_module_named(&name, false);
    assert!(result.is_err(), "Expected publish to fail with broken module");

    // Describe should fail because database doesn't exist
    let describe_output = test.spacetime_cmd(&["describe", "--server", &test.server_url, "--json", &name]);
    assert!(
        !describe_output.status.success(),
        "Expected describe to fail for non-existent database"
    );
    let stderr = String::from_utf8_lossy(&describe_output.stderr);
    assert!(
        stderr.contains("No such database"),
        "Expected 'No such database' in stderr, got: {}",
        stderr
    );

    // We can publish a fixed module under the same database name.
    // This used to be broken; the failed initial publish would leave
    // the control database in a bad state.
    test.use_precompiled_module("fail-initial-publish-fixed");
    test.publish_module_named(&name, false).unwrap();

    let describe_output = test
        .spacetime(&["describe", "--server", &test.server_url, "--json", &name])
        .unwrap();
    assert!(
        describe_output.contains(FIXED_QUERY),
        "Expected describe output to contain fixed query.\nGot: {}",
        describe_output
    );

    // Publishing the broken code again fails, but the database still exists afterwards,
    // with the previous version of the module code.
    test.write_module_code(MODULE_CODE_BROKEN).unwrap();
    let result = test.publish_module_named(&name, false);
    assert!(result.is_err(), "Expected publish to fail with broken module");

    // Database should still exist with the fixed code
    let describe_output = test
        .spacetime(&["describe", "--server", &test.server_url, "--json", &name])
        .unwrap();
    assert!(
        describe_output.contains(FIXED_QUERY),
        "Expected describe output to still contain fixed query after failed update.\nGot: {}",
        describe_output
    );
}
