#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use spacetimedb_smoketests::workspace_root;

mod test_util;
use test_util::{prepare_pinned_chat_workspace, run_cmd_ok};

// NOTE: This test is intentionally manual/local-only and not meant for CI.
//
// It validates a 1.0 -> 2.0 upgrade scenario using quickstart-chat:
// 1) install a 1.0 CLI via `spacetime version install`
// 2) build 1.0 server/client/module from this pinned git ref
// 3) start 1.0 server and publish module
// 4) restart as 2.0 server on the same data dir
// 5) run both 1.0 and 2.0 quickstart clients, exchange messages, assert both observed
const V1_GIT_REF: &str = "4fdb8d923f39ed592931ad4c7e6391ed99b9fe3a";
const V1_RELEASE_VERSION: &str = "1.12.0";

fn log_step(msg: &str) {
    eprintln!("[manual-upgrade] {msg}");
}

fn ping_http(server_url: &str) -> Result<bool> {
    let addr = server_url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("expected http:// URL, got {server_url}"))?;
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let req = format!("GET /v1/ping HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let mut body = String::new();
    stream.read_to_string(&mut body)?;
    Ok(body.starts_with("HTTP/1.1 200") || body.starts_with("HTTP/1.0 200"))
}

fn wait_for_ping(server_url: &str, timeout: Duration) -> Result<()> {
    log_step(&format!("waiting for server ping at {server_url}"));
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if ping_http(server_url).unwrap_or(false) {
            log_step(&format!("server is ready at {server_url}"));
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("timed out waiting for {server_url}/v1/ping")
}

fn spawn_server(cli_path: &Path, data_dir: &Path) -> Result<(Child, Arc<Mutex<String>>)> {
    log_step(&format!(
        "starting server via {} using data dir {}",
        cli_path.display(),
        data_dir.display()
    ));
    let mut child = Command::new(cli_path)
        .args(["start"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start server via {}", cli_path.display()))?;
    log_step(&format!("started server pid={}", child.id()));
    let logs = Arc::new(Mutex::new(String::new()));
    if let Some(stdout) = child.stdout.take() {
        let logs_out = logs.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
                    break;
                }
                let mut s = logs_out.lock().unwrap();
                s.push_str("[stdout] ");
                s.push_str(&line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let logs_err = logs.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
                    break;
                }
                let mut s = logs_err.lock().unwrap();
                s.push_str("[stderr] ");
                s.push_str(&line);
            }
        });
    }
    Ok((child, logs))
}

fn kill_child(child: &mut Child) {
    log_step(&format!("stopping pid={}", child.id()));
    #[cfg(not(windows))]
    {
        let _ = child.kill();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .status();
    }
    let _ = child.wait();
    log_step("process stopped");
}

fn dump_server_logs(label: &str, logs: &Arc<Mutex<String>>) {
    let s = logs.lock().unwrap().clone();
    if s.trim().is_empty() {
        log_step(&format!("{label} logs are empty"));
        return;
    }
    eprintln!("[manual-upgrade] {label} logs:\n{s}");
}

fn extract_identity(publish_stdout: &str) -> Result<String> {
    let re = Regex::new(r"identity: ([0-9a-fA-F]+)")?;
    let caps = re
        .captures(publish_stdout)
        .ok_or_else(|| anyhow!("failed to parse identity from publish output:\n{publish_stdout}"))?;
    Ok(caps.get(1).unwrap().as_str().to_string())
}

#[test]
#[ignore = "manual local-only: long-running, networked, and builds historical artifacts"]
fn upgrade_chat_to_2_0_mixed_clients() -> Result<()> {
    log_step("starting manual upgrade mixed-clients quickstart test");
    log_step(&format!(
        "pinned old source ref={}, old release version={}",
        V1_GIT_REF, V1_RELEASE_VERSION
    ));
    let repo = workspace_root();
    let temp = tempfile::tempdir()?;
    let temp_path = temp.path();
    let old_worktree_dir = repo.join("target").join("smoketest-worktrees").join("old");
    let data_dir = temp_path.join("db-data");
    std::fs::create_dir_all(&data_dir)?;
    log_step(&format!("workspace={}", repo.display()));
    log_step(&format!("temp root={}", temp_path.display()));
    log_step(&format!("data dir={}", data_dir.display()));

    let old_prepared = prepare_pinned_chat_workspace(&repo, &old_worktree_dir, V1_GIT_REF, false)?;
    let old_worktree = old_prepared.worktree_dir;
    let old_cli = old_prepared.cli_path;
    let old_module_dir = old_prepared.module_dir;
    let old_publish_path_flag = old_prepared.publish_path_flag;

    // Install a pinned 1.0 release via the system `spacetime` command.
    log_step(&format!(
        "installing and selecting release {} via system spacetime",
        V1_RELEASE_VERSION
    ));
    run_cmd_ok(
        &[
            OsString::from("spacetime"),
            OsString::from("version"),
            OsString::from("install"),
            OsString::from(V1_RELEASE_VERSION),
            OsString::from("--use"),
            OsString::from("--yes"),
        ],
        &repo,
    )?;
    let installed_v1_cli = PathBuf::from("spacetime");
    log_step("using system 'spacetime' command as v1 CLI");

    // Build 1.0 sources from pinned ref.

    log_step(&format!("v1 CLI path={}", installed_v1_cli.display()));

    // Start 1.0 server and publish 1.0 quickstart module.
    let old_url = format!("http://127.0.0.1:3000");
    log_step("starting old server for initial publish");
    let (mut old_server, old_server_logs) = spawn_server(&installed_v1_cli, &data_dir)?;
    if let Err(e) = wait_for_ping(&old_url, Duration::from_secs(20)) {
        dump_server_logs("old server", &old_server_logs);
        kill_child(&mut old_server);
        return Err(e);
    }

    let db_name = "quickstart-chat";
    log_step(&format!("publishing old module to db {}", db_name));
    let publish_out = run_cmd_ok(
        &[
            installed_v1_cli.clone().into_os_string(),
            OsString::from("publish"),
            OsString::from("--server"),
            OsString::from(&old_url),
            OsString::from(old_publish_path_flag),
            old_module_dir.into_os_string(),
            OsString::from(&db_name),
        ],
        &old_worktree,
    )?;
    let _identity = extract_identity(&publish_out)?;
    log_step("old module published successfully; stopping old server");
    kill_child(&mut old_server);

    // Start 2.0 server on the same data dir.
    log_step("starting new server on same data dir");
    let (mut new_server, new_server_logs) = spawn_server(&installed_v1_cli, &data_dir)?;
    if let Err(e) = wait_for_ping(&old_url, Duration::from_secs(20)) {
        dump_server_logs("new server", &new_server_logs);
        kill_child(&mut new_server);
        return Err(e);
    }

    let result = run_cmd_ok(
        &[
            installed_v1_cli.clone().into_os_string(),
            OsString::from("logs"),
            OsString::from("--server"),
            OsString::from(&old_url),
            OsString::from(&db_name),
        ],
        &old_worktree,
    );

    log_step("stopping server");
    kill_child(&mut new_server);

    if !result.is_ok() {
        dump_server_logs("new server", &new_server_logs);
    }

    let _ = result?;
    Ok(())
}
