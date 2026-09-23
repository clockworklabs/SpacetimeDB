use spacetimedb_smoketests::Smoketest;

fn logs_filtered(test: &Smoketest, n: usize, extra_args: &[&str]) -> Vec<serde_json::Value> {
    let identity = test.database_identity.as_ref().expect("No database published");
    let n_str = n.to_string();

    let mut args = vec!["logs", "--server", &test.server_url, "--format=json", "-n", &n_str];
    args.extend_from_slice(extra_args);
    args.push("--");
    args.push(identity);

    let output = test.spacetime(&args).expect("spacetime logs failed");
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("Failed to parse log record"))
        .collect()
}

fn messages(records: &[serde_json::Value]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| r.get("message").and_then(|m| m.as_str()).map(String::from))
        .collect()
}

/// Without --level, all log levels are returned.
#[test]
fn test_logs_no_filter() {
    let test = Smoketest::builder().precompiled_module("logs-level-filter").build();

    test.call("log_all_levels", &[]).unwrap();

    let msgs = messages(&logs_filtered(&test, 100, &[]));
    assert!(msgs.iter().any(|m| m == "msg-trace"), "missing trace: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-debug"), "missing debug: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-info"), "missing info: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-warn"), "missing warn: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-error"), "missing error: {msgs:?}");
}

/// --level filters to that level and above.
#[test]
fn test_logs_level_minimum() {
    let test = Smoketest::builder().precompiled_module("logs-level-filter").build();

    test.call("log_all_levels", &[]).unwrap();

    // --level warn: only warn and error
    let msgs = messages(&logs_filtered(&test, 100, &["--level", "warn"]));
    assert!(!msgs.iter().any(|m| m == "msg-trace"), "unexpected trace: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-debug"), "unexpected debug: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-info"), "unexpected info: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-warn"), "missing warn: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-error"), "missing error: {msgs:?}");

    // --level error: only error
    let msgs = messages(&logs_filtered(&test, 100, &["--level", "error"]));
    assert!(!msgs.iter().any(|m| m == "msg-trace"), "unexpected trace: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-debug"), "unexpected debug: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-info"), "unexpected info: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-warn"), "unexpected warn: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-error"), "missing error: {msgs:?}");
}

/// --level-exact shows only the specified level.
#[test]
fn test_logs_level_exact() {
    let test = Smoketest::builder().precompiled_module("logs-level-filter").build();

    test.call("log_all_levels", &[]).unwrap();

    // --level info --level-exact: only info
    let msgs = messages(&logs_filtered(&test, 100, &["--level", "info", "--level-exact"]));
    assert!(!msgs.iter().any(|m| m == "msg-trace"), "unexpected trace: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-debug"), "unexpected debug: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-info"), "missing info: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-warn"), "unexpected warn: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-error"), "unexpected error: {msgs:?}");

    // --level debug --level-exact: only debug
    let msgs = messages(&logs_filtered(&test, 100, &["--level", "debug", "--level-exact"]));
    assert!(!msgs.iter().any(|m| m == "msg-trace"), "unexpected trace: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "msg-debug"), "missing debug: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-info"), "unexpected info: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-warn"), "unexpected warn: {msgs:?}");
    assert!(!msgs.iter().any(|m| m == "msg-error"), "unexpected error: {msgs:?}");
}
