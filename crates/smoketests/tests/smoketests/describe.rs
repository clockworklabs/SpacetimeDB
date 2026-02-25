use spacetimedb_smoketests::Smoketest;

/// Check describing a module
#[test]
fn test_describe() {
    let test = Smoketest::builder().precompiled_module("describe").build();

    let identity = test.database_identity.as_ref().unwrap();

    // Describe the whole module
    test.spacetime(&["describe", "--server", &test.server_url, "--json", identity])
        .unwrap();

    // Describe a specific reducer
    test.spacetime(&[
        "describe",
        "--server",
        &test.server_url,
        "--json",
        identity,
        "reducer",
        "say_hello",
    ])
    .unwrap();

    // Describe a specific table
    test.spacetime(&[
        "describe",
        "--server",
        &test.server_url,
        "--json",
        identity,
        "table",
        "person",
    ])
    .unwrap();
}
