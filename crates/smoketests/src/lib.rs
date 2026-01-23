#![allow(clippy::disallowed_macros)]
//! Rust smoketest infrastructure for SpacetimeDB.
//!
//! This crate provides utilities for writing end-to-end tests that compile and publish
//! SpacetimeDB modules, then exercise them via CLI commands.
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
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find workspace root")
        .to_path_buf()
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
}

/// Builder for creating `Smoketest` instances.
pub struct SmoketestBuilder {
    module_code: Option<String>,
    bindings_features: Vec<String>,
    extra_deps: String,
    autopublish: bool,
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
        }
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
    pub fn build(self) -> Smoketest {
        let build_start = Instant::now();

        let guard = timed!("server spawn", SpacetimeDbGuard::spawn_in_temp_data_dir());
        let project_dir = tempfile::tempdir().expect("Failed to create temp project directory");

        let project_setup_start = Instant::now();

        // Create project structure
        fs::create_dir_all(project_dir.path().join("src")).expect("Failed to create src directory");

        // Write Cargo.toml
        let workspace_root = workspace_root();
        let bindings_path = workspace_root.join("crates/bindings");
        let bindings_path_str = bindings_path.display().to_string().replace('\\', "/");
        let features_str = format!("{:?}", self.bindings_features);

        let cargo_toml = format!(
            r#"[package]
name = "smoketest-module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{}", features = {} }}
log = "0.4"
{}
"#,
            bindings_path_str, features_str, self.extra_deps
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

    /// Runs a spacetime CLI command with the configured server.
    ///
    /// Returns the command output. The command is run but not yet asserted.
    /// The `--server` flag is automatically inserted before any `--` separator,
    /// or at the end if no separator exists.
    pub fn spacetime_cmd(&self, args: &[&str]) -> Output {
        let start = Instant::now();
        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);

        // Use test-specific config path to avoid polluting user's config
        cmd.arg("--config-path").arg(&self.config_path);

        // Insert --server before any "--" separator, or at the end
        // This ensures --server is processed as a flag, not a positional arg
        if let Some(pos) = args.iter().position(|&a| a == "--") {
            cmd.args(&args[..pos])
                .arg("--server")
                .arg(&self.server_url)
                .args(&args[pos..]);
        } else {
            cmd.args(args).arg("--server").arg(&self.server_url);
        }

        let output = cmd
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

    /// Runs a spacetime CLI command without adding the --server flag.
    ///
    /// Use this for local-only commands like `generate` or `server` subcommands
    /// that don't need a server connection.
    /// Still uses --config-path to isolate test config from user config.
    pub fn spacetime_local(&self, args: &[&str]) -> Result<String> {
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
        eprintln!("[TIMING] spacetime_local {}: {:?}", cmd_name, start.elapsed());

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
        let output = Command::new(&cli_path)
            .args(["build", "--project-path", project_path])
            .current_dir(self.project_dir.path())
            .output()
            .expect("Failed to execute spacetime build");
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
        let build_output = Command::new(&cli_path)
            .args(["build", "--project-path", &project_path])
            .current_dir(self.project_dir.path())
            .output()
            .expect("Failed to execute spacetime build");
        eprintln!("[TIMING] spacetime build: {:?}", build_start.elapsed());

        if !build_output.status.success() {
            bail!(
                "spacetime build failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&build_output.stdout),
                String::from_utf8_lossy(&build_output.stderr)
            );
        }

        // Construct the wasm path (module name is smoketest-module -> smoketest_module.wasm)
        let wasm_path = self
            .project_dir
            .path()
            .join("target/wasm32-unknown-unknown/release/smoketest_module.wasm");
        let wasm_path_str = wasm_path.to_str().unwrap().to_string();

        // Now publish with --bin-path to skip rebuild
        let publish_start = Instant::now();
        let mut args = vec!["publish", "--bin-path", &wasm_path_str, "--yes"];

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

        let mut cmd_args = vec!["call", "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime(&cmd_args)
    }

    /// Calls a reducer/procedure and returns the full output including stderr.
    pub fn call_output(&self, name: &str, args: &[&str]) -> Output {
        let identity = self.database_identity.as_ref().expect("No database published");

        let mut cmd_args = vec!["call", "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime_cmd(&cmd_args)
    }

    /// Executes a SQL query against the database.
    pub fn sql(&self, query: &str) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&["sql", identity.as_str(), query])
    }

    /// Executes a SQL query with the --confirmed flag.
    pub fn sql_confirmed(&self, query: &str) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&["sql", "--confirmed", identity.as_str(), query])
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

        let output = self.spacetime(&["logs", "--format=json", "-n", &n.to_string(), "--", identity])?;

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
