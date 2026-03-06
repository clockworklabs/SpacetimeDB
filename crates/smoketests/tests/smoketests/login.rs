use spacetimedb_smoketests::Smoketest;

/// Test that `spacetime login --token <token>` exits immediately after saving
/// the token, without falling through to the interactive web login flow.
///
/// Without the fix in PR #4579, the command would fall through to the web
/// login flow, which tries to open a browser and fails in CI.
#[test]
fn test_login_with_token_does_not_fallthrough() {
    let test = Smoketest::builder()
        .autopublish(false)
        .build();

    // A dummy token that won't decode to a valid identity.
    let output = test.spacetime_cmd(&["login", "--token", "test-dummy-token"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Expected `spacetime login --token` to succeed, but it failed.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Token saved."),
        "Expected 'Token saved.' in output, got: {stdout}"
    );
}
