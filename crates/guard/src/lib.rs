#![allow(clippy::disallowed_macros)]

use std::{
    env,
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread::{self, sleep},
    time::{Duration, Instant},
};

/// Lazily-initialized path to the pre-built CLI binary.
static CLI_BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Ensures `spacetimedb-cli` and `spacetimedb-standalone` are built once,
/// returning the path to the CLI binary.
///
/// This is useful for tests that need to run CLI commands directly.
pub fn ensure_binaries_built() -> PathBuf {
    CLI_BINARY_PATH
        .get_or_init(|| {
            // Navigate from crates/guard/ to workspace root
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let workspace_root = manifest_dir
                .parent() // crates/
                .and_then(|p| p.parent()) // workspace root
                .expect("Failed to find workspace root");

            // Determine target directory
            let target_dir = env::var("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| workspace_root.join("target"));

            // Determine profile
            let profile = if cfg!(debug_assertions) { "debug" } else { "release" };

            // Build both binaries (standalone needed by CLI's start command)
            for pkg in ["spacetimedb-standalone", "spacetimedb-cli"] {
                let mut args = vec!["build", "-p", pkg];
                if profile == "release" {
                    args.push("--release");
                }

                // Clear cargo-provided env vars to avoid unnecessary rebuilds.
                // When running under `cargo test`, cargo sets env vars like
                // CARGO_ENCODED_RUSTFLAGS that differ from a normal build,
                // causing the child cargo to think it needs to recompile.
                let mut cmd = Command::new("cargo");
                cmd.args(&args).current_dir(workspace_root);
                for (key, _) in env::vars() {
                    if key.starts_with("CARGO") && key != "CARGO_HOME" && key != "CARGO_TARGET_DIR" {
                        cmd.env_remove(&key);
                    }
                }

                let status = cmd
                    .status()
                    .unwrap_or_else(|e| panic!("Failed to build {}: {}", pkg, e));

                assert!(status.success(), "Building {} failed", pkg);
            }

            // Return path to CLI binary
            let cli_name = if cfg!(windows) {
                "spacetimedb-cli.exe"
            } else {
                "spacetimedb-cli"
            };
            let cli_path = target_dir.join(profile).join(cli_name);

            assert!(cli_path.exists(), "CLI binary not found at {}", cli_path.display());

            cli_path
        })
        .clone()
}

use reqwest::blocking::Client;

pub struct SpacetimeDbGuard {
    pub child: Child,
    pub host_url: String,
    pub logs: Arc<Mutex<String>>,
    /// The PostgreSQL wire protocol port, if enabled.
    pub pg_port: Option<u16>,
    /// The data directory path (for restart scenarios).
    pub data_dir: PathBuf,
    /// Owns the temporary data directory (if created by spawn_in_temp_data_dir).
    /// When this is Some, dropping the guard will clean up the temp dir.
    data_dir_handle: Option<tempfile::TempDir>,
}

// Remove all Cargo-provided env vars from a child process. These are set by the fact that we're running in a cargo
// command (e.g. `cargo test`). We don't want to inherit any of these to a child cargo process, because it causes
// unnecessary rebuilds.
impl SpacetimeDbGuard {
    /// Start `spacetimedb` in a temporary data directory via:
    /// cargo run -p spacetimedb-cli -- start --data-dir <temp-dir> --listen-addr <addr>
    pub fn spawn_in_temp_data_dir() -> Self {
        Self::spawn_in_temp_data_dir_with_pg_port(None)
    }

    /// Start `spacetimedb` in a temporary data directory with optional PostgreSQL wire protocol.
    pub fn spawn_in_temp_data_dir_with_pg_port(pg_port: Option<u16>) -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir_path = temp_dir.path().to_path_buf();
        let data_dir_str = data_dir_path.display().to_string();

        Self::spawn_spacetime_start_with_data_dir(
            false,
            &["start", "--data-dir", &data_dir_str],
            pg_port,
            data_dir_path,
            Some(temp_dir),
        )
    }

    /// Start `spacetimedb` in a temporary data directory via:
    /// spacetime start --data-dir <temp-dir> --listen-addr <addr>
    pub fn spawn_in_temp_data_dir_use_cli() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir_path = temp_dir.path().to_path_buf();
        let data_dir_str = data_dir_path.display().to_string();

        Self::spawn_spacetime_start_with_data_dir(
            true,
            &["start", "--data-dir", &data_dir_str],
            None,
            data_dir_path,
            Some(temp_dir),
        )
    }

    /// Start `spacetimedb` with an explicit data directory (for restart scenarios).
    ///
    /// Unlike `spawn_in_temp_data_dir`, this method does not create a temporary directory.
    /// The caller is responsible for managing the data directory lifetime.
    pub fn spawn_with_data_dir(data_dir: PathBuf, pg_port: Option<u16>) -> Self {
        let data_dir_str = data_dir.display().to_string();
        Self::spawn_spacetime_start_with_data_dir(
            false,
            &["start", "--data-dir", &data_dir_str],
            pg_port,
            data_dir,
            None,
        )
    }

    fn spawn_spacetime_start_with_data_dir(
        use_installed_cli: bool,
        _extra_args: &[&str],
        pg_port: Option<u16>,
        data_dir: PathBuf,
        data_dir_handle: Option<tempfile::TempDir>,
    ) -> Self {
        if use_installed_cli {
            // Use the installed CLI (rare case, mainly for spawn_in_temp_data_dir_use_cli)
            let address = "127.0.0.1:0".to_string();
            let data_dir_str = data_dir.display().to_string();

            let args = ["start", "--data-dir", &data_dir_str, "--listen-addr", &address];
            let cmd = Command::new("spacetime");
            let (child, logs) = Self::spawn_child(cmd, env!("CARGO_MANIFEST_DIR"), &args);

            let listen_addr = wait_for_listen_addr(&logs, Duration::from_secs(10))
                .unwrap_or_else(|| panic!("Timed out waiting for SpacetimeDB to report listen address"));
            let host_url = format!("http://{}", listen_addr);
            let guard = SpacetimeDbGuard {
                child,
                host_url,
                logs,
                pg_port,
                data_dir,
                data_dir_handle,
            };
            guard.wait_until_http_ready(Duration::from_secs(10));
            guard
        } else {
            // Use the built CLI (common case)
            let (child, logs, host_url) = Self::spawn_server(&data_dir, pg_port);
            SpacetimeDbGuard {
                child,
                host_url,
                logs,
                pg_port,
                data_dir,
                data_dir_handle,
            }
        }
    }

    /// Stop the server process without dropping the guard.
    ///
    /// This kills the server process but preserves the data directory.
    /// Use `restart()` to start the server again with the same data.
    pub fn stop(&mut self) {
        self.kill_process();
    }

    /// Restart the server with the same data directory.
    ///
    /// This stops the current server process and starts a new one
    /// with the same data directory, preserving all data.
    pub fn restart(&mut self) {
        self.stop();

        let (child, logs, host_url) = Self::spawn_server(&self.data_dir, self.pg_port);

        self.child = child;
        self.logs = logs;
        self.host_url = host_url;
    }

    /// Kills the current server process and waits for it to exit.
    fn kill_process(&mut self) {
        // Kill the process tree to ensure all child processes are terminated.
        // On Windows, child.kill() only kills the direct child (spacetimedb-cli),
        // leaving spacetimedb-standalone running as an orphan.
        #[cfg(windows)]
        {
            let pid = self.child.id();
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(windows))]
        {
            let _ = self.child.kill();
        }

        let _ = self.child.wait();
    }

    /// Spawns a new server process with the given data directory.
    /// Returns (child, logs, host_url).
    fn spawn_server(data_dir: &PathBuf, pg_port: Option<u16>) -> (Child, Arc<Mutex<String>>, String) {
        let data_dir_str = data_dir.display().to_string();
        let pg_port_str = pg_port.map(|p| p.to_string());

        let address = "127.0.0.1:0".to_string();
        let cli_path = ensure_binaries_built();

        let mut args = vec!["start", "--data-dir", &data_dir_str, "--listen-addr", &address];
        if let Some(ref port) = pg_port_str {
            args.extend(["--pg-port", port]);
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("Failed to find workspace root");

        let cmd = Command::new(&cli_path);
        let (child, logs) = Self::spawn_child(cmd, workspace_root.to_str().unwrap(), &args);

        // Wait for the server to be ready
        let listen_addr = wait_for_listen_addr(&logs, Duration::from_secs(10))
            .unwrap_or_else(|| panic!("Timed out waiting for SpacetimeDB to report listen address"));
        let host_url = format!("http://{}", listen_addr);

        // Wait until HTTP is ready
        let client = Client::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let url = format!("{}/v1/ping", host_url);
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return (child, logs, host_url);
                }
            }
            sleep(Duration::from_millis(50));
        }
        panic!("Timed out waiting for SpacetimeDB HTTP /v1/ping at {}", host_url);
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
        self.kill_process();

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
