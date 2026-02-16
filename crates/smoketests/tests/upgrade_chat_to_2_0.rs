#![allow(clippy::disallowed_macros)]

use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use spacetimedb_smoketests::workspace_root;

// NOTE: This test is intentionally manual/local-only and not meant for CI.
//
// It validates a 1.0 -> 2.0 upgrade scenario using quickstart-chat:
// 1) install a 1.0 CLI via `spacetime version install`
// 2) build 1.0 server/client/module from this pinned git ref
// 3) start 1.0 server and publish module
// 4) restart as 2.0 server on the same data dir
// 5) run both 1.0 and 2.0 quickstart clients, exchange messages, assert both observed
const V1_GIT_REF: &str = "v1.12.0";
const V1_RELEASE_VERSION: &str = "1.12.0";

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
fn upgrade_chat_to_2_0_mixed_clients() -> Result<()> {
    log_step("starting manual upgrade mixed-clients quickstart test");
    log_step(&format!(
        "pinned old source ref={}, old release version={}",
        V1_GIT_REF, V1_RELEASE_VERSION
    ));
    let repo = workspace_root();
    let temp = tempfile::tempdir()?;
    let temp_path = temp.path();
    let old_worktree = temp_path.join("old");
    let data_dir = temp_path.join("db-data");
    std::fs::create_dir_all(&data_dir)?;
    log_step(&format!("workspace={}", repo.display()));
    log_step(&format!("temp root={}", temp_path.display()));
    log_step(&format!("data dir={}", data_dir.display()));

    // Prepare current binaries (including update helper needed by `version install` from target dir).
    log_step("building current spacetimedb-cli and spacetimedb-update");
    run_cmd_ok(
        &[
            OsString::from("cargo"),
            OsString::from("build"),
            OsString::from("-p"),
            OsString::from("spacetimedb-cli"),
            OsString::from("-p"),
            OsString::from("spacetimedb-standalone"),
            OsString::from("-p"),
            OsString::from("spacetimedb-update"),
        ],
        &repo,
    )?;

    let current_cli = repo.join("target").join("debug").join(exe_name("spacetimedb-cli"));
    anyhow::ensure!(
        current_cli.exists(),
        "current CLI binary missing at {}",
        current_cli.display()
    );

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

    let cleanup_worktree = || {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&old_worktree)
            .current_dir(&repo)
            .status();
    };

    let old_result: Result<()> = (|| {
        log_step("running v1 codegen for chat-console-rs (cli generate)");
        run_cmd_ok(
            &[
                installed_v1_cli.clone().into_os_string(),
                OsString::from("generate"),
                OsString::from("--project-path"),
                OsString::from("spacetimedb/"),
                OsString::from("--out-dir"),
                OsString::from("src/module_bindings/"),
                OsString::from("--lang"),
                OsString::from("rust"),
            ],
            &old_worktree.join("templates/chat-console-rs"),
        )?;

        log_step("building old quickstart chat client");
        run_cmd_ok(
            &[OsString::from("cargo"), OsString::from("build")],
            &old_worktree.join("templates/chat-console-rs"),
        )?;

        run_cmd_ok(
            &[
                current_cli.clone().into_os_string(),
                OsString::from("generate"),
                OsString::from("--module-path"),
                OsString::from("spacetimedb/"),
                OsString::from("--out-dir"),
                OsString::from("src/module_bindings/"),
                OsString::from("--lang"),
                OsString::from("rust"),
            ],
            &repo.join("templates/chat-console-rs"),
        )?;

        log_step("building current quickstart chat client");
        run_cmd_ok(
            &[OsString::from("cargo"), OsString::from("build")],
            &repo.join("templates/chat-console-rs"),
        )?;

        let old_client = old_worktree
            .join("templates/chat-console-rs")
            .join("target")
            .join("debug")
            .join(exe_name("rust-quickstart-chat"));
        anyhow::ensure!(
            old_client.exists(),
            "old chat client not found at {}",
            old_client.display()
        );
        let new_client = repo
            .join("templates/chat-console-rs")
            .join("target")
            .join("debug")
            .join(exe_name("rust-quickstart-chat"));
        anyhow::ensure!(
            new_client.exists(),
            "new chat client not found at {}",
            new_client.display()
        );

        log_step(&format!("v1 CLI path={}", installed_v1_cli.display()));
        log_step(&format!("old client path={}", old_client.display()));
        log_step(&format!("new client path={}", new_client.display()));

        // Start 1.0 server and publish 1.0 quickstart module.
        let old_port = pick_unused_port()?;
        let old_url = format!("http://127.0.0.1:{old_port}");
        log_step("starting old server for initial publish");
        let (mut old_server, old_server_logs) = spawn_server(&installed_v1_cli, &data_dir, old_port)?;
        if let Err(e) = wait_for_ping(&old_url, Duration::from_secs(20)) {
            dump_server_logs("old server", &old_server_logs);
            kill_child(&mut old_server);
            return Err(e);
        }

        let db_name = format!("manual-upgrade-chat-{}", spacetimedb_smoketests::random_string());
        log_step(&format!("publishing old module to db {}", db_name));
        let publish_out = run_cmd_ok(
            &[
                installed_v1_cli.clone().into_os_string(),
                OsString::from("publish"),
                OsString::from("--server"),
                OsString::from(&old_url),
                OsString::from("--project-path"),
                old_worktree
                    .join("templates/chat-console-rs/spacetimedb")
                    .into_os_string(),
                OsString::from("--yes"),
                OsString::from(&db_name),
            ],
            &old_worktree,
        )?;
        let _identity = extract_identity(&publish_out)?;
        log_step("old module published successfully; stopping old server");
        kill_child(&mut old_server);

        // Start 2.0 server on the same data dir.
        let new_port = pick_unused_port()?;
        let new_url = format!("http://127.0.0.1:{new_port}");
        log_step("starting new server on same data dir");
        let (mut new_server, new_server_logs) = spawn_server(&current_cli, &data_dir, new_port)?;
        if let Err(e) = wait_for_ping(&new_url, Duration::from_secs(20)) {
            dump_server_logs("new server", &new_server_logs);
            kill_child(&mut new_server);
            return Err(e);
        }
        log_step("publishing HEAD module to same database name on 2.0 server");
        run_cmd_ok_with_stdin(
            &[
                current_cli.clone().into_os_string(),
                OsString::from("publish"),
                OsString::from("--server"),
                OsString::from(&new_url),
                OsString::from("--module-path"),
                repo.join("templates/chat-console-rs/spacetimedb").into_os_string(),
                OsString::from("--yes"),
                OsString::from(&db_name),
            ],
            &repo,
            "upgrade\n",
        )?;

        // Spawn 1.0 and 2.0 quickstart clients against the upgraded 2.0 server.
        log_step("starting old and new clients");
        let (mut c1, logs1) = spawn_chat_client("client-v1", &old_client, &new_url, &db_name)?;
        let (mut c2, logs2) = spawn_chat_client("client-v2", &new_client, &new_url, &db_name)?;

        thread::sleep(Duration::from_secs(2));
        write_line(&mut c1, "/name old-v1")?;
        write_line(&mut c2, "/name new-v2")?;
        write_line(&mut c1, "hello-from-v1")?;
        write_line(&mut c2, "hello-from-v2")?;

        // Both clients should observe both messages in their output.
        log_step("waiting for both clients to observe both messages");
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ok = false;
        while Instant::now() < deadline {
            let l1 = logs1.lock().unwrap().clone();
            let l2 = logs2.lock().unwrap().clone();
            let saw_v1 = l1.contains("old-v1: hello-from-v1") && l2.contains("old-v1: hello-from-v1");
            let saw_v2 = l1.contains("new-v2: hello-from-v2") && l2.contains("new-v2: hello-from-v2");
            if saw_v1 && saw_v2 {
                log_step("success condition met: both clients saw both messages");
                ok = true;
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }

        log_step("stopping clients and new server");
        kill_child(&mut c1);
        kill_child(&mut c2);
        kill_child(&mut new_server);

        if !ok {
            let l1 = logs1.lock().unwrap().clone();
            let l2 = logs2.lock().unwrap().clone();
            bail!(
                "message exchange incomplete.\nclient-v1 logs:\n{}\n\nclient-v2 logs:\n{}",
                l1,
                l2
            );
        }

        Ok(())
    })();

    log_step("cleaning up temporary git worktree");
    cleanup_worktree();
    log_step("manual test finished");
    old_result
}
