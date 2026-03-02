use spacetimedb_smoketests::Smoketest;

const JOIN_QUERY: &str = "select t_1.* from t_1 join t_2 on t_1.id = t_2.id where t_2.id = 1001";

/// First publish without the indices,
/// then add the indices, and publish,
/// and finally remove the indices, and publish again.
/// There should be no errors
/// and the unindexed versions should reject subscriptions.
#[test]
fn test_add_then_remove_index() {
    let mut test = Smoketest::builder()
        .precompiled_module("add-remove-index")
        .autopublish(false)
        .build();

    // TODO: Does the name do anything? Other tests just let the DB assign.
    let name = format!("test-db-{}", std::process::id());

    // Publish and attempt a subscribing to a join query.
    // There are no indices, resulting in an unsupported unindexed join.
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail without indices");

    // Publish the indexed version.
    // Now we have indices, so the query should be accepted.
    test.use_precompiled_module("add-remove-index-indexed");
    test.publish_module_named(&name, false).unwrap();

    // Subscribe and hold across the call, then collect results
    let sub = test.subscribe_background(&[JOIN_QUERY], 1).unwrap();
    test.call_anon("add", &[]).unwrap();
    let results = sub.collect().unwrap();
    assert_eq!(results.len(), 1, "Expected 1 update from subscription");

    // Publish the unindexed version again, removing the index.
    // The initial subscription should be rejected again.
    test.use_precompiled_module("add-remove-index");
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail after removing indices");
}
