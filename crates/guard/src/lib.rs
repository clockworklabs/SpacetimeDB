#![allow(clippy::disallowed_macros)]

use std::{
    env,
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::{Duration, Instant},
};

use reqwest::blocking::Client;

pub struct SpacetimeDbGuard {
    pub child: Child,
    pub host_url: String,
    pub logs: Arc<Mutex<String>>,
}

// Remove all Cargo-provided env vars from a child process. These are set by the fact that we're running in a cargo
// command (e.g. `cargo test`). We don't want to inherit any of these to a child cargo process, because it causes
// unnecessary rebuilds.
impl SpacetimeDbGuard {
    /// Start `spacetimedb` in a temporary data directory via:
    /// cargo run -p spacetimedb-cli -- start --data-dir <temp-dir> --listen-addr <addr>
    pub fn spawn_in_temp_data_dir() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir = temp_dir.path().display().to_string();

        Self::spawn_spacetime_start(false, &["start", "--data-dir", &data_dir])
    }

    /// Start `spacetimedb` in a temporary data directory via:
    /// spacetime start --data-dir <temp-dir> --listen-addr <addr>
    pub fn spawn_in_temp_data_dir_use_cli() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir = temp_dir.path().display().to_string();

        Self::spawn_spacetime_start(true, &["start", "--data-dir", &data_dir])
    }

    fn spawn_spacetime_start(use_installed_cli: bool, extra_args: &[&str]) -> Self {
        // Ask SpacetimeDB/OS to allocate an ephemeral port.
        // Using loopback avoids needing to "connect to 0.0.0.0".
        let address = "127.0.0.1:0".to_string();

        // Workspace root for `cargo run -p ...`
        let workspace_dir = env!("CARGO_MANIFEST_DIR");

        let mut args = vec![];

        let (child, logs) = if use_installed_cli {
            args.extend_from_slice(extra_args);
            args.extend_from_slice(&["--listen-addr", &address]);

            let cmd = Command::new("spacetime");
            Self::spawn_child(cmd, env!("CARGO_MANIFEST_DIR"), &args)
        } else {
            Self::build_prereqs(workspace_dir);
            args.extend(vec!["run", "-p", "spacetimedb-cli", "--"]);
            args.extend(extra_args);
            args.extend(["--listen-addr", &address]);

            let cmd = Command::new("cargo");
            Self::spawn_child(cmd, workspace_dir, &args)
        };

        // Parse the actual bound address from logs.
        let listen_addr = wait_for_listen_addr(&logs, Duration::from_secs(10))
            .unwrap_or_else(|| panic!("Timed out waiting for SpacetimeDB to report listen address"));
        let host_url = format!("http://{}", listen_addr);
        let guard = SpacetimeDbGuard { child, host_url, logs };
        guard.wait_until_http_ready(Duration::from_secs(10));
        guard
    }

    // Ensure standalone is built before we start, if that’s needed.
    // This is best-effort and usually a no-op when already built.
    // Also build the CLI before running it to avoid that being included in the
    // timeout for readiness.
    fn build_prereqs(workspace_dir: &str) {
        let targets = ["spacetimedb-standalone", "spacetimedb-cli"];

        for pkg in targets {
            let mut cmd = Command::new("cargo");
            let _ = cmd
                .args(["build", "-p", pkg])
                .current_dir(workspace_dir)
                .status()
                .unwrap_or_else(|_| panic!("failed to build {}", pkg));
        }
    }

    fn spawn_child(mut cmd: Command, workspace_dir: &str, args: &[&str]) -> (Child, Arc<Mutex<String>>) {
        let mut child = cmd
            .args(args)
            .current_dir(workspace_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn spacetimedb-cli");

        let logs = Arc::new(Mutex::new(String::new()));

        // Attach stdout logger
        if let Some(stdout) = child.stdout.take() {
            let logs_clone = logs.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    let mut buf = logs_clone.lock().unwrap();
                    buf.push_str("[STDOUT] ");
                    buf.push_str(&line);
                    buf.push('\n');
                }
            });
        }

        // Attach stderr logger
        if let Some(stderr) = child.stderr.take() {
            let logs_clone = logs.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    let mut buf = logs_clone.lock().unwrap();
                    buf.push_str("[STDERR] ");
                    buf.push_str(&line);
                    buf.push('\n');
                }
            });
        }

        (child, logs)
    }

    fn wait_until_http_ready(&self, timeout: Duration) {
        let client = Client::new();
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            let url = format!("{}/v1/ping", self.host_url);

            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return; // Fully ready!
                }
            }

            sleep(Duration::from_millis(50));
        }
        panic!("Timed out waiting for SpacetimeDB HTTP /v1/ping at {}", self.host_url);
    }
}

/// Wait for a line like:
/// "... Starting SpacetimeDB listening on 0.0.0.0:24326"
fn wait_for_listen_addr(logs: &Arc<Mutex<String>>, timeout: Duration) -> Option<SocketAddr> {
    let deadline = Instant::now() + timeout;
    let mut cursor = 0usize;

    while Instant::now() < deadline {
        let (new_text, new_len) = {
            let buf = logs.lock().unwrap();
            if cursor >= buf.len() {
                (String::new(), buf.len())
            } else {
                (buf[cursor..].to_string(), buf.len())
            }
        };
        cursor = new_len;

        for line in new_text.lines() {
            if let Some(addr) = parse_listen_addr_from_line(line) {
                return Some(addr);
            }
        }

        sleep(Duration::from_millis(25));
    }

    None
}

fn parse_listen_addr_from_line(line: &str) -> Option<SocketAddr> {
    const PREFIX: &str = "Starting SpacetimeDB listening on ";

    let i = line.find(PREFIX)?;
    let rest = line[i + PREFIX.len()..].trim();

    // Next token should be the socket address (e.g. "0.0.0.0:24326" or "[::]:24326")
    let token = rest.split_whitespace().next()?;
    token.parse::<SocketAddr>().ok()
}

impl Drop for SpacetimeDbGuard {
    fn drop(&mut self) {
        // Best-effort cleanup.
        let _ = self.child.kill();
        let _ = self.child.wait();

        // Only print logs if the test is currently panicking
        if std::thread::panicking() {
            if let Ok(logs) = self.logs.lock() {
                eprintln!(
                    "\n===== SpacetimeDB child logs (only on failure) =====\n{}\n====================================================",
                    *logs
                );
            }
        }
    }
}
