//! Add/remove index tests translated from smoketests/tests/add_remove_index.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}
"#;

const MODULE_CODE_INDEXED: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { #[index(btree)] id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { #[index(btree)] id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext) {
    let id = 1_001;
    ctx.db.t1().insert(T1 { id });
    ctx.db.t2().insert(T2 { id });
}
"#;

const JOIN_QUERY: &str = "select t1.* from t1 join t2 on t1.id = t2.id where t2.id = 1001";

/// First publish without the indices,
/// then add the indices, and publish,
/// and finally remove the indices, and publish again.
/// There should be no errors
/// and the unindexed versions should reject subscriptions.
#[test]
fn test_add_then_remove_index() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let name = format!("test-db-{}", std::process::id());

    // Publish and attempt a subscribing to a join query.
    // There are no indices, resulting in an unsupported unindexed join.
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail without indices");

    // Publish the indexed version.
    // Now we have indices, so the query should be accepted.
    test.write_module_code(MODULE_CODE_INDEXED).unwrap();
    test.publish_module_named(&name, false).unwrap();

    // Subscription should work now (n=0 just verifies the query is accepted)
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(
        result.is_ok(),
        "Expected subscription to succeed with indices, got: {:?}",
        result.err()
    );

    // Verify call works too
    test.call("add", &[]).unwrap();

    // Publish the unindexed version again, removing the index.
    // The initial subscription should be rejected again.
    test.write_module_code(MODULE_CODE).unwrap();
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail after removing indices");
}
