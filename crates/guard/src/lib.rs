#![allow(clippy::disallowed_macros)]

use std::{
    env,
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, sleep},
    time::{Duration, Instant},
};

/// Global counter for spawn IDs to correlate log messages across threads.
static SPAWN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_spawn_id() -> u64 {
    SPAWN_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Returns the workspace root directory.
// TODO: Should this use something like `git rev-parse --show-toplevel` to avoid being directory-relative? Or perhaps `CARGO_WORKSPACE_DIR` is set?
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .expect("Failed to find workspace root")
        .to_path_buf()
}

/// Returns the target directory.
fn target_dir() -> PathBuf {
    let workspace_root = workspace_root();
    env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"))
}

/// Returns the expected CLI binary path.
fn cli_binary_path() -> PathBuf {
    // Use CARGO_BUILD_PROFILE if set, otherwise default to release for backwards compatibility
    let profile = env::var("CARGO_BUILD_PROFILE")
        .or_else(|_| env::var("PROFILE"))
        .unwrap_or_else(|_| "release".to_string());
    let cli_name = if cfg!(windows) {
        "spacetimedb-cli.exe"
    } else {
        "spacetimedb-cli"
    };
    target_dir().join(profile).join(cli_name)
}

/// Lazily-initialized path to the pre-built CLI binary.
static CLI_BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Returns the path to the pre-built CLI binary.
///
/// **This function does NOT build anything.** The binary must already exist.
/// Use `cargo smoketest` to build binaries before running tests.
///
/// # Panics
///
/// Panics if the binary does not exist.
pub fn ensure_binaries_built() -> PathBuf {
    CLI_BINARY_PATH
        .get_or_init(|| {
            let cli_path = cli_binary_path();

            if !cli_path.exists() {
                panic!(
                    "\n\
                    ========================================================================\n\
                    ERROR: CLI binary not found at {}\n\
                    \n\
                    Smoketests require pre-built binaries. Run:\n\
                    \n\
                    cargo smoketest\n\
                    \n\
                    Or build manually:\n\
                    \n\
                    cargo build -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests\n\
                    ========================================================================\n",
                    cli_path.display()
                );
            }

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
    _data_dir_handle: Option<tempfile::TempDir>,
    /// Reader thread handles for stdout/stderr - joined on drop to prevent leaks.
    reader_threads: Vec<thread::JoinHandle<()>>,
    use_installed_cli: bool,
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

    /// Start a temporary server, returning startup errors after cleaning up the child.
    pub fn try_spawn_in_temp_data_dir() -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let data_dir = temp_dir.path().to_path_buf();
        Self::spawn_spacetime_start_with_data_dir(false, None, data_dir, Some(temp_dir))
    }

    /// Start `spacetimedb` in a temporary data directory with optional PostgreSQL wire protocol.
    pub fn spawn_in_temp_data_dir_with_pg_port(pg_port: Option<u16>) -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir_path = temp_dir.path().to_path_buf();

        Self::spawn_spacetime_start_with_data_dir(false, pg_port, data_dir_path, Some(temp_dir))
            .expect("failed to start SpacetimeDB")
    }

    /// Start `spacetimedb` in a temporary data directory via:
    /// spacetime start --data-dir <temp-dir> --listen-addr <addr>
    pub fn spawn_in_temp_data_dir_use_cli() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir_path = temp_dir.path().to_path_buf();

        Self::spawn_spacetime_start_with_data_dir(true, None, data_dir_path, Some(temp_dir))
            .expect("failed to start SpacetimeDB")
    }

    /// Start `spacetimedb` with an explicit data directory (for restart scenarios).
    ///
    /// Unlike `spawn_in_temp_data_dir`, this method does not create a temporary directory.
    /// The caller is responsible for managing the data directory lifetime.
    pub fn spawn_with_data_dir(data_dir: PathBuf, pg_port: Option<u16>) -> Self {
        Self::spawn_spacetime_start_with_data_dir(false, pg_port, data_dir, None).expect("failed to start SpacetimeDB")
    }

    fn spawn_spacetime_start_with_data_dir(
        use_installed_cli: bool,
        pg_port: Option<u16>,
        data_dir: PathBuf,
        _data_dir_handle: Option<tempfile::TempDir>,
    ) -> std::io::Result<Self> {
        let spawn_id = next_spawn_id();
        let (child, logs, host_url, reader_threads) =
            Self::spawn_server(&data_dir, pg_port, spawn_id, use_installed_cli)?;
        Ok(SpacetimeDbGuard {
            child,
            host_url,
            logs,
            pg_port,
            data_dir,
            _data_dir_handle,
            reader_threads,
            use_installed_cli,
        })
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
        let spawn_id = next_spawn_id();
        let old_pid = self.child.id();
        eprintln!("[RESTART-{:03}] Starting restart, old pid={}", spawn_id, old_pid);

        self.stop();
        eprintln!("[RESTART-{:03}] Old process stopped, sleeping 100ms", spawn_id);

        // Brief pause to ensure system resources are fully released
        sleep(Duration::from_millis(100));

        eprintln!("[RESTART-{:03}] Spawning new server", spawn_id);
        let (child, logs, host_url, reader_threads) =
            Self::spawn_server(&self.data_dir, self.pg_port, spawn_id, self.use_installed_cli)
                .expect("failed to restart SpacetimeDB");
        eprintln!(
            "[RESTART-{:03}] New server ready, pid={}, url={}",
            spawn_id,
            child.id(),
            host_url
        );
        // Ensure logs from the killed server are retained.
        {
            let mut new_logs = logs.lock().unwrap();
            let old_logs = self.logs.lock().unwrap();
            new_logs.insert_str(0, old_logs.as_str());
        }

        self.child = child;
        self.logs = logs;
        self.host_url = host_url;
        self.reader_threads = reader_threads;
    }

    /// Kills the current server process and waits for it to exit.
    fn kill_process(&mut self) {
        Self::kill_child(&mut self.child, &mut self.reader_threads);
    }

    fn kill_child(child: &mut Child, reader_threads: &mut Vec<thread::JoinHandle<()>>) {
        let pid = child.id();
        eprintln!("[KILL] Killing process tree for pid={}", pid);

        // Kill the process tree to ensure all child processes are terminated.
        // On Windows, child.kill() only kills the direct child (spacetimedb-cli),
        // leaving spacetimedb-standalone running as an orphan.
        #[cfg(windows)]
        {
            let status = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            eprintln!("[KILL] taskkill result for pid={}: {:?}", pid, status);
        }

        #[cfg(not(windows))]
        {
            let result = child.kill();
            eprintln!("[KILL] kill result for pid={}: {:?}", pid, result);
        }

        let wait_result = child.wait();
        eprintln!("[KILL] wait() result for pid={}: {:?}", pid, wait_result);

        // Join reader threads to prevent leaks.
        // The threads will exit naturally once the process is killed and pipes close.
        let threads = std::mem::take(reader_threads);
        for handle in threads {
            let _ = handle.join();
        }
        eprintln!("[KILL] Reader threads joined for pid={}", pid);
    }

    /// Spawns a new server process with the given data directory.
    /// Returns (child, logs, host_url, reader_threads).
    #[allow(clippy::type_complexity)]
    fn spawn_server(
        data_dir: &Path,
        pg_port: Option<u16>,
        spawn_id: u64,
        use_installed_cli: bool,
    ) -> std::io::Result<(Child, Arc<Mutex<String>>, String, Vec<thread::JoinHandle<()>>)> {
        let cmd = if use_installed_cli {
            eprintln!("[SPAWN-{:03}] START Using installed CLI", spawn_id);
            Command::new("spacetime")
        } else {
            // Use the built CLI (common case)
            let cli_path = cli_binary_path();
            Command::new(&cli_path)
        };
        eprintln!(
            "[SPAWN-{:03}] START data_dir={:?} pg_port={:?}",
            spawn_id, data_dir, pg_port
        );

        let data_dir_str = data_dir.display().to_string();
        let pg_port_str = pg_port.map(|p| p.to_string());

        let address = "127.0.0.1:0".to_string();

        let mut args = vec![
            "start",
            "--jwt-key-dir",
            &data_dir_str,
            "--data-dir",
            &data_dir_str,
            "--listen-addr",
            &address,
        ];
        if let Some(ref port) = pg_port_str {
            args.extend(["--pg-port", port]);
        }

        eprintln!("[SPAWN-{:03}] Spawning child process", spawn_id);
        let (mut child, logs, mut reader_threads) = Self::spawn_child(cmd, &args, spawn_id)?;
        eprintln!("[SPAWN-{:03}] Child spawned pid={}", spawn_id, child.id());
        let host_url = Self::finish_server_start(
            &mut child,
            &logs,
            &mut reader_threads,
            spawn_id,
            Duration::from_secs(10),
        )?;
        Ok((child, logs, host_url, reader_threads))
    }

    fn finish_server_start(
        child: &mut Child,
        logs: &Arc<Mutex<String>>,
        reader_threads: &mut Vec<thread::JoinHandle<()>>,
        spawn_id: u64,
        timeout: Duration,
    ) -> std::io::Result<String> {
        let result = (|| {
            let listen_addr = wait_for_listen_addr(logs, timeout, spawn_id).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "SpacetimeDB did not report its listen address",
                )
            })?;
            let host_url = format!("http://{listen_addr}");
            let client = Client::builder()
                .timeout(timeout)
                .build()
                .map_err(std::io::Error::other)?;
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                if let Ok(resp) = client
                    .get(format!("{host_url}/v1/ping"))
                    .timeout(deadline.saturating_duration_since(Instant::now()))
                    .send()
                    && resp.status().is_success()
                {
                    eprintln!("[SPAWN-{:03}] HTTP ready at {}", spawn_id, host_url);
                    return Ok(host_url);
                }
                sleep(Duration::from_millis(50));
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("SpacetimeDB HTTP /v1/ping not ready at {host_url}"),
            ))
        })();
        if result.is_err() {
            Self::kill_child(child, reader_threads);
            if let Ok(logs) = logs.lock() {
                eprintln!("[SPAWN-{spawn_id:03}] startup failed; server logs:\n{logs}");
            }
        }
        result
    }

    #[allow(clippy::type_complexity)]
    fn spawn_child(
        mut cmd: Command,
        args: &[&str],
        spawn_id: u64,
    ) -> std::io::Result<(Child, Arc<Mutex<String>>, Vec<thread::JoinHandle<()>>)> {
        eprintln!("[SPAWN-{:03}] spawn_child: about to spawn", spawn_id);

        let mut child = cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

        let pid = child.id();
        eprintln!("[SPAWN-{:03}] spawn_child: spawned pid={}", spawn_id, pid);

        let logs = Arc::new(Mutex::new(String::new()));
        let mut reader_threads = Vec::new();

        // Attach stdout logger with diagnostic logging
        if let Some(stdout) = child.stdout.take() {
            let logs_clone = logs.clone();
            let handle = thread::spawn(move || {
                eprintln!("[READER-{:03}] stdout reader started for pid={}", spawn_id, pid);
                let reader = BufReader::new(stdout);
                let mut line_count = 0;
                for line in reader.lines().map_while(Result::ok) {
                    line_count += 1;
                    // Log the first few lines and any line containing the listen address
                    if line_count <= 5 || line.contains("Starting SpacetimeDB") {
                        eprintln!("[READER-{:03}] stdout line {}: {:.100}", spawn_id, line_count, line);
                    }
                    let mut buf = logs_clone.lock().unwrap();
                    buf.push_str("[STDOUT] ");
                    buf.push_str(&line);
                    buf.push('\n');
                }
                eprintln!(
                    "[READER-{:03}] stdout reader ended, {} lines total",
                    spawn_id, line_count
                );
            });
            reader_threads.push(handle);
        }

        // Attach stderr logger with diagnostic logging
        if let Some(stderr) = child.stderr.take() {
            let logs_clone = logs.clone();
            let handle = thread::spawn(move || {
                eprintln!("[READER-{:03}] stderr reader started for pid={}", spawn_id, pid);
                let reader = BufReader::new(stderr);
                let mut line_count = 0;
                for line in reader.lines().map_while(Result::ok) {
                    line_count += 1;
                    // Log the first few lines and any errors
                    if line_count <= 5 || line.contains("error") || line.contains("Error") {
                        eprintln!("[READER-{:03}] stderr line {}: {:.100}", spawn_id, line_count, line);
                    }
                    let mut buf = logs_clone.lock().unwrap();
                    buf.push_str("[STDERR] ");
                    buf.push_str(&line);
                    buf.push('\n');
                }
                eprintln!(
                    "[READER-{:03}] stderr reader ended, {} lines total",
                    spawn_id, line_count
                );
            });
            reader_threads.push(handle);
        }

        eprintln!("[SPAWN-{:03}] spawn_child: readers attached", spawn_id);
        Ok((child, logs, reader_threads))
    }
}

/// Wait for a line like:
/// "... Starting SpacetimeDB listening on 0.0.0.0:24326"
fn wait_for_listen_addr(logs: &Arc<Mutex<String>>, timeout: Duration, spawn_id: u64) -> Option<SocketAddr> {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut last_len = 0;
    let mut last_report = Instant::now();

    while Instant::now() < deadline {
        // Always search the entire log buffer to avoid missing lines that
        // might be split across multiple reader iterations.
        let buf = logs.lock().unwrap().clone();

        for line in buf.lines() {
            if let Some(addr) = parse_listen_addr_from_line(line) {
                eprintln!("[SPAWN-{:03}] Found listen addr after {:?}", spawn_id, start.elapsed());
                return Some(addr);
            }
        }

        // Progress report every 2 seconds
        let current_len = buf.len();
        if last_report.elapsed() > Duration::from_secs(2) {
            let delta = current_len.saturating_sub(last_len);
            eprintln!(
                "[SPAWN-{:03}] Waiting: {} bytes (+{}), {} lines, {:?} elapsed",
                spawn_id,
                current_len,
                delta,
                buf.lines().count(),
                start.elapsed()
            );
            last_len = current_len;
            last_report = Instant::now();
        }

        sleep(Duration::from_millis(25));
    }

    // Debug output on timeout
    let buf = logs.lock().unwrap().clone();
    eprintln!(
        "[SPAWN-{:03}] wait_for_listen_addr TIMEOUT: {} bytes, {} lines, elapsed {:?}",
        spawn_id,
        buf.len(),
        buf.lines().count(),
        start.elapsed()
    );
    eprintln!(
        "[SPAWN-{:03}] Contains 'Starting SpacetimeDB': {}",
        spawn_id,
        buf.contains("Starting SpacetimeDB")
    );
    // Show first 500 chars
    let preview: String = buf.chars().take(500).collect();
    eprintln!("[SPAWN-{:03}] First 500 chars: {:?}", spawn_id, preview);

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
        if std::thread::panicking()
            && let Ok(logs) = self.logs.lock()
        {
            eprintln!(
                    "\n===== SpacetimeDB child logs (only on failure) =====\n{}\n====================================================",
                    *logs
                );
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn waiting_child() {
        if env::var_os("SPACETIMEDB_GUARD_TEST_CHILD").is_some() {
            sleep(Duration::from_secs(60));
        }
    }

    #[test]
    fn startup_timeout_reaps_child_for_both_readiness_phases() {
        for reported_address in [false, true] {
            let mut command = Command::new(env::current_exe().unwrap());
            command.env("SPACETIMEDB_GUARD_TEST_CHILD", "1");
            let (mut child, logs, mut readers) =
                SpacetimeDbGuard::spawn_child(command, &["--exact", "startup_tests::waiting_child"], 0).unwrap();
            assert!(!readers.is_empty());
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let log = if reported_address {
                format!("Starting SpacetimeDB listening on {}\n", listener.local_addr().unwrap())
            } else {
                String::new()
            };
            *logs.lock().unwrap() = log;
            let error =
                SpacetimeDbGuard::finish_server_start(&mut child, &logs, &mut readers, 0, Duration::from_millis(10))
                    .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
            assert_eq!(error.to_string().contains("HTTP"), reported_address);
            assert!(
                child.try_wait().unwrap().is_some(),
                "failed startup left its child alive"
            );
            assert!(readers.is_empty());
        }
    }
}
