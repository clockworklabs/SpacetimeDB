#![allow(clippy::disallowed_macros)]
//! Rust smoketest infrastructure for SpacetimeDB.
//!
//! This crate provides utilities for writing end-to-end tests that compile and publish
//! SpacetimeDB modules, then exercise them via CLI commands.
//!
//! # Running Smoketests
//!
//! Always run smoketests using the xtask command to ensure binaries are pre-built:
//!
//! ```bash
//! cargo smoketest                     # Run all smoketests
//! cargo smoketest -- test_name        # Run specific tests
//! cargo xtask smoketest -- --help     # See all options
//! ```
//!
//! # Example
//!
//! ```ignore
//! use spacetimedb_smoketests::Smoketest;
//!
//! const MODULE_CODE: &str = r#"
//! use spacetimedb::{table, reducer};
//!
//! #[spacetimedb::table(name = person, public)]
//! pub struct Person {
//!     name: String,
//! }
//!
//! #[spacetimedb::reducer]
//! pub fn add(ctx: &ReducerContext, name: String) {
//!     ctx.db.person().insert(Person { name });
//! }
//! "#;
//!
//! #[test]
//! fn test_example() {
//!     let mut test = Smoketest::builder()
//!         .module_code(MODULE_CODE)
//!         .build();
//!
//!     test.call("add", &["Alice"]).unwrap();
//!     test.assert_sql("SELECT * FROM person", "name\n-----\nAlice");
//! }
//! ```

use anyhow::{bail, Context, Result};
use regex::Regex;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

/// Helper macro for timing operations and printing results
macro_rules! timed {
    ($label:expr, $expr:expr) => {{
        let start = Instant::now();
        let result = $expr;
        let elapsed = start.elapsed();
        eprintln!("[TIMING] {}: {:?}", $label, elapsed);
        result
    }};
}

/// Returns the workspace root directory.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find workspace root")
        .to_path_buf()
}

/// Controls whether smoketests share build caches or run in complete isolation.
///
/// - `true`: All tests share `target/smoketest-modules/` and the global `~/.cargo/` cache.
///   First test compiles dependencies, subsequent tests reuse cached artifacts.
///
/// - `false`: Each test gets its own `CARGO_HOME` and target directory for complete isolation.
///   No lock contention between parallel tests since nothing is shared.
const USE_SHARED_TARGET_DIR: bool = true;

/// Returns the shared target directory for smoketest module builds, if enabled.
///
/// This directory is shared across all smoketests to cache compiled dependencies.
/// Using a shared target directory dramatically reduces build times since the
/// spacetimedb bindings and other dependencies only need to be compiled once.
fn shared_module_target_dir() -> Option<PathBuf> {
    if !USE_SHARED_TARGET_DIR {
        return None;
    }
    static TARGET_DIR: OnceLock<PathBuf> = OnceLock::new();
    Some(
        TARGET_DIR
            .get_or_init(|| {
                let target_dir = workspace_root().join("target/smoketest-modules");
                fs::create_dir_all(&target_dir).expect("Failed to create shared module target directory");
                target_dir
            })
            .clone(),
    )
}

/// Generates a random lowercase alphabetic string suitable for database names.
pub fn random_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    // Convert to base-26 using lowercase letters only (a-z)
    let mut result = String::with_capacity(20);
    let mut n = timestamp;
    while n > 0 || result.len() < 10 {
        let c = (b'a' + (n % 26) as u8) as char;
        result.push(c);
        n /= 26;
    }
    result
}

/// Returns true if dotnet 8.0+ is available on the system.
pub fn have_dotnet() -> bool {
    static HAVE_DOTNET: OnceLock<bool> = OnceLock::new();
    *HAVE_DOTNET.get_or_init(|| {
        Command::new("dotnet")
            .args(["--list-sdks"])
            .output()
            .map(|output| {
                if !output.status.success() {
                    return false;
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Check for dotnet 8.0 or higher
                stdout
                    .lines()
                    .any(|line| line.starts_with("8.") || line.starts_with("9.") || line.starts_with("10."))
            })
            .unwrap_or(false)
    })
}

/// Returns true if psql (PostgreSQL client) is available on the system.
pub fn have_psql() -> bool {
    static HAVE_PSQL: OnceLock<bool> = OnceLock::new();
    *HAVE_PSQL.get_or_init(|| {
        Command::new("psql")
            .args(["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

/// Returns true if pnpm is available on the system.
pub fn have_pnpm() -> bool {
    static HAVE_PNPM: OnceLock<bool> = OnceLock::new();
    *HAVE_PNPM.get_or_init(|| {
        Command::new("pnpm")
            .args(["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

/// Parse code blocks from quickstart markdown documentation.
/// Extracts code blocks with the specified language tag.
///
/// - `language`: "rust", "csharp", or "typescript"
/// - `module_name`: The name to replace "quickstart-chat" with
/// - `server`: If true, look for server code blocks (e.g. "rust server"), else client blocks
pub fn parse_quickstart(doc_content: &str, language: &str, module_name: &str, server: bool) -> String {
    // Normalize line endings to Unix style (LF) for consistent regex matching
    let doc_content = doc_content.replace("\r\n", "\n");

    // Determine the codeblock language tag to search for
    let codeblock_lang = if server {
        if language == "typescript" {
            "ts server".to_string()
        } else {
            format!("{} server", language)
        }
    } else if language == "typescript" {
        "ts".to_string()
    } else {
        language.to_string()
    };

    // Extract code blocks with the specified language
    let pattern = format!(r"```{}\n([\s\S]*?)\n```", regex::escape(&codeblock_lang));
    let re = Regex::new(&pattern).unwrap();
    let mut blocks: Vec<String> = re
        .captures_iter(&doc_content)
        .map(|cap| cap.get(1).unwrap().as_str().to_string())
        .collect();

    let mut end = String::new();

    // C# specific fixups
    if language == "csharp" {
        let mut found_on_connected = false;
        let mut filtered_blocks = Vec::new();

        for mut block in blocks {
            // The doc first creates an empty class Module, so we need to fixup the closing brace
            if block.contains("partial class Module") {
                block = block.replace("}", "");
                end = "\n}".to_string();
            }
            // Remove the first `OnConnected` block, which body is later updated
            if block.contains("OnConnected(DbConnection conn") && !found_on_connected {
                found_on_connected = true;
                continue;
            }
            filtered_blocks.push(block);
        }
        blocks = filtered_blocks;
    }

    // Join blocks and replace module name
    let result = blocks.join("\n").replace("quickstart-chat", module_name);
    result + &end
}

/// A smoketest instance that manages a SpacetimeDB server and module project.
pub struct Smoketest {
    /// The SpacetimeDB server guard (stops server on drop).
    pub guard: SpacetimeDbGuard,
    /// Temporary directory containing the module project.
    pub project_dir: tempfile::TempDir,
    /// Database identity after publishing (if any).
    pub database_identity: Option<String>,
    /// The server URL (e.g., "http://127.0.0.1:3000").
    pub server_url: String,
    /// Path to the test-specific CLI config file (isolates tests from user config).
    pub config_path: std::path::PathBuf,
    /// Unique module name for this test instance.
    /// Used to avoid wasm output conflicts when tests run in parallel.
    module_name: String,
}

/// Response from an HTTP API call.
pub struct ApiResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// Response body.
    pub body: Vec<u8>,
}

impl ApiResponse {
    /// Returns the body as a string.
    pub fn text(&self) -> Result<String> {
        String::from_utf8(self.body.clone()).context("Response body is not valid UTF-8")
    }

    /// Parses the body as JSON.
    pub fn json(&self) -> Result<serde_json::Value> {
        serde_json::from_slice(&self.body).context("Failed to parse response as JSON")
    }

    /// Returns true if the status code indicates success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }
}

/// Builder for creating `Smoketest` instances.
pub struct SmoketestBuilder {
    module_code: Option<String>,
    bindings_features: Vec<String>,
    extra_deps: String,
    autopublish: bool,
    pg_port: Option<u16>,
}

impl Default for SmoketestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SmoketestBuilder {
    /// Creates a new builder with default settings.
    pub fn new() -> Self {
        Self {
            module_code: None,
            bindings_features: vec!["unstable".to_string()],
            extra_deps: String::new(),
            autopublish: true,
            pg_port: None,
        }
    }

    /// Enables the PostgreSQL wire protocol on the specified port.
    pub fn pg_port(mut self, port: u16) -> Self {
        self.pg_port = Some(port);
        self
    }

    /// Sets the module code to compile and publish.
    pub fn module_code(mut self, code: &str) -> Self {
        self.module_code = Some(code.to_string());
        self
    }

    /// Sets additional features for the spacetimedb bindings dependency.
    pub fn bindings_features(mut self, features: &[&str]) -> Self {
        self.bindings_features = features.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Adds extra dependencies to the module's Cargo.toml.
    pub fn extra_deps(mut self, deps: &str) -> Self {
        self.extra_deps = deps.to_string();
        self
    }

    /// Sets whether to automatically publish the module on build.
    /// Default is true.
    pub fn autopublish(mut self, yes: bool) -> Self {
        self.autopublish = yes;
        self
    }

    /// Builds the `Smoketest` instance.
    ///
    /// This spawns a SpacetimeDB server, creates a temporary project directory,
    /// writes the module code, and optionally publishes the module.
    ///
    /// # Panics
    ///
    /// Panics if the CLI/standalone binaries haven't been built or are stale.
    /// Run `cargo smoketest prepare` to build binaries before running tests.
    pub fn build(self) -> Smoketest {
        // Check binaries first - this will panic with a helpful message if missing/stale
        let _ = ensure_binaries_built();
        let build_start = Instant::now();

        let guard = timed!(
            "server spawn",
            SpacetimeDbGuard::spawn_in_temp_data_dir_with_pg_port(self.pg_port)
        );
        let project_dir = tempfile::tempdir().expect("Failed to create temp project directory");

        let project_setup_start = Instant::now();

        // Generate a unique module name to avoid wasm output conflicts in parallel tests.
        // The format is smoketest_module_{random} which produces smoketest_module_{random}.wasm
        let module_name = format!("smoketest_module_{}", random_string());

        // Create project structure
        fs::create_dir_all(project_dir.path().join("src")).expect("Failed to create src directory");

        // Write Cargo.toml with unique module name
        let workspace_root = workspace_root();
        let bindings_path = workspace_root.join("crates/bindings");
        let bindings_path_str = bindings_path.display().to_string().replace('\\', "/");
        let features_str = format!("{:?}", self.bindings_features);

        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{}", features = {} }}
log = "0.4"
{}
"#,
            module_name, bindings_path_str, features_str, self.extra_deps
        );
        fs::write(project_dir.path().join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

        // Copy rust-toolchain.toml
        let toolchain_src = workspace_root.join("rust-toolchain.toml");
        if toolchain_src.exists() {
            fs::copy(&toolchain_src, project_dir.path().join("rust-toolchain.toml"))
                .expect("Failed to copy rust-toolchain.toml");
        }

        // Write module code
        let module_code = self.module_code.unwrap_or_else(|| {
            r#"use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
pub fn noop(_ctx: &ReducerContext) {}
"#
            .to_string()
        });
        fs::write(project_dir.path().join("src/lib.rs"), &module_code).expect("Failed to write lib.rs");

        eprintln!("[TIMING] project setup: {:?}", project_setup_start.elapsed());

        let server_url = guard.host_url.clone();
        let config_path = project_dir.path().join("config.toml");
        let mut smoketest = Smoketest {
            guard,
            project_dir,
            database_identity: None,
            server_url,
            config_path,
            module_name,
        };

        if self.autopublish {
            smoketest.publish_module().expect("Failed to publish module");
        }

        eprintln!("[TIMING] total build: {:?}", build_start.elapsed());
        smoketest
    }
}

impl Smoketest {
    /// Creates a new builder for configuring a smoketest.
    pub fn builder() -> SmoketestBuilder {
        SmoketestBuilder::new()
    }

    /// Restart the SpacetimeDB server.
    ///
    /// This stops the current server process and starts a new one with the
    /// same data directory. All data is preserved across the restart.
    /// The server URL may change since a new ephemeral port is allocated.
    pub fn restart_server(&mut self) {
        self.guard.restart();
        // Update server_url since the port may have changed
        self.server_url = self.guard.host_url.clone();
    }

    /// Returns the server host (without protocol), e.g., "127.0.0.1:3000".
    pub fn server_host(&self) -> &str {
        self.server_url
            .strip_prefix("http://")
            .or_else(|| self.server_url.strip_prefix("https://"))
            .unwrap_or(&self.server_url)
    }

    /// Returns the PostgreSQL wire protocol port, if enabled.
    pub fn pg_port(&self) -> Option<u16> {
        self.guard.pg_port
    }

    /// Reads the authentication token from the config file.
    pub fn read_token(&self) -> Result<String> {
        let config_content = fs::read_to_string(&self.config_path).context("Failed to read config file")?;

        // Parse as TOML and extract spacetimedb_token
        let config: toml::Value = config_content.parse().context("Failed to parse config as TOML")?;

        config
            .get("spacetimedb_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .context("No spacetimedb_token found in config")
    }

    /// Runs psql command against the PostgreSQL wire protocol server.
    ///
    /// Returns the output on success, or an error with stderr on failure.
    pub fn psql(&self, database: &str, sql: &str) -> Result<String> {
        let pg_port = self.pg_port().context("PostgreSQL wire protocol not enabled")?;
        let token = self.read_token()?;

        // Extract just the host part (without port)
        let host = self.server_host().split(':').next().unwrap_or("127.0.0.1");

        let output = Command::new("psql")
            .args([
                "-h",
                host,
                "-p",
                &pg_port.to_string(),
                "-U",
                "postgres",
                "-d",
                database,
                "--quiet",
                "-c",
                sql,
            ])
            .env("PGPASSWORD", &token)
            .output()
            .context("Failed to run psql")?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() && !output.status.success() {
            bail!("{}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Asserts that psql output matches the expected value.
    pub fn assert_psql(&self, database: &str, sql: &str, expected: &str) {
        let output = self.psql(database, sql).expect("psql failed");
        let output_normalized: String = output.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
        let expected_normalized: String = expected.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
        assert_eq!(
            output_normalized, expected_normalized,
            "psql output mismatch for query: {}\n\nExpected:\n{}\n\nActual:\n{}",
            sql, expected_normalized, output_normalized
        );
    }

    /// Runs a spacetime CLI command.
    ///
    /// Returns the command output. The command is run but not yet asserted.
    /// Uses --config-path to isolate test config from user config.
    /// Callers should pass `--server` explicitly when the command needs it.
    pub fn spacetime_cmd(&self, args: &[&str]) -> Output {
        let start = Instant::now();
        let cli_path = ensure_binaries_built();
        let output = Command::new(&cli_path)
            .arg("--config-path")
            .arg(&self.config_path)
            .args(args)
            .current_dir(self.project_dir.path())
            .output()
            .expect("Failed to execute spacetime command");

        let cmd_name = args.first().unwrap_or(&"unknown");
        eprintln!("[TIMING] spacetime {}: {:?}", cmd_name, start.elapsed());
        output
    }

    /// Runs a spacetime CLI command and returns stdout as a string.
    ///
    /// Panics if the command fails.
    /// Callers should pass `--server` explicitly when the command needs it.
    pub fn spacetime(&self, args: &[&str]) -> Result<String> {
        let output = self.spacetime_cmd(args);
        if !output.status.success() {
            bail!(
                "spacetime {:?} failed:\nstdout: {}\nstderr: {}",
                args,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Writes new module code to the project.
    pub fn write_module_code(&self, code: &str) -> Result<()> {
        fs::write(self.project_dir.path().join("src/lib.rs"), code).context("Failed to write module code")?;
        Ok(())
    }

    /// Runs `spacetime build` and returns the raw output.
    ///
    /// Use this when you need to check for build failures (e.g., wasm_bindgen detection).
    pub fn spacetime_build(&self) -> Output {
        let start = Instant::now();
        let project_path = self.project_dir.path().to_str().unwrap();
        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);
        cmd.args(["build", "--project-path", project_path])
            .current_dir(self.project_dir.path());

        if let Some(target_dir) = shared_module_target_dir() {
            // Shared mode: use shared target directory, inherit global CARGO_HOME
            cmd.env("CARGO_TARGET_DIR", target_dir);
        } else {
            // Isolated mode: each test gets its own CARGO_HOME for complete isolation
            let isolated_cargo_home = self.project_dir.path().join(".cargo-home");
            fs::create_dir_all(&isolated_cargo_home).expect("Failed to create isolated CARGO_HOME");
            cmd.env("CARGO_HOME", &isolated_cargo_home);
        }

        let output = cmd.output().expect("Failed to execute spacetime build");
        eprintln!("[TIMING] spacetime build: {:?}", start.elapsed());
        output
    }

    /// Publishes the module and stores the database identity.
    pub fn publish_module(&mut self) -> Result<String> {
        self.publish_module_opts(None, false)
    }

    /// Publishes the module with a specific name and optional clear flag.
    ///
    /// If `name` is provided, the database will be published with that name.
    /// If `clear` is true, the database will be cleared before publishing.
    pub fn publish_module_named(&mut self, name: &str, clear: bool) -> Result<String> {
        self.publish_module_opts(Some(name), clear)
    }

    /// Re-publishes the module to the existing database identity with optional clear.
    ///
    /// This is useful for testing auto-migrations where you want to update
    /// the module without clearing the database.
    pub fn publish_module_clear(&mut self, clear: bool) -> Result<String> {
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published yet")?
            .clone();
        self.publish_module_opts(Some(&identity), clear)
    }

    /// Internal helper for publishing with options.
    fn publish_module_opts(&mut self, name: Option<&str>, clear: bool) -> Result<String> {
        let start = Instant::now();
        let project_path = self.project_dir.path().to_str().unwrap().to_string();

        // First, run spacetime build to compile the WASM module (separate from publish)
        let build_start = Instant::now();
        let cli_path = ensure_binaries_built();
        let target_dir = shared_module_target_dir();
        let mut build_cmd = Command::new(&cli_path);
        build_cmd
            .args(["build", "--project-path", &project_path])
            .current_dir(self.project_dir.path());

        if let Some(ref dir) = target_dir {
            // Shared mode: use shared target directory, inherit global CARGO_HOME
            build_cmd.env("CARGO_TARGET_DIR", dir);
        } else {
            // Isolated mode: each test gets its own CARGO_HOME for complete isolation
            let isolated_cargo_home = self.project_dir.path().join(".cargo-home");
            fs::create_dir_all(&isolated_cargo_home).expect("Failed to create isolated CARGO_HOME");
            build_cmd.env("CARGO_HOME", &isolated_cargo_home);
        }

        let build_output = build_cmd.output().expect("Failed to execute spacetime build");
        let build_elapsed = build_start.elapsed();
        eprintln!("[TIMING] spacetime build: {:?}", build_elapsed);

        // In isolated mode, log detailed build breakdown from cargo output
        if target_dir.is_none() {
            let stderr = String::from_utf8_lossy(&build_output.stderr);
            let mut downloading_count = 0;
            let mut compiling_count = 0;
            for line in stderr.lines() {
                if line.contains("Downloading") {
                    downloading_count += 1;
                } else if line.contains("Compiling") {
                    compiling_count += 1;
                } else if line.contains("Blocking") || line.contains("Waiting") {
                    eprintln!("[BUILD] {}", line);
                }
            }
            eprintln!(
                "[BUILD DETAILS] Downloaded {} crates, Compiled {} crates in {:?}",
                downloading_count, compiling_count, build_elapsed
            );
        }

        if !build_output.status.success() {
            bail!(
                "spacetime build failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&build_output.stdout),
                String::from_utf8_lossy(&build_output.stderr)
            );
        }

        // Construct the wasm path using the unique module name
        // Use the target directory where the build output goes (shared or per-project)
        let wasm_filename = format!("{}.wasm", self.module_name);
        let effective_target_dir = target_dir.unwrap_or_else(|| self.project_dir.path().join("target"));
        let wasm_path = effective_target_dir
            .join("wasm32-unknown-unknown/release")
            .join(&wasm_filename);
        let wasm_path_str = wasm_path.to_str().unwrap().to_string();

        // Now publish with --bin-path to skip rebuild
        let publish_start = Instant::now();
        let mut args = vec![
            "publish",
            "--server",
            &self.server_url,
            "--bin-path",
            &wasm_path_str,
            "--yes",
        ];

        if clear {
            args.push("--clear-database");
        }

        let name_owned;
        if let Some(n) = name {
            name_owned = n.to_string();
            args.push(&name_owned);
        }

        let output = self.spacetime(&args)?;
        eprintln!(
            "[TIMING] spacetime publish (after build): {:?}",
            publish_start.elapsed()
        );
        eprintln!("[TIMING] publish_module total: {:?}", start.elapsed());

        // Parse the identity from output like "identity: abc123..."
        let re = Regex::new(r"identity: ([0-9a-fA-F]+)").unwrap();
        if let Some(caps) = re.captures(&output) {
            let identity = caps.get(1).unwrap().as_str().to_string();
            self.database_identity = Some(identity.clone());
            Ok(identity)
        } else {
            bail!("Failed to parse database identity from publish output: {}", output);
        }
    }

    /// Calls a reducer or procedure with the given arguments.
    ///
    /// Arguments are passed directly to the CLI as strings.
    pub fn call(&self, name: &str, args: &[&str]) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        let mut cmd_args = vec!["call", "--server", &self.server_url, "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime(&cmd_args)
    }

    /// Calls a reducer/procedure and returns the full output including stderr.
    pub fn call_output(&self, name: &str, args: &[&str]) -> Output {
        let identity = self.database_identity.as_ref().expect("No database published");

        let mut cmd_args = vec!["call", "--server", &self.server_url, "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime_cmd(&cmd_args)
    }

    /// Executes a SQL query against the database.
    pub fn sql(&self, query: &str) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&["sql", "--server", &self.server_url, identity.as_str(), query])
    }

    /// Executes a SQL query with the --confirmed flag.
    pub fn sql_confirmed(&self, query: &str) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&[
            "sql",
            "--server",
            &self.server_url,
            "--confirmed",
            identity.as_str(),
            query,
        ])
    }

    /// Asserts that a SQL query produces the expected output.
    ///
    /// Both the actual output and expected string have trailing whitespace
    /// trimmed from each line for comparison.
    pub fn assert_sql(&self, query: &str, expected: &str) {
        let actual = self.sql(query).expect("SQL query failed");
        let actual_normalized = normalize_whitespace(&actual);
        let expected_normalized = normalize_whitespace(expected);

        assert_eq!(
            actual_normalized, expected_normalized,
            "SQL output mismatch for query: {}\n\nExpected:\n{}\n\nActual:\n{}",
            query, expected_normalized, actual_normalized
        );
    }

    /// Fetches the last N log entries from the database.
    pub fn logs(&self, n: usize) -> Result<Vec<String>> {
        let records = self.log_records(n)?;
        Ok(records
            .into_iter()
            .filter_map(|r| r.get("message").and_then(|m| m.as_str()).map(String::from))
            .collect())
    }

    /// Fetches the last N log records as JSON values.
    pub fn log_records(&self, n: usize) -> Result<Vec<serde_json::Value>> {
        let identity = self.database_identity.as_ref().context("No database published")?;
        let n_str = n.to_string();

        let output = self.spacetime(&[
            "logs",
            "--server",
            &self.server_url,
            "--format=json",
            "-n",
            &n_str,
            "--",
            identity,
        ])?;

        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("Failed to parse log record"))
            .collect()
    }

    /// Creates a new identity by logging out and logging back in with a server-issued identity.
    ///
    /// This is useful for tests that need to test with multiple identities.
    pub fn new_identity(&self) -> Result<()> {
        let cli_path = ensure_binaries_built();
        let config_path_str = self.config_path.to_str().unwrap();

        // Logout first (ignore errors - may not be logged in)
        let _ = Command::new(&cli_path)
            .args(["--config-path", config_path_str, "logout"])
            .output();

        // Login with server-issued identity
        // Format: login --server-issued-login <server>
        let output = Command::new(&cli_path)
            .args([
                "--config-path",
                config_path_str,
                "login",
                "--server-issued-login",
                &self.server_url,
            ])
            .output()
            .context("Failed to login with new identity")?;

        if !output.status.success() {
            bail!(
                "Failed to create new identity:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Makes an HTTP API call to the server.
    ///
    /// Returns the response body as bytes, or an error with the HTTP status code.
    pub fn api_call(&self, method: &str, path: &str) -> Result<ApiResponse> {
        self.api_call_with_body(method, path, None)
    }

    /// Makes an HTTP API call with an optional request body.
    pub fn api_call_with_body(&self, method: &str, path: &str, body: Option<&[u8]>) -> Result<ApiResponse> {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        // Parse server URL to get host and port
        let url = &self.server_url;
        let host_port = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .unwrap_or(url);

        let mut stream = TcpStream::connect(host_port).context("Failed to connect to server")?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();

        // Build HTTP request
        let content_length = body.map(|b| b.len()).unwrap_or(0);
        let request = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            method, path, host_port, content_length
        );

        stream.write_all(request.as_bytes())?;
        if let Some(body) = body {
            stream.write_all(body)?;
        }

        // Read response
        let mut response = Vec::new();
        stream.read_to_end(&mut response)?;

        // Parse HTTP response
        let response_str = String::from_utf8_lossy(&response);
        let mut lines = response_str.lines();

        // Parse status line
        let status_line = lines.next().context("Empty response")?;
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .context("Failed to parse status code")?;

        // Find body (after empty line)
        let header_end = response_str.find("\r\n\r\n").unwrap_or(response_str.len());
        let body_start = header_end + 4;
        let body = if body_start < response.len() {
            response[body_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(ApiResponse { status_code, body })
    }

    /// Starts a subscription and waits for N updates (synchronous).
    ///
    /// Returns the updates as JSON values.
    /// For tests that need to perform actions while subscribed, use `subscribe_background` instead.
    pub fn subscribe(&self, queries: &[&str], n: usize) -> Result<Vec<serde_json::Value>> {
        self.subscribe_opts(queries, n, false)
    }

    /// Starts a subscription with --confirmed flag and waits for N updates.
    pub fn subscribe_confirmed(&self, queries: &[&str], n: usize) -> Result<Vec<serde_json::Value>> {
        self.subscribe_opts(queries, n, true)
    }

    /// Internal helper for subscribe with options.
    fn subscribe_opts(&self, queries: &[&str], n: usize, confirmed: bool) -> Result<Vec<serde_json::Value>> {
        let start = Instant::now();
        let identity = self.database_identity.as_ref().context("No database published")?;
        let config_path_str = self.config_path.to_str().unwrap();

        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);
        let mut args = vec![
            "--config-path",
            config_path_str,
            "subscribe",
            "--server",
            &self.server_url,
            identity,
            "-t",
            "30",
            "-n",
        ];
        let n_str = n.to_string();
        args.push(&n_str);
        args.push("--print-initial-update");
        if confirmed {
            args.push("--confirmed");
        }
        args.push("--");
        cmd.args(&args)
            .args(queries)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().context("Failed to run subscribe command")?;
        eprintln!("[TIMING] subscribe (n={}): {:?}", n, start.elapsed());

        if !output.status.success() {
            bail!("subscribe failed:\nstderr: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("Failed to parse subscription update"))
            .collect()
    }

    /// Starts a subscription in the background and returns a handle.
    ///
    /// This matches Python's subscribe semantics - start subscription first,
    /// perform actions, then call the handle to collect results.
    pub fn subscribe_background(&self, queries: &[&str], n: usize) -> Result<SubscriptionHandle> {
        self.subscribe_background_opts(queries, n, false)
    }

    /// Starts a subscription in the background with --confirmed flag.
    pub fn subscribe_background_confirmed(&self, queries: &[&str], n: usize) -> Result<SubscriptionHandle> {
        self.subscribe_background_opts(queries, n, true)
    }

    /// Internal helper for background subscribe with options.
    fn subscribe_background_opts(&self, queries: &[&str], n: usize, confirmed: bool) -> Result<SubscriptionHandle> {
        use std::io::{BufRead, BufReader};

        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?
            .clone();

        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);
        // Use --print-initial-update so we know when subscription is established
        let config_path_str = self.config_path.to_str().unwrap().to_string();
        let mut args = vec![
            "--config-path".to_string(),
            config_path_str,
            "subscribe".to_string(),
            "--server".to_string(),
            self.server_url.clone(),
            identity,
            "-t".to_string(),
            "30".to_string(),
            "-n".to_string(),
            n.to_string(),
            "--print-initial-update".to_string(),
        ];
        if confirmed {
            args.push("--confirmed".to_string());
        }
        args.push("--".to_string());
        cmd.args(&args)
            .args(queries)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn subscribe command")?;
        let stdout = child.stdout.take().context("No stdout from subscribe")?;
        let stderr = child.stderr.take().context("No stderr from subscribe")?;
        let mut reader = BufReader::new(stdout);

        // Wait for initial update line - this blocks until subscription is established
        let mut init_line = String::new();
        reader
            .read_line(&mut init_line)
            .context("Failed to read initial update from subscribe")?;
        eprintln!("[SUBSCRIBE] initial update received: {}", init_line.trim());

        Ok(SubscriptionHandle {
            child,
            reader,
            stderr,
            n,
            start: Instant::now(),
        })
    }
}

/// Handle for a background subscription.
pub struct SubscriptionHandle {
    child: std::process::Child,
    reader: std::io::BufReader<std::process::ChildStdout>,
    stderr: std::process::ChildStderr,
    n: usize,
    start: Instant,
}

impl SubscriptionHandle {
    /// Wait for the subscription to complete and return the updates.
    pub fn collect(mut self) -> Result<Vec<serde_json::Value>> {
        use std::io::{BufRead, Read};

        // Read remaining lines from stdout
        let mut updates = Vec::new();
        for line in self.reader.by_ref().lines() {
            let line = line.context("Failed to read line from subscribe")?;
            if !line.trim().is_empty() {
                let value: serde_json::Value =
                    serde_json::from_str(&line).context("Failed to parse subscription update")?;
                updates.push(value);
            }
        }

        // Wait for child to complete
        let status = self.child.wait().context("Failed to wait for subscribe")?;
        eprintln!(
            "[TIMING] subscribe_background (n={}): {:?}",
            self.n,
            self.start.elapsed()
        );

        if !status.success() {
            let mut stderr_buf = String::new();
            self.stderr.read_to_string(&mut stderr_buf).ok();
            bail!("subscribe failed:\nstderr: {}", stderr_buf);
        }

        Ok(updates)
    }
}

/// Normalizes whitespace by trimming trailing whitespace from each line.
fn normalize_whitespace(s: &str) -> String {
    s.lines().map(|line| line.trim_end()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        let input = "hello   \nworld  \n  foo  ";
        let expected = "hello\nworld\n  foo";
        assert_eq!(normalize_whitespace(input), expected);
    }
}
