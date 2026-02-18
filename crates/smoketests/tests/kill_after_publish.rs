#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use spacetimedb_smoketests::workspace_root;

// NOTE: This test is intentionally manual/local-only and not meant for CI.
//
// It validates a kill/restart scenario at a pinned historical version:
// 1) build CLI/server from this pinned git ref in a temporary worktree
// 2) start server and publish module
// 3) restart server immediately on the same data dir and same version
// 4) verify `spacetime logs` succeeds
const V1_GIT_REF: &str = "2772036d511ab8ead00c132e484a057b1bdb6bd4";
const MODULE_PATH_FLAG_CUTOFF_COMMIT: &str = "4c962b9170c577b6e6c7afeecf05a60635fa1536";

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

#[test]
#[ignore = "manual local-only: long-running, networked, and builds historical artifacts"]
fn kill_after_publish_logs_after_restart() -> Result<()> {
    log_step("starting manual kill-after-publish test");
    log_step(&format!("pinned source ref={}", V1_GIT_REF));
    let repo = workspace_root();
    let stable_worktree_dir = repo
        .join("target")
        .join("smoketest-worktrees")
        .join("kill-after-publish-v1");
    let temp = tempfile::tempdir()?;
    let temp_path = temp.path();
    let old_worktree = stable_worktree_dir;
    let data_dir = temp_path.join("db-data");
    std::fs::create_dir_all(&data_dir)?;
    log_step(&format!("workspace={}", repo.display()));
    log_step(&format!("worktree={}", old_worktree.display()));
    log_step(&format!("temp root={}", temp_path.display()));
    log_step(&format!("data dir={}", data_dir.display()));

    // Build pinned sources from a temporary worktree.
    if !old_worktree.exists() {
        let parent = old_worktree
            .parent()
            .ok_or_else(|| anyhow!("worktree path has no parent: {}", old_worktree.display()))?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create worktree parent {}", parent.display()))?;
        log_step(&format!("creating worktree at ref {}", V1_GIT_REF));
        run_cmd_ok(
            &[
                OsString::from("git"),
                OsString::from("worktree"),
                OsString::from("add"),
                old_worktree.clone().into_os_string(),
                OsString::from(V1_GIT_REF),
            ],
            &repo,
        )?;
    } else {
        log_step("reusing existing worktree");
    }

    let old_result: Result<()> = (|| {
        log_step("building pinned spacetimedb-cli and spacetimedb-standalone");
        run_cmd_ok(
            &[
                OsString::from("cargo"),
                OsString::from("build"),
                OsString::from("-p"),
                OsString::from("spacetimedb-cli"),
                OsString::from("-p"),
                OsString::from("spacetimedb-standalone"),
            ],
            &old_worktree,
        )?;

        let old_cli = old_worktree
            .join("target")
            .join("debug")
            .join(exe_name("spacetimedb-cli"));
        anyhow::ensure!(old_cli.exists(), "pinned CLI binary not found at {}", old_cli.display());

        log_step(&format!("pinned CLI path={}", old_cli.display()));
        let publish_path_flag = if is_strictly_before_commit(&repo, V1_GIT_REF, MODULE_PATH_FLAG_CUTOFF_COMMIT)? {
            "--project-path"
        } else {
            "--module-path"
        };
        log_step(&format!(
            "using {} for publish path (cutoff {})",
            publish_path_flag, MODULE_PATH_FLAG_CUTOFF_COMMIT
        ));

        // Start pinned server and publish pinned quickstart module.
        let old_port = pick_unused_port()?;
        let old_url = format!("http://127.0.0.1:{old_port}");
        log_step("starting pinned server for initial publish");
        let (mut old_server, old_server_logs) = spawn_server(&old_cli, &data_dir, old_port)?;
        if let Err(e) = wait_for_ping(&old_url, Duration::from_secs(20)) {
            dump_server_logs("pinned server", &old_server_logs);
            kill_child(&mut old_server);
            return Err(e);
        }

        let db_name = format!("kill-after-publish-{}", spacetimedb_smoketests::random_string());
        log_step(&format!("publishing pinned module to db {}", db_name));
        run_cmd_ok(
            &[
                old_cli.clone().into_os_string(),
                OsString::from("publish"),
                OsString::from("--server"),
                OsString::from(&old_url),
                OsString::from(publish_path_flag),
                old_worktree
                    .join("templates/chat-console-rs/spacetimedb")
                    .into_os_string(),
                OsString::from("--yes"),
                OsString::from(&db_name),
            ],
            &old_worktree,
        )?;
        log_step("module published successfully; stopping server");
        kill_child(&mut old_server);

        // Restart pinned server on the same data dir and verify logs command succeeds.
        let restart_port = pick_unused_port()?;
        let restart_url = format!("http://127.0.0.1:{restart_port}");
        log_step("restarting pinned server on same data dir");
        let (mut restarted_server, restarted_server_logs) = spawn_server(&old_cli, &data_dir, restart_port)?;
        if let Err(e) = wait_for_ping(&restart_url, Duration::from_secs(20)) {
            dump_server_logs("restarted server", &restarted_server_logs);
            kill_child(&mut restarted_server);
            return Err(e);
        }

        log_step("running `spacetime logs` against restarted server");
        run_cmd_ok(
            &[
                old_cli.clone().into_os_string(),
                OsString::from("logs"),
                OsString::from("--server"),
                OsString::from(&restart_url),
                OsString::from("-n"),
                OsString::from("20"),
                OsString::from(&db_name),
            ],
            &old_worktree,
        )?;

        log_step("stopping restarted server");
        kill_child(&mut restarted_server);

        Ok(())
    })();

    log_step("manual test finished");
    old_result
}
