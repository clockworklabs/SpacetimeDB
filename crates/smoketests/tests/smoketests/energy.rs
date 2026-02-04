use regex::Regex;
use spacetimedb_smoketests::Smoketest;

/// Test getting energy balance.
#[test]
fn test_energy_balance() {
    let test = Smoketest::builder().build();

    let output = test
        .spacetime(&["energy", "balance", "--server", &test.server_url])
        .unwrap();
    let re = Regex::new(r#"\{"balance":"-?[0-9]+"\}"#).unwrap();
    assert!(re.is_match(&output), "Expected energy balance JSON, got: {}", output);
}
