use spacetimedb_smoketests::{random_string, Smoketest};

const TIMESTAMP_TAG: &str = "__timestamp_micros_since_unix_epoch__";

/// Test the /v1/database/{name}/unstable/timestamp endpoint
#[test]
fn test_timestamp_route() {
    let mut test = Smoketest::builder().autopublish(false).build();

    let name = random_string();

    // Since we didn't publish, we're not logged in yet, so `api_call` will fail to get a token.
    test.new_identity().unwrap();

    // A request for the timestamp at a non-existent database is an error with code 404
    let resp = test
        .api_call("GET", &format!("/v1/database/{}/unstable/timestamp", name))
        .unwrap();
    assert_eq!(
        resp.status_code, 404,
        "Expected 404 for non-existent database, got {}",
        resp.status_code
    );

    // Publish a module with the random name
    test.publish_module_named(&name, false).unwrap();

    // A request for the timestamp at an extant database is a success
    let resp = test
        .api_call("GET", &format!("/v1/database/{}/unstable/timestamp", name))
        .unwrap();
    assert!(
        resp.is_success(),
        "Expected success for existing database, got {}",
        resp.status_code
    );

    // The response body is a SATS-JSON encoded `Timestamp`
    let timestamp = resp.json().unwrap();
    assert!(
        timestamp.is_object(),
        "Expected timestamp to be an object, got {:?}",
        timestamp
    );
    assert!(
        timestamp.get(TIMESTAMP_TAG).is_some(),
        "Expected timestamp to have '{}' field, got {:?}",
        TIMESTAMP_TAG,
        timestamp
    );
    assert!(
        timestamp[TIMESTAMP_TAG].is_i64() || timestamp[TIMESTAMP_TAG].is_u64(),
        "Expected timestamp value to be an integer, got {:?}",
        timestamp[TIMESTAMP_TAG]
    );
}
