use spacetimedb_smoketests::{random_string, require_local_server, Smoketest};

const RECURSIVE_MODULE: &str = r#"
use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
pub struct TestNestedType {
    pub test_nest: Option<Box<TestNestedType>>,
}

#[spacetimedb::table(accessor = container, public)]
pub struct Container {
    pub nested: TestNestedType,
}
"#;

const VALID_MODULE: &str = r#"
#[spacetimedb::table(accessor = container, public)]
pub struct Container {
    pub id: u64,
}
"#;

#[test]
fn test_recursive_type_publish_is_rejected_without_crashing_server() {
    require_local_server!();

    let mut test = Smoketest::builder()
        .module_code(RECURSIVE_MODULE)
        .autopublish(false)
        .build();
    let database_name = format!("recursive-type-{}", random_string());

    let error = test
        .publish()
        .name(&database_name)
        .run()
        .expect_err("publishing a recursive table column should fail validation");
    let error = format!("{error:#}");
    assert!(
        error.contains("has a recursive type") && error.contains("store references as IDs instead"),
        "expected an actionable recursive type validation error, got: {error}"
    );

    // A subsequent valid publish proves the standalone server survived the rejected module.
    test.write_module_code(VALID_MODULE).unwrap();
    test.publish().name(database_name).run().unwrap();
}
