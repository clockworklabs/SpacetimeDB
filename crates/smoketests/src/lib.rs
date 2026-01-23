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
use serde::Serialize;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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
        let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
        let project_dir = tempfile::tempdir().expect("Failed to create temp project directory");

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
        fs::write(project_dir.path().join("Cargo.toml"), cargo_toml)
            .expect("Failed to write Cargo.toml");

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

        let server_url = guard.host_url.clone();
        let mut smoketest = Smoketest {
            guard,
            project_dir,
            database_identity: None,
            server_url,
        };

        if self.autopublish {
            smoketest.publish_module().expect("Failed to publish module");
        }

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
    /// The `--server` flag is automatically inserted after the first argument (the subcommand).
    pub fn spacetime_cmd(&self, args: &[&str]) -> Output {
        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);

        // Insert --server after the subcommand
        if let Some((subcommand, rest)) = args.split_first() {
            cmd.arg(subcommand)
                .arg("--server")
                .arg(&self.server_url)
                .args(rest);
        }

        cmd.current_dir(self.project_dir.path())
            .output()
            .expect("Failed to execute spacetime command")
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

    /// Writes new module code to the project.
    pub fn write_module_code(&self, code: &str) -> Result<()> {
        fs::write(self.project_dir.path().join("src/lib.rs"), code)
            .context("Failed to write module code")?;
        Ok(())
    }

    /// Publishes the module and stores the database identity.
    pub fn publish_module(&mut self) -> Result<String> {
        let project_path = self.project_dir.path().to_str().unwrap().to_string();
        let output = self.spacetime(&[
            "publish",
            "--project-path",
            &project_path,
            "--yes",
        ])?;

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
    /// Arguments are serialized to JSON.
    pub fn call<T: Serialize>(&self, name: &str, args: &[T]) -> Result<String> {
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?;

        let mut cmd_args = vec!["call", "--", identity.as_str(), name];
        let json_args: Vec<String> = args.iter().map(|a| serde_json::to_string(a).unwrap()).collect();
        let json_refs: Vec<&str> = json_args.iter().map(|s| s.as_str()).collect();
        cmd_args.extend(json_refs);

        self.spacetime(&cmd_args)
    }

    /// Calls a reducer or procedure with raw string arguments (no JSON serialization).
    pub fn call_raw(&self, name: &str, args: &[&str]) -> Result<String> {
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?;

        let mut cmd_args = vec!["call", "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime(&cmd_args)
    }

    /// Calls a reducer/procedure and returns the full output including stderr.
    pub fn call_output(&self, name: &str, args: &[&str]) -> Output {
        let identity = self
            .database_identity
            .as_ref()
            .expect("No database published");

        let mut cmd_args = vec!["call", "--", identity.as_str(), name];
        cmd_args.extend(args);

        self.spacetime_cmd(&cmd_args)
    }

    /// Executes a SQL query against the database.
    pub fn sql(&self, query: &str) -> Result<String> {
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?;

        self.spacetime(&["sql", identity.as_str(), query])
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
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?;

        let output = self.spacetime(&["logs", "--format=json", "-n", &n.to_string(), "--", identity])?;

        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("Failed to parse log record"))
            .collect()
    }

    /// Starts a subscription and waits for N updates.
    ///
    /// Returns the updates as JSON values.
    pub fn subscribe(&self, queries: &[&str], n: usize) -> Result<Vec<serde_json::Value>> {
        let identity = self
            .database_identity
            .as_ref()
            .context("No database published")?;

        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);
        cmd.args(["subscribe", "--server", &self.server_url, identity, "-t", "600", "-n", &n.to_string(), "--print-initial-update", "--"])
            .args(queries)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().context("Failed to run subscribe command")?;

        if !output.status.success() {
            bail!(
                "subscribe failed:\nstderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).context("Failed to parse subscription update"))
            .collect()
    }
}

/// Normalizes whitespace by trimming trailing whitespace from each line.
fn normalize_whitespace(s: &str) -> String {
    s.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
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
