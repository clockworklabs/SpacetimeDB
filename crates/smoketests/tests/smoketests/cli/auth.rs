//! CLI auth command tests (`login` / `logout`)

use spacetimedb_smoketests::{require_local_server, Smoketest};
use std::fs;
use std::process::Output;
use std::time::{Duration, Instant};

fn output_stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn output_stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed:\nstdout: {}\nstderr: {}",
        output_stdout(output),
        output_stderr(output),
    );
}

fn read_config(test: &Smoketest) -> toml::Table {
    let raw = fs::read_to_string(&test.config_path).expect("Failed to read config");
    raw.parse::<toml::Table>().expect("Failed to parse config")
}

fn write_config(test: &Smoketest, config: &toml::Table) {
    let raw = toml::to_string(config).expect("Failed to serialize config");
    fs::write(&test.config_path, raw).expect("Failed to write config");
}

#[test]
fn cli_logout_removes_cached_tokens() {
    require_local_server!();
    let test = Smoketest::builder().autopublish(false).build();

    let login = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&login, "initial login");

    // Simulate a cached web session token; logout should clear both token fields.
    let mut config = read_config(&test);
    config.insert(
        "web_session_token".to_string(),
        toml::Value::String("fake-web-session-token".to_string()),
    );
    write_config(&test, &config);

    let logout = test.spacetime_cmd(&["logout"]);
    assert_success(&logout, "logout");
    assert!(
        output_stdout(&logout).contains("Logged out (identity "),
        "logout stdout should include identity message:\n{}",
        output_stdout(&logout),
    );

    let config_after = read_config(&test);
    assert!(
        config_after.get("spacetimedb_token").is_none(),
        "spacetimedb_token should be removed after logout: {:?}",
        config_after.get("spacetimedb_token")
    );
    assert!(
        config_after.get("web_session_token").is_none(),
        "web_session_token should be removed after logout: {:?}",
        config_after.get("web_session_token")
    );
}

#[test]
// Even if there's no web session, logout still removes the SpacetimeDB token
fn cli_logout_removes_cached_tokens_without_web_token() {
    require_local_server!();
    let test = Smoketest::builder().autopublish(false).build();

    let login = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&login, "initial login");

    let logout = test.spacetime_cmd(&["logout"]);
    assert_success(&logout, "logout");
    assert!(
        output_stdout(&logout).contains("Logged out (identity "),
        "logout stdout should include identity message:\n{}",
        output_stdout(&logout),
    );

    let config_after = read_config(&test);
    assert!(
        config_after.get("spacetimedb_token").is_none(),
        "spacetimedb_token should be removed after logout: {:?}",
        config_after.get("spacetimedb_token")
    );
    assert!(
        config_after.get("web_session_token").is_none(),
        "web_session_token should be removed after logout: {:?}",
        config_after.get("web_session_token")
    );
}

#[test]
fn cli_logout_is_idempotent() {
    require_local_server!();
    let test = Smoketest::builder().autopublish(false).build();

    let login = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&login, "initial login");

    let first_logout = test.spacetime_cmd(&["logout"]);
    assert_success(&first_logout, "first logout");
    assert!(
        output_stdout(&first_logout).contains("Logged out "),
        "first logout should report logged-out:\n{}",
        output_stdout(&first_logout)
    );

    let second_logout = test.spacetime_cmd(&["logout"]);
    assert_success(&second_logout, "second logout");
    assert!(
        output_stdout(&second_logout).contains("You are not logged in."),
        "second logout should report not logged in:\n{}",
        output_stdout(&second_logout)
    );
}

#[test]
fn cli_direct_login_works_and_shows_core_messages() {
    require_local_server!();
    let test = Smoketest::builder().autopublish(false).build();

    let login = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&login, "direct login");

    let login_stdout = output_stdout(&login);
    assert!(
        login_stdout.contains("Logged in "),
        "direct login stdout missing confirmation:\n{}",
        login_stdout
    );

    let show = test.spacetime_cmd(&["login", "show"]);
    assert_success(&show, "login show");
    assert!(
        output_stdout(&show).contains("You are logged in as "),
        "login show should report current identity:\n{}",
        output_stdout(&show)
    );
}

#[test]
fn cli_logging_in_twice_works() {
    require_local_server!();
    let test = Smoketest::builder().autopublish(false).build();

    let first = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&first, "first login");

    let second = test.spacetime_cmd(&["login", "--server-issued-login", &test.server_url]);
    assert_success(&second, "second login");

    let second_stdout = output_stdout(&second);
    assert!(
        second_stdout.contains("Logged out (identity "),
        "second login should log out previous identity first:\n{}",
        second_stdout
    );
    assert!(
        second_stdout.contains("Logged in with identity "),
        "second login should complete with a new login:\n{}",
        second_stdout
    );
}

fn try_until_timeout<F: FnMut() -> Option<R>, R>(timeout: Duration, mut f: F) -> Option<R> {
    let start = Instant::now();
    loop {
        match f() {
            Some(result) => return Some(result),
            None => {
                if start.elapsed() > timeout {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
}

/// Test that `spacetime login --token <token>` exits immediately after saving
/// the token, without falling through to the interactive web login flow.
///
/// Without the fix in PR #4579, the command would fall through to the web
/// login flow, which hangs waiting for a browser callback.
#[test]
fn cli_login_with_token() {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let test = Smoketest::builder().autopublish(false).build();
    let cli_path = spacetimedb_guard::ensure_binaries_built();

    let mut child = Command::new(&cli_path)
        .arg("--config-path")
        .arg(&test.config_path)
        .args(["login", "--token", "test-dummy-token", "--no-browser"])
        .env_remove("BROWSER")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn spacetime login");

    // Run with timeout in case something goes wrong and it tries to open the browser for login.
    let timeout = Duration::from_secs(5);
    let Some(status) = try_until_timeout(timeout, || child.try_wait().expect("Failed to poll child")) else {
        child.kill().ok();
        panic!(
            "spacetime login --token hung for >{timeout:?} — \
            likely fell through to web login flow"
        );
    };
    let mut stdout = String::new();
    child.stdout.take().unwrap().read_to_string(&mut stdout).unwrap();
    assert!(
        status.success(),
        "spacetime login --token failed (exit {status}):\n{stdout}"
    );
    assert!(
        stdout.contains("Token saved."),
        "Expected 'Token saved.' in output, got: {stdout}"
    );
}
