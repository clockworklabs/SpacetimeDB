//! CLI server command tests

use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

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

fn free_local_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind local port");
    let port = listener.local_addr().expect("missing local addr").port();
    drop(listener);
    port
}

fn write_cli_toml(root_dir: &Path, listen_addr: &str) {
    let config_dir = root_dir.join("config");
    fs::create_dir_all(&config_dir).expect("failed to create config dir");
    fs::write(
        config_dir.join("cli.toml"),
        format!("listen_addr = \"{listen_addr}\"\n"),
    )
    .expect("failed to write cli.toml");
}

fn spawn_start(root_dir: &Path, extra_args: &[&str]) -> Child {
    let root_dir = root_dir.to_str().expect("root dir should be utf8");
    cli_cmd()
        .args(["--root-dir", root_dir, "start"])
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn spacetime start")
}

fn ping_url(url: &str) -> Output {
    cli_cmd()
        .args(["server", "ping", url])
        .output()
        .expect("failed to execute server ping")
}

fn wait_for_server(url: &str, child: &mut Child, timeout: Duration) {
    let start = Instant::now();
    loop {
        let ping = ping_url(url);
        if ping.status.success() {
            return;
        }

        if let Some(status) = child.try_wait().expect("failed to poll child") {
            let mut stdout = String::new();
            let mut stderr = String::new();
            child.stdout.take().unwrap().read_to_string(&mut stdout).unwrap();
            child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
            panic!(
                "spacetime start exited early ({status}) while waiting for {url}:\nstdout: {stdout}\nstderr: {stderr}"
            );
        }

        if start.elapsed() > timeout {
            child.kill().ok();
            let _ = child.wait();
            panic!("timed out waiting for spacetime start to answer on {url}");
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

fn stop_child(mut child: Child) {
    child.kill().ok();
    let _ = child.wait();
}

#[test]
fn cli_can_ping_spacetimedb_on_disk() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let output = cli_cmd()
        .args(["server", "ping", &spacetime.host_url.to_string()])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "ping failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_start_uses_listen_addr_from_cli_toml() {
    let root = tempfile::tempdir().expect("failed to create tempdir");
    let port = free_local_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("http://{listen_addr}");
    write_cli_toml(root.path(), &listen_addr);

    let mut child = spawn_start(root.path(), &[]);
    wait_for_server(&url, &mut child, Duration::from_secs(20));

    let ping = ping_url(&url);
    assert_success(&ping, "server ping after config-based start");
    stop_child(child);
}

#[test]
fn cli_start_explicit_listen_addr_overrides_cli_toml() {
    let root = tempfile::tempdir().expect("failed to create tempdir");
    let config_port = free_local_port();
    let mut explicit_port = free_local_port();
    while explicit_port == config_port {
        explicit_port = free_local_port();
    }

    let config_addr = format!("127.0.0.1:{config_port}");
    let explicit_addr = format!("127.0.0.1:{explicit_port}");
    let explicit_url = format!("http://{explicit_addr}");
    write_cli_toml(root.path(), &config_addr);

    let mut child = spawn_start(root.path(), &["--listen-addr", &explicit_addr]);
    wait_for_server(&explicit_url, &mut child, Duration::from_secs(20));

    let ping = ping_url(&explicit_url);
    assert_success(&ping, "server ping after explicit listen-addr override");
    stop_child(child);
}
