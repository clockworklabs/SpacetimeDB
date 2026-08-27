use spacetimedb_smoketests::Smoketest;

/// End-to-end test of database env vars:
/// CLI management, module reads via `ctx.env`, and upsert/delete semantics.
#[test]
fn test_env_vars() {
    let test = Smoketest::builder().precompiled_module("env-vars").build();
    let identity = test.database_identity.clone().unwrap();
    let server = test.server_url.clone();

    let env = |args: &[&str]| {
        let mut cmd = vec!["env"];
        cmd.extend(args);
        cmd.extend(["--server", &server]);
        test.spacetime(&cmd)
    };

    // Setting and getting a value round-trips.
    env(&["set", &identity, "MY_KEY", "hello"]).unwrap();
    assert_eq!(env(&["get", &identity, "MY_KEY"]).unwrap().trim(), "hello");

    // The module sees the value through ctx.env.
    test.call("read_env", &["\"MY_KEY\""]).unwrap();
    let logs = test.logs(100).unwrap();
    assert!(logs.iter().any(|l| l.contains("env: MY_KEY=hello")), "logs: {logs:?}");

    // Setting an existing key overwrites it, and the module
    // sees the update on the next call without a republish.
    env(&["set", &identity, "MY_KEY", "world"]).unwrap();
    assert_eq!(env(&["get", &identity, "MY_KEY"]).unwrap().trim(), "world");
    test.call("read_env", &["\"MY_KEY\""]).unwrap();
    let logs = test.logs(100).unwrap();
    assert!(logs.iter().any(|l| l.contains("env: MY_KEY=world")), "logs: {logs:?}");

    // An unset key reads as absent.
    test.call("read_env", &["\"NO_KEY\""]).unwrap();
    let logs = test.logs(100).unwrap();
    assert!(logs.iter().any(|l| l.contains("env: NO_KEY is unset")), "logs: {logs:?}");

    // With the value omitted, it is read from stdin and never echoed.
    let out = env_with_stdin(&test, &server, &identity, "SECRET", "hunter2\n");
    assert!(!out.contains("hunter2"), "secret leaked in output: {out}");
    assert_eq!(env(&["get", &identity, "SECRET"]).unwrap().trim(), "hunter2");

    // `env list` shows keys only.
    let out = env(&["list", &identity]).unwrap();
    assert!(out.contains("MY_KEY") && out.contains("SECRET"), "list: {out}");
    assert!(!out.contains("world") && !out.contains("hunter2"), "list: {out}");

    // Owners can also inspect st_env with plain SQL.
    test.assert_sql(
        "SELECT value FROM st_env WHERE key = 'MY_KEY'",
        r#" value
---------
 "world""#,
    );

    // Invalid keys are rejected.
    assert!(env(&["set", &identity, "1BAD", "v"]).is_err());
    assert!(env(&["set", &identity, "has-dash", "v"]).is_err());

    // `env del` removes the key and is idempotent.
    env(&["del", &identity, "MY_KEY"]).unwrap();
    assert!(env(&["get", &identity, "MY_KEY"]).is_err());
    env(&["del", &identity, "MY_KEY"]).unwrap();
}

fn env_with_stdin(test: &Smoketest, server: &str, identity: &str, key: &str, stdin: &str) -> String {
    test.spacetime_with_stdin(&["env", "set", identity, key, "--server", server], stdin)
        .unwrap()
}

/// The host must reject module syscalls that target `st_env` directly,
/// so a hand-written module bypassing the SDK bindings cannot read secrets.
#[test]
fn test_st_env_hidden_from_modules() {
    let test = Smoketest::builder().precompiled_module("env-vars").build();
    let identity = test.database_identity.clone().unwrap();

    test.spacetime(&[
        "env",
        "set",
        &identity,
        "SECRET",
        "hunter2",
        "--server",
        &test.server_url,
    ])
    .unwrap();

    test.call("probe_st_env", &[]).unwrap();
    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("probe: st_env not found")),
        "logs: {logs:?}"
    );
    assert!(!logs.iter().any(|l| l.contains("probe: resolved st_env")), "logs: {logs:?}");
}

/// Non-owners can neither manage env vars nor read `st_env`.
#[test]
fn test_env_vars_permissions() {
    let test = Smoketest::builder().precompiled_module("env-vars").build();
    let identity = test.database_identity.clone().unwrap();
    let server = test.server_url.clone();

    test.spacetime(&["env", "set", &identity, "MY_KEY", "hello", "--server", &server])
        .unwrap();

    // Switch to a non-owner identity.
    test.new_identity().unwrap();

    assert!(test
        .spacetime(&["env", "set", &identity, "MY_KEY", "stolen", "--server", &server])
        .is_err());
    assert!(test
        .spacetime(&["env", "get", &identity, "MY_KEY", "--server", &server])
        .is_err());
    assert!(test
        .spacetime(&["env", "del", &identity, "MY_KEY", "--server", &server])
        .is_err());
    assert!(test
        .spacetime(&["env", "list", &identity, "--server", &server])
        .is_err());
    assert!(test.sql("SELECT * FROM st_env").is_err());
}
