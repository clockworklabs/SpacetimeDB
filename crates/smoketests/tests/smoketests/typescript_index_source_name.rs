use spacetimedb_smoketests::{random_string, require_local_server, require_pnpm, Smoketest};

const TYPESCRIPT_MODULE_WITHOUT_NEW_COLUMNS: &str = r#"import { schema, table, t } from "spacetimedb/server";

const users = table(
  { name: "users", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    emailAddress: t.string().index("btree"),
  },
);

const spacetimedb = schema({
  users,
});
export default spacetimedb;

export const insert_user = spacetimedb.reducer(
  {
    name: t.string(),
    emailAddress: t.string(),
  },
  (ctx, { name, emailAddress }) => {
    ctx.db.users.insert({
      id: 0n,
      name,
      emailAddress,
    });
  },
);
"#;

const TYPESCRIPT_MODULE_WITH_NEW_COLUMNS: &str = r#"import { schema, table, t } from "spacetimedb/server";

const users = table(
  { name: "users", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    emailAddress: t.string().index("btree"),
    age: t.number().optional().default(undefined),
    isActive: t.bool().default(false).index(),
  },
);

const spacetimedb = schema({
  users,
});
export default spacetimedb;

export const find_user_by_email = spacetimedb.reducer(
  { emailAddress: t.string() },
  (ctx, { emailAddress }) => {
    let count = 0;
    for (const _row of ctx.db.users.emailAddress.filter(emailAddress)) {
      count += 1;
    }
    console.info(`matched ${count}`);
  },
);

export const find_users_by_active_status = spacetimedb.reducer(
  { isActive: t.bool() },
  (ctx, { isActive }) => {
    let count = 0;
    for (const _row of ctx.db.users.isActive.filter(isActive)) {
      count += 1;
    }
    console.info(`matched active users ${count}`);
  },
);
"#;

#[test]
fn test_typescript_add_optional_columns() {
    require_pnpm!();
    require_local_server!();

    let mut test = Smoketest::builder().autopublish(false).build();
    let module_name = format!("typescript-add-optional-columns-{}", random_string());

    let database_identity = test
        .publish_typescript_module_source(
            "typescript-add-optional-columns-v1",
            &module_name,
            TYPESCRIPT_MODULE_WITHOUT_NEW_COLUMNS,
        )
        .unwrap();

    test.call("insert_user", &["Alice", "alice@example.com"]).unwrap();

    test.restart_server();

    test.publish_typescript_module_source_clear(
        "typescript-add-optional-columns-v2",
        &database_identity,
        TYPESCRIPT_MODULE_WITH_NEW_COLUMNS,
        false,
    )
    .unwrap();

    test.call("find_user_by_email", &["alice@example.com"]).unwrap();
    test.call("find_users_by_active_status", &["false"]).unwrap();
}
