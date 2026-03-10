use spacetimedb_smoketests::{require_local_server, require_pnpm, Smoketest};

const TS_MODULE_BASIC: &str = r#"import { schema, t, table } from "spacetimedb/server";

const person = table(
    { name: "person", public: true },
    {
        id: t.u64().primaryKey().autoInc(),
        name: t.string()
    }
);
const spacetimedb = schema({ person });
export default spacetimedb;

export const add = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ id: 0n, name });
});
"#;

/// Tests that updating a module and also changing the host type works.
///
/// Note that this test restarts the server.
#[test]
fn test_update_with_different_host_type() {
    require_pnpm!();
    require_local_server!();

    const PERSON_A: &str = "Person A";
    const PERSON_B: &str = "Person B";
    const PERSON_C: &str = "Person C";

    let mut test = Smoketest::builder()
        .precompiled_module("modules-basic")
        .autopublish(false)
        .build();

    let database_identity = test.publish_module().unwrap();
    add_person(&test, PERSON_A, "initial");

    // Publish a TS module.
    test.publish_typescript_module_source_clear("modules-basic-ts", &database_identity, TS_MODULE_BASIC, false)
        .unwrap();
    add_person(&test, PERSON_B, "post module update");

    // Restart and assert that the data is still there.
    test.restart_server();
    assert_has_rows(&test, &[PERSON_A, PERSON_B], "post restart");

    // Change back to original module and assert that the data is still there.
    test.publish_module_clear(false).unwrap();
    add_person(&test, PERSON_C, "post revert");

    // Restart once more and assert that the data is still there.
    test.restart_server();
    assert_has_rows(&test, &[PERSON_A, PERSON_B, PERSON_C], "post restart 2");
}

fn add_person(test: &Smoketest, name: &str, context: &str) {
    test.call("add", &[name]).unwrap();
    assert_has_person(test, name, context);
}

fn assert_has_person(test: &Smoketest, name: &str, context: &str) {
    let output = test
        .sql_confirmed(&format!("select * from person where name = '{name}'"))
        .unwrap();
    assert!(
        output.contains(name),
        "{}: expected {} to be in result: {}",
        context,
        name,
        output
    );
}

fn assert_has_rows(test: &Smoketest, names: &[&str], context: &str) {
    let output = test.sql_confirmed("select * from person").unwrap();
    assert!(
        output
            .lines()
            .skip(2)
            .all(|row| names.iter().any(|name| row.contains(name))),
        "{context}: expected all of {names:?} to be in result: {output}"
    )
}
