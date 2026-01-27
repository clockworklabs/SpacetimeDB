use regex::Regex;
use spacetimedb_smoketests::Smoketest;

/// Verify that we can add and list server configurations
#[test]
fn test_servers() {
    let test = Smoketest::builder().autopublish(false).build();

    // Add a test server (local-only command, no --server flag needed)
    let output = test
        .spacetime(&[
            "server",
            "add",
            "--url",
            "https://testnet.spacetimedb.com",
            "testnet",
            "--no-fingerprint",
        ])
        .unwrap();

    assert!(
        output.contains("testnet.spacetimedb.com"),
        "Expected host in output: {}",
        output
    );

    // List servers (local-only command)
    let servers = test.spacetime(&["server", "list"]).unwrap();

    let testnet_re = Regex::new(r"(?m)^\s*testnet\.spacetimedb\.com\s+https\s+testnet\s*$").unwrap();
    assert!(
        testnet_re.is_match(&servers),
        "Expected testnet in server list: {}",
        servers
    );

    // Add the local test server to the config so we can check its fingerprint
    test.spacetime(&[
        "server",
        "add",
        "--url",
        &test.server_url,
        "test-local",
        "--no-fingerprint",
    ])
    .unwrap();

    // Check fingerprint commands (local-only command)
    let output = test.spacetime(&["server", "fingerprint", "test-local", "-y"]).unwrap();
    // The exact message may vary, just check it doesn't error
    assert!(
        output.contains("fingerprint") || output.contains("Fingerprint"),
        "Expected fingerprint message: {}",
        output
    );
}

/// Verify that we can edit server configurations
#[test]
fn test_edit_server() {
    let test = Smoketest::builder().autopublish(false).build();

    // Add a server to edit (local-only command)
    test.spacetime(&["server", "add", "--url", "https://foo.com", "foo", "--no-fingerprint"])
        .unwrap();

    // Edit the server (local-only command)
    test.spacetime(&[
        "server",
        "edit",
        "foo",
        "--url",
        "https://edited-testnet.spacetimedb.com",
        "--new-name",
        "edited-testnet",
        "--no-fingerprint",
        "--yes",
    ])
    .unwrap();

    // Verify the edit (local-only command)
    let servers = test.spacetime(&["server", "list"]).unwrap();
    let edited_re = Regex::new(r"(?m)^\s*edited-testnet\.spacetimedb\.com\s+https\s+edited-testnet\s*$").unwrap();
    assert!(
        edited_re.is_match(&servers),
        "Expected edited server in list: {}",
        servers
    );
}
