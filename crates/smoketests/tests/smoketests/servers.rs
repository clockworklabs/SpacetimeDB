use regex::Regex;
use spacetimedb_smoketests::Smoketest;

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
