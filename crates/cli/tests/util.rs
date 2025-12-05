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

fn find_free_port() -> u16 {
    portpicker::pick_unused_port().expect("no free ports available")
}

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

        Self::spawn_spacetime_start(&["start", "--data-dir", &data_dir])
    }

    fn spawn_spacetime_start(extra_args: &[&str]) -> Self {
        let port = find_free_port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let address = addr.to_string();
        let host_url = format!("http://{}", addr);

        // Workspace root for `cargo run -p ...`
        let workspace_dir = env!("CARGO_MANIFEST_DIR");

        Self::build_prereqs(workspace_dir);
        let mut cargo_args = vec!["run", "-p", "spacetimedb-cli", "--"];

        cargo_args.extend(extra_args);
        cargo_args.extend(["--listen-addr", &address]);

        let (child, logs) = Self::spawn_child(workspace_dir, &cargo_args);

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

    fn spawn_child(workspace_dir: &str, args: &[&str]) -> (Child, Arc<Mutex<String>>) {
        let mut cmd = Command::new("cargo");
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

            if let Ok(resp) = client.get(&url).send()
                && resp.status().is_success()
            {
                return; // Fully ready!
            }

            sleep(Duration::from_millis(50));
        }
        panic!("Timed out waiting for SpacetimeDB HTTP /v1/ping at {}", self.host_url);
    }
}

impl Drop for SpacetimeDbGuard {
    fn drop(&mut self) {
        // Best-effort cleanup.
        let _ = self.child.kill();
        let _ = self.child.wait();

        // Only print logs if the test is currently panicking
        if std::thread::panicking()
            && let Ok(logs) = self.logs.lock()
        {
            eprintln!("\n===== SpacetimeDB child logs (only on failure) =====\n{}\n====================================================", *logs);
        }
    }
}
