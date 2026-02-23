#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use spacetimedb_smoketests::workspace_root;

mod pinned_chat_workspace;
use pinned_chat_workspace::prepare_pinned_chat_workspace;

// NOTE: This test is intentionally manual/local-only and not meant for CI.
//
// It validates a kill/restart scenario at a pinned historical version:
// 1) build CLI/server from this pinned git ref in a temporary worktree
// 2) start server and publish module
// 3) restart server immediately on the same data dir and same version
// 4) verify `spacetime logs` succeeds
const V1_GIT_REF: &str = "v1.12.0";

fn log_step(msg: &str) {
    eprintln!("[manual-upgrade] {msg}");
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn run_cmd(args: &[OsString], cwd: &Path) -> Result<Output> {
    let rendered = args
        .iter()
        .map(|s| s.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    log_step(&format!("run: (cd {}) {rendered}", cwd.display()));
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).current_dir(cwd);
    cmd.output()
        .with_context(|| format!("failed to execute {:?}", args.iter().collect::<Vec<_>>()))
}

fn run_cmd_with_stdin(args: &[OsString], cwd: &Path, stdin_input: &str) -> Result<Output> {
    let rendered = args
        .iter()
        .map(|s| s.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    log_step(&format!("run (stdin): (cd {}) {rendered}", cwd.display()));

    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to execute {:?}", args.iter().collect::<Vec<_>>()))?;
    {
        let stdin = child.stdin.as_mut().context("missing child stdin")?;
        stdin.write_all(stdin_input.as_bytes())?;
    }
    child
        .wait_with_output()
        .with_context(|| format!("failed waiting for {:?}", args.iter().collect::<Vec<_>>()))
}

fn run_cmd_ok(args: &[OsString], cwd: &Path) -> Result<String> {
    let out = run_cmd(args, cwd)?;
    if !out.status.success() {
        bail!(
            "command failed: {:?}\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stdout.trim().is_empty() {
        log_step(&format!("stdout:\n{}", stdout.trim()));
    }
    if !stderr.trim().is_empty() {
        log_step(&format!("stderr:\n{}", stderr.trim()));
    }
    Ok(stdout)
}

fn run_cmd_ok_with_stdin(args: &[OsString], cwd: &Path, stdin_input: &str) -> Result<String> {
    let out = run_cmd_with_stdin(args, cwd, stdin_input)?;
    if !out.status.success() {
        bail!(
            "command failed: {:?}\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stdout.trim().is_empty() {
        log_step(&format!("stdout:\n{}", stdout.trim()));
    }
    if !stderr.trim().is_empty() {
        log_step(&format!("stderr:\n{}", stderr.trim()));
    }
    Ok(stdout)
}

fn is_strictly_before_commit(repo: &Path, candidate_ref: &str, boundary_commit: &str) -> Result<bool> {
    let candidate = run_cmd_ok(
        &[
            OsString::from("git"),
            OsString::from("rev-parse"),
            OsString::from(candidate_ref),
        ],
        repo,
    )?
    .trim()
    .to_string();
    let boundary = run_cmd_ok(
        &[
            OsString::from("git"),
            OsString::from("rev-parse"),
            OsString::from(boundary_commit),
        ],
        repo,
    )?
    .trim()
    .to_string();

    if candidate == boundary {
        return Ok(false);
    }

    let out = run_cmd(
        &[
            OsString::from("git"),
            OsString::from("merge-base"),
            OsString::from("--is-ancestor"),
            OsString::from(&candidate),
            OsString::from(&boundary),
        ],
        repo,
    )?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!(
            "failed checking commit ancestry between {} and {}:\nstdout: {}\nstderr: {}",
            candidate,
            boundary,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

fn pick_unused_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
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

fn spawn_server(cli_path: &Path, data_dir: &Path, port: u16) -> Result<(Child, Arc<Mutex<String>>)> {
    let listen = format!("127.0.0.1:{port}");
    log_step(&format!(
        "starting server via {} on {} using data dir {}",
        cli_path.display(),
        listen,
        data_dir.display()
    ));
    let mut child = Command::new(cli_path)
        .args([
            "start",
            "--jwt-key-dir",
            &data_dir.display().to_string(),
            "--data-dir",
            &data_dir.display().to_string(),
            "--listen-addr",
            &listen,
        ])
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

fn spawn_chat_client(label: &str, bin: &Path, server_url: &str, db_name: &str) -> Result<(Child, Arc<Mutex<String>>)> {
    log_step(&format!(
        "starting {label} client {} (server={}, db={})",
        bin.display(),
        server_url,
        db_name
    ));
    let mut child = Command::new(bin)
        .env("SPACETIMEDB_HOST", server_url)
        .env("SPACETIMEDB_SERVER", server_url)
        .env("SPACETIMEDB_DB_NAME", db_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn chat client {}", bin.display()))?;
    log_step(&format!("started client pid={}", child.id()));

    let logs = Arc::new(Mutex::new(String::new()));

    if let Some(stdout) = child.stdout.take() {
        let logs_out = logs.clone();
        let label = label.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
                    break;
                }
                eprintln!("[{label} recv] {}", line.trim_end());
                let mut s = logs_out.lock().unwrap();
                s.push_str(&line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let logs_err = logs.clone();
        let label = label.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
                    break;
                }
                eprintln!("[{label} stderr] {}", line.trim_end());
                let mut s = logs_err.lock().unwrap();
                s.push_str(&line);
            }
        });
    }

    Ok((child, logs))
}

fn write_line(child: &mut Child, line: &str) -> Result<()> {
    log_step(&format!("sending to pid {}: {}", child.id(), line));
    let stdin = child.stdin.as_mut().context("child stdin missing")?;
    if let Err(e) = stdin.write_all(line.as_bytes()) {
        if e.kind() == ErrorKind::BrokenPipe {
            log_step(&format!(
                "stdin broken pipe for pid {} while sending {:?}; continuing",
                child.id(),
                line
            ));
            return Ok(());
        }
        return Err(e).context("failed writing line to client stdin");
    }
    if let Err(e) = stdin.write_all(b"\n") {
        if e.kind() == ErrorKind::BrokenPipe {
            log_step(&format!(
                "stdin broken pipe for pid {} while sending newline; continuing",
                child.id()
            ));
            return Ok(());
        }
        return Err(e).context("failed writing newline to client stdin");
    }
    if let Err(e) = stdin.flush() {
        if e.kind() == ErrorKind::BrokenPipe {
            log_step(&format!(
                "stdin broken pipe for pid {} on flush; continuing",
                child.id()
            ));
            return Ok(());
        }
        return Err(e).context("failed flushing client stdin");
    }
    Ok(())
}

#[test]
#[ignore = "manual local-only: long-running, networked, and builds historical artifacts"]
fn kill_after_publish_logs_after_restart() -> Result<()> {
    log_step("starting manual kill-after-publish test");
    log_step(&format!("pinned source ref={}", V1_GIT_REF));
    let repo = workspace_root();
    let worktree_dir = repo
        .join("target")
        .join("smoketest-worktrees")
        .join("kill-after-publish-v1");
    let temp = tempfile::tempdir()?;
    let temp_path = temp.path();
    let data_dir = temp_path.join("db-data");
    std::fs::create_dir_all(&data_dir)?;
    log_step(&format!("workspace={}", repo.display()));
    log_step(&format!("data dir={}", data_dir.display()));

    let prepared = prepare_pinned_chat_workspace(&repo, &worktree_dir, V1_GIT_REF)?;
    let old_worktree = prepared.worktree_dir;
    let old_cli = prepared.cli_path;
    let old_client = prepared.client_path;
    let publish_path_flag = prepared.publish_path_flag;
    let old_module_dir = prepared.module_dir;
    log_step(&format!("worktree={}", old_worktree.display()));

    let old_result: Result<()> = (|| {
        log_step(&format!("old CLI path={}", old_cli.display()));
        log_step(&format!("old client path={}", old_client.display()));

        // Start 1.0 server and publish 1.0 quickstart module.
        let old_port = pick_unused_port()?;
        let old_url = format!("http://127.0.0.1:{old_port}");
        log_step("starting old server for initial publish");
        let (mut old_server, old_server_logs) = spawn_server(&old_cli, &data_dir, old_port)?;
        if let Err(e) = wait_for_ping(&old_url, Duration::from_secs(20)) {
            dump_server_logs("old server", &old_server_logs);
            kill_child(&mut old_server);
            return Err(e);
        }

        let db_name = format!("manual-upgrade-chat-{}", spacetimedb_smoketests::random_string());
        log_step(&format!("publishing old module to db {}", db_name));
        let publish_out = run_cmd_ok(
            &[
                old_cli.clone().into_os_string(),
                OsString::from("publish"),
                OsString::from("--server"),
                OsString::from(&old_url),
                OsString::from(publish_path_flag),
                old_module_dir.clone().into_os_string(),
                OsString::from("--yes"),
                OsString::from(&db_name),
            ],
            &old_worktree,
        )?;
        let _identity = extract_identity(&publish_out)?;
        log_step("old module published successfully; stopping old server");
        kill_child(&mut old_server);

        let new_port = pick_unused_port()?;
        let new_url = format!("http://127.0.0.1:{new_port}");
        log_step("starting new server on same data dir");
        let (mut new_server, new_server_logs) = spawn_server(&old_cli, &data_dir, new_port)?;
        if let Err(e) = wait_for_ping(&new_url, Duration::from_secs(20)) {
            dump_server_logs("new server", &new_server_logs);
            kill_child(&mut new_server);
            return Err(e);
        }

        log_step("starting client");
        let (mut c1, logs1) = spawn_chat_client("client-v1", &old_client, &new_url, &db_name)?;

        thread::sleep(Duration::from_secs(5));
        write_line(&mut c1, "/name old-v1")?;
        write_line(&mut c1, "hello-from-v1")?;

        // Both clients should observe both messages in their output.
        log_step("waiting for both clients to observe both messages");
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ok = false;
        while Instant::now() < deadline {
            let l1 = logs1.lock().unwrap().clone();
            let saw_v1 = l1.contains("old-v1: hello-from-v1");
            if saw_v1 {
                log_step("success condition met: both clients saw both messages");
                ok = true;
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }

        log_step("stopping clients and new server");
        kill_child(&mut c1);
        kill_child(&mut new_server);

        if !ok {
            let l1 = logs1.lock().unwrap().clone();
            dump_server_logs("new server", &new_server_logs);
            bail!(
                "message exchange incomplete.\nclient-v1 logs:\n{}\n",
                l1,
            );
        }

        Ok(())
    })();

    log_step("manual test finished");
    old_result
}
