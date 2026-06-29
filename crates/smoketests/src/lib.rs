#![allow(clippy::disallowed_macros)]
//! Rust smoketest infrastructure for SpacetimeDB.
//!
//! This crate provides utilities for writing end-to-end tests that compile and publish
//! SpacetimeDB modules, then exercise them via CLI commands.
//!
//! # Pre-compiled Modules
//!
//! For better performance, modules can be pre-compiled during the warmup phase.
//! Use `Smoketest::builder().precompiled_module("name")` to use a pre-compiled module
//! instead of `module_code()` which compiles at runtime.
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
//! #[spacetimedb::table(accessor = person, public)]
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

mod csharp;
pub mod modules;

use anyhow::{bail, Context, Result};
use regex::Regex;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::Instant;
use which::which;

/// Returns the remote server URL if running against a remote server.
///
/// Set the `SPACETIME_REMOTE_SERVER` environment variable to run tests against
/// a remote server instead of spawning local servers.
pub fn remote_server_url() -> Option<String> {
    std::env::var("SPACETIME_REMOTE_SERVER").ok()
}

/// Returns true if running against a remote server.
pub fn is_remote_server() -> bool {
    remote_server_url().is_some()
}

/// Returns true if remote smoketests are using a SpacetimeAuth-issued token.
pub fn is_spacetime_login() -> bool {
    std::env::var("SPACETIME_SMOKETEST_SPACETIME_LOGIN").ok().as_deref() == Some("1")
}

/// Skip this test if running against a remote server.
///
/// Use this macro at the start of tests that require a local server,
/// such as tests that call `restart_server()` or access local data directories.
///
/// # Example
///
/// ```ignore
/// #[test]
/// fn test_restart() {
///     require_local_server!();
///     let mut test = Smoketest::builder().build();
///     test.restart_server();
///     // ...
/// }
/// ```
#[macro_export]
macro_rules! require_local_server {
    () => {
        if $crate::is_remote_server() {
            #[allow(clippy::disallowed_macros)]
            {
                eprintln!("Skipping test: requires local server");
            }
            return;
        }
    };
}

#[macro_export]
macro_rules! require_server_issued_login {
    () => {
        if $crate::is_spacetime_login() {
            #[allow(clippy::disallowed_macros)]
            {
                eprintln!("Skipping test: requires server-issued throwaway identities");
            }
            return;
        }
    };
}

#[macro_export]
macro_rules! require_dotnet {
    () => {
        if !$crate::allow_dotnet() {
            #[allow(clippy::disallowed_macros)]
            {
                eprintln!("Skipping dotnet test");
            }
            return;
        }
        if !$crate::have_dotnet() {
            panic!("dotnet 8.0+ not found");
        }
    };
}

#[macro_export]
macro_rules! require_psql {
    () => {
        if !$crate::have_psql() {
            panic!("psql not found");
        }
    };
}

#[macro_export]
macro_rules! require_pnpm {
    () => {
        if $crate::pnpm_path().is_none() {
            panic!("pnpm not found");
        }
    };
}

#[macro_export]
macro_rules! require_emscripten {
    () => {
        if !$crate::have_emscripten() {
            panic!("emcc (Emscripten) not found");
        }
    };
}

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

/// Rewrites `spacetimedb` dependency in `<module_dir>/Cargo.toml` to use local workspace bindings.
pub fn patch_module_cargo_to_local_bindings(module_dir: &Path) -> Result<()> {
    let cargo_toml_path = module_dir.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    let bindings_path = workspace_root().join("crates/bindings");
    let bindings_path_str = bindings_path.display().to_string().replace('\\', "/");
    let replacement = format!(r#"spacetimedb = {{ path = "{bindings_path_str}", features = ["unstable"] }}"#);

    let patched = cargo_toml
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("spacetimedb = ") {
                replacement.as_str()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&cargo_toml_path, format!("{patched}\n"))
        .with_context(|| format!("Failed to write {}", cargo_toml_path.display()))?;
    Ok(())
}

/// Returns the shared target directory for smoketest module builds.
///
/// All tests share this directory to cache compiled dependencies. The warmup step
/// pre-compiles dependencies, then each test only needs to compile its unique module.
/// Cargo serializes builds due to directory locking, but this is still faster than
/// each test compiling all dependencies from scratch.
fn shared_target_dir() -> PathBuf {
    static TARGET_DIR: OnceLock<PathBuf> = OnceLock::new();
    TARGET_DIR
        .get_or_init(|| {
            let target_dir = workspace_root().join("target/smoketest-modules");
            fs::create_dir_all(&target_dir).expect("Failed to create shared module target directory");
            target_dir
        })
        .clone()
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

/// Returns true if tests are configured to allow dotnet
pub fn allow_dotnet() -> bool {
    let Ok(s) = std::env::var("SMOKETESTS_DOTNET") else {
        return true;
    };
    match s.as_str() {
        "" | "0" => false,
        s => s.to_lowercase() != "false",
    }
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
pub fn pnpm_path() -> Option<PathBuf> {
    static PNPM_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PNPM_PATH.get_or_init(|| which("pnpm").ok()).clone()
}

fn pnpm_minimum_release_age() -> Result<String> {
    let workspace = fs::read_to_string(workspace_root().join("pnpm-workspace.yaml"))?;
    workspace
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("minimumReleaseAge:")?
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map(|age| age.to_string())
        .context("pnpm-workspace.yaml is missing minimumReleaseAge")
}

/// Runs a command and returns stdout as a string.
pub fn run_cmd(args: &[&str], cwd: &Path) -> Result<String> {
    run_cmd_inner(args, cwd, None)
}

/// Runs a command with stdin input and returns stdout as a string.
pub fn run_cmd_with_stdin(args: &[&str], cwd: &Path, stdin_input: &str) -> Result<String> {
    run_cmd_inner(args, cwd, Some(stdin_input))
}

fn run_cmd_inner(args: &[&str], cwd: &Path, stdin_input: Option<&str>) -> Result<String> {
    let Some(program) = args.first() else {
        bail!("run_cmd called with no program");
    };

    let mut cmd = Command::new(program);
    cmd.args(&args[1..])
        .current_dir(cwd)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped());

    if stdin_input.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn command: {args:?}"))?;

    if let Some(input) = stdin_input {
        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(input.as_bytes())?;
        }
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        bail!(
            "command {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Runs a `pnpm` command and returns stdout as a string.
pub fn pnpm(args: &[&str], cwd: &Path) -> Result<String> {
    let pnpm_path = pnpm_path().context("Could not locate pnpm")?;
    let minimum_release_age = pnpm_minimum_release_age()?;

    // Smoketests often install inside temp projects created by `spacetime init`.
    // Those projects intentionally do not carry the repo's .npmrc, so pass the
    // repo policy through pnpm's environment variable instead.
    let output = Command::new(&pnpm_path)
        .args(args)
        .current_dir(cwd)
        .env("npm_config_minimum_release_age", minimum_release_age)
        .output()
        .with_context(|| format!("Failed to spawn pnpm {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "pnpm {} (in {:?}) failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            cwd,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Builds the local TypeScript bindings package.
pub fn build_typescript_sdk() -> Result<()> {
    let workspace = workspace_root();
    let ts_bindings = workspace.join("crates/bindings-typescript");
    pnpm(&["install"], &ts_bindings)?;
    pnpm(&["build"], &ts_bindings)?;
    Ok(())
}

/// Returns true if Emscripten (emcc) is available on the system.
pub fn have_emscripten() -> bool {
    static HAVE_EMSCRIPTEN: OnceLock<bool> = OnceLock::new();
    *HAVE_EMSCRIPTEN.get_or_init(|| which("emcc").is_ok() || which("emcc.bat").is_ok())
}

const CPP_SMOKETEST_CMAKELISTS: &str = r#"cmake_minimum_required(VERSION 3.16)
project(smoketest_cpp_module)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(SPACETIMEDB_CPP_LIBRARY_PATH "@SPACETIMEDB_CPP_LIBRARY_PATH@")

add_executable(lib src/lib.cpp)

target_include_directories(lib PRIVATE
    ${SPACETIMEDB_CPP_LIBRARY_PATH}/include
)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    target_compile_options(lib PRIVATE -fno-exceptions -O2 -g0)
    target_compile_definitions(lib PRIVATE SPACETIMEDB_UNSTABLE_FEATURES)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DSPACETIMEDB_UNSTABLE_FEATURES")
endif()

add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_cpp_library)
target_link_libraries(lib PRIVATE spacetimedb_cpp_library)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    set(EXPORTED_FUNCS
        "['_malloc','_free','___describe_module__','___call_reducer__','___call_procedure__','___call_http_handler__']"
    )

    target_link_options(lib PRIVATE
        "SHELL:-sSTANDALONE_WASM=1"
        "SHELL:-sWASM=1"
        "SHELL:--no-entry"
        "SHELL:-sEXPORTED_FUNCTIONS=${EXPORTED_FUNCS}"
        "SHELL:-sERROR_ON_UNDEFINED_SYMBOLS=1"
        "SHELL:-sFILESYSTEM=0"
        "SHELL:-sDISABLE_EXCEPTION_CATCHING=1"
        "SHELL:-sALLOW_MEMORY_GROWTH=0"
        "SHELL:-sINITIAL_MEMORY=16MB"
        "SHELL:-sSUPPORT_LONGJMP=0"
        "SHELL:-sSUPPORT_ERRNO=0"
        "SHELL:-std=c++20"
        "SHELL:-O2"
        "SHELL:-g0"
    )

    set_target_properties(lib PROPERTIES OUTPUT_NAME "lib" SUFFIX ".wasm")
endif()
"#;

fn parse_identity_from_publish_output(publish_output: &str) -> Result<String> {
    let re = Regex::new(r"identity: ([0-9a-fA-F]+)").unwrap();
    re.captures(publish_output)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .context("Failed to parse database identity from publish output")
}

/// A smoketest instance that manages a SpacetimeDB server and module project.
pub struct Smoketest {
    /// The SpacetimeDB server guard (stops server on drop).
    /// None when running against a remote server.
    pub guard: Option<SpacetimeDbGuard>,
    /// Owns a copied fixture data directory, if this smoketest was started from one.
    _data_dir_fixture: Option<tempfile::TempDir>,
    /// Temporary directory containing the module project.
    pub project_dir: tempfile::TempDir,
    /// Additional features for the spacetimedb bindings dependency.
    pub bindings_features: Vec<String>,
    /// Additional dependencies to add to the module's Cargo.toml.
    pub extra_deps: String,
    /// Database identity after publishing (if any).
    pub database_identity: Option<String>,
    /// The server URL (e.g., "http://127.0.0.1:3000").
    pub server_url: String,
    /// Path to the test-specific CLI config file (isolates tests from user config).
    pub config_path: std::path::PathBuf,
    /// Unique module name for this test instance.
    /// Used to avoid wasm output conflicts when tests run in parallel.
    module_name: String,
    /// Path to pre-compiled WASM file (if using precompiled_module).
    precompiled_wasm_path: Option<PathBuf>,
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

pub struct PublishBuilder<'a> {
    smoketest: &'a mut Smoketest,
    name: Option<String>,
    clear: bool,
    break_clients: bool,
    num_replicas: Option<u32>,
    organization: Option<String>,
    force: bool,
    stdin_input: Option<String>,
    source: Option<ModuleSource>,
}

#[derive(Clone, Copy, Debug)]
pub enum ModuleLanguage {
    TypeScript,
    CSharp,
    Cpp,
}

struct ModuleSource {
    language: ModuleLanguage,
    project_dir_name: String,
    module_source: String,
}

impl<'a> PublishBuilder<'a> {
    fn new(smoketest: &'a mut Smoketest) -> Self {
        Self {
            smoketest,
            name: None,
            clear: false,
            break_clients: false,
            num_replicas: None,
            organization: None,
            force: true,
            stdin_input: None,
            source: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn clear(mut self, clear: bool) -> Self {
        self.clear = clear;
        self
    }

    pub fn break_clients(mut self, break_clients: bool) -> Self {
        self.break_clients = break_clients;
        self
    }

    pub fn num_replicas(mut self, num_replicas: u32) -> Self {
        self.num_replicas = Some(num_replicas);
        self
    }

    pub fn organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    pub fn stdin(mut self, stdin_input: impl Into<String>) -> Self {
        self.force = false;
        self.stdin_input = Some(stdin_input.into());
        self
    }

    pub fn current_database(mut self) -> Result<Self> {
        let identity = self
            .smoketest
            .database_identity
            .as_ref()
            .context("No database published yet")?
            .clone();
        self.name = Some(identity);
        Ok(self)
    }

    pub fn source(
        mut self,
        language: ModuleLanguage,
        project_dir_name: impl Into<String>,
        module_source: impl Into<String>,
    ) -> Self {
        self.source = Some(ModuleSource {
            language,
            project_dir_name: project_dir_name.into(),
            module_source: module_source.into(),
        });
        self
    }

    pub fn run(self) -> Result<String> {
        let PublishBuilder {
            smoketest,
            name,
            clear,
            break_clients,
            num_replicas,
            organization,
            force,
            stdin_input,
            source,
        } = self;

        if let Some(source) = source {
            let module_name = name.as_deref().context("No module name provided for source publish")?;
            return match source.language {
                ModuleLanguage::TypeScript => smoketest.publish_typescript_module_source_internal(
                    &source.project_dir_name,
                    module_name,
                    &source.module_source,
                    clear,
                ),
                ModuleLanguage::CSharp => smoketest.publish_csharp_module_source_internal(
                    &source.project_dir_name,
                    module_name,
                    &source.module_source,
                    clear,
                ),
                ModuleLanguage::Cpp => smoketest.publish_cpp_module_source_internal(
                    &source.project_dir_name,
                    module_name,
                    &source.module_source,
                    clear,
                ),
            };
        }

        smoketest.publish_module_internal(
            name.as_deref(),
            clear,
            break_clients,
            num_replicas,
            organization.as_deref(),
            force,
            stdin_input.as_deref(),
        )
    }
}

pub struct SubscribeBuilder<'a> {
    smoketest: &'a Smoketest,
    database: Option<String>,
    queries: Vec<String>,
    expected_rows: Option<usize>,
    confirmed: Option<bool>,
}

impl<'a> SubscribeBuilder<'a> {
    fn new(smoketest: &'a Smoketest, queries: &[&str]) -> Self {
        Self {
            smoketest,
            database: None,
            queries: queries.iter().map(|query| query.to_string()).collect(),
            expected_rows: None,
            confirmed: None,
        }
    }

    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    pub fn expect_rows(mut self, expected_rows: usize) -> Self {
        self.expected_rows = Some(expected_rows);
        self
    }

    pub fn confirmed(mut self, confirmed: bool) -> Self {
        self.confirmed = Some(confirmed);
        self
    }

    pub fn run(self) -> Result<Vec<serde_json::Value>> {
        let start = Instant::now();
        let owned_identity;
        let database = if let Some(database) = self.database.as_deref() {
            database
        } else {
            owned_identity = self
                .smoketest
                .database_identity
                .as_ref()
                .context("No database published")?
                .clone();
            &owned_identity
        };
        let queries = self.queries.iter().map(String::as_str).collect::<Vec<_>>();
        self.smoketest
            .subscribe_on_impl(database, &queries, self.expected_rows, self.confirmed, start)
    }

    pub fn background(self) -> Result<SubscriptionHandle> {
        let owned_identity;
        let database = if let Some(database) = self.database.as_deref() {
            database
        } else {
            owned_identity = self
                .smoketest
                .database_identity
                .as_ref()
                .context("No database published")?
                .clone();
            &owned_identity
        };
        let queries = self.queries.iter().map(String::as_str).collect::<Vec<_>>();
        self.smoketest
            .subscribe_background_on_impl(database, &queries, self.expected_rows, self.confirmed)
    }
}

/// Builder for creating `Smoketest` instances.
pub struct SmoketestBuilder {
    module_code: Option<String>,
    precompiled_module: Option<String>,
    data_dir_fixture: Option<DataDirFixture>,
    bindings_features: Vec<String>,
    extra_deps: String,
    autopublish: bool,
    pg_port: Option<u16>,
    server_url_override: Option<String>,
}

struct DataDirFixture {
    path: PathBuf,
    database_identity: String,
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
            precompiled_module: None,
            data_dir_fixture: None,
            bindings_features: vec!["unstable".to_string()],
            extra_deps: String::new(),
            autopublish: true,
            pg_port: None,
            server_url_override: None,
        }
    }

    pub fn server_url(mut self, url: &str) -> Self {
        self.server_url_override = Some(url.to_string());
        self
    }

    /// Starts the local server from a copy of a persisted standalone data directory fixture.
    ///
    /// The fixture directory is copied to a temporary directory before startup so tests can
    /// freely mutate it. Tests using this should normally also call `autopublish(false)`.
    pub fn data_dir_fixture(mut self, path: impl AsRef<Path>, database_identity: impl Into<String>) -> Self {
        self.data_dir_fixture = Some(DataDirFixture {
            path: path.as_ref().to_path_buf(),
            database_identity: database_identity.into(),
        });
        self
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

    /// Uses a pre-compiled module instead of runtime compilation.
    ///
    /// Pre-compiled modules are built during the warmup phase and stored in
    /// `crates/smoketests/modules/target/`. This eliminates per-test compilation
    /// overhead for static modules.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let test = Smoketest::builder()
    ///     .precompiled_module("filtering")
    ///     .build();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the module name is not found in the registry.
    pub fn precompiled_module(mut self, name: &str) -> Self {
        self.precompiled_module = Some(name.to_string());
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
    /// This spawns a SpacetimeDB server (unless `SPACETIME_REMOTE_SERVER` is set),
    /// creates a temporary project directory, writes the module code, and optionally
    /// publishes the module.
    ///
    /// When `SPACETIME_REMOTE_SERVER` is set, tests run against the remote server
    /// instead of spawning a local server. Tests that require local server control
    /// (like restart tests) should use `skip_if_remote!()` at the start.
    ///
    /// # Panics
    ///
    /// Panics if the CLI/standalone binaries haven't been built or are stale.
    /// Run `cargo smoketest prepare` to build binaries before running tests.
    pub fn build(self) -> Smoketest {
        // Check binaries first - this will panic with a helpful message if missing/stale
        let _ = ensure_binaries_built();
        let build_start = Instant::now();

        let fixture_identity = self
            .data_dir_fixture
            .as_ref()
            .map(|fixture| fixture.database_identity.clone());

        // Check if we're running against a remote server
        let (guard, server_url, data_dir_fixture) = if let Some(fixture) = self.data_dir_fixture.as_ref() {
            if self.server_url_override.is_some() || remote_server_url().is_some() {
                panic!("data_dir_fixture requires a local server managed by the smoketest harness");
            }

            let temp_dir = tempfile::tempdir().expect("Failed to create temp data fixture directory");
            let copy_options = fs_extra::dir::CopyOptions {
                content_only: true,
                overwrite: true,
                ..Default::default()
            };
            fs_extra::dir::copy(&fixture.path, temp_dir.path(), &copy_options).unwrap_or_else(|err| {
                panic!(
                    "failed to copy data dir fixture from {} to {}: {err:#}",
                    fixture.path.display(),
                    temp_dir.path().display()
                )
            });

            let guard = timed!(
                "server spawn from data dir fixture",
                SpacetimeDbGuard::spawn_with_data_dir(temp_dir.path().to_path_buf(), self.pg_port)
            );
            let url = guard.host_url.clone();
            (Some(guard), url, Some(temp_dir))
        } else if let Some(url) = self.server_url_override {
            eprintln!("[REMOTE] Using explicit server URL: {}", url);
            (None, url, None)
        } else if let Some(remote_url) = remote_server_url() {
            eprintln!("[REMOTE] Using remote server: {}", remote_url);
            (None, remote_url, None)
        } else {
            let guard = timed!(
                "server spawn",
                SpacetimeDbGuard::spawn_in_temp_data_dir_with_pg_port(self.pg_port)
            );
            let url = guard.host_url.clone();
            (Some(guard), url, None)
        };

        let project_dir = tempfile::tempdir().expect("Failed to create temp project directory");

        // Check if we're using a pre-compiled module
        let precompiled_wasm_path = self.precompiled_module.as_ref().map(|name| {
            let path = modules::precompiled_module(name);
            if !path.exists() {
                panic!(
                    "Pre-compiled module '{}' not found at {:?}. \
                    Run `cargo smoketest` to build pre-compiled modules during warmup.",
                    name, path
                );
            }
            eprintln!("[PRECOMPILED] Using pre-compiled module: {}", name);
            path
        });

        let project_setup_start = Instant::now();

        // Generate a unique module name to avoid wasm output conflicts in parallel tests.
        // The format is smoketest_module_{random} which produces smoketest_module_{random}.wasm
        let module_name = format!("smoketest_module_{}", random_string());

        let config_path = project_dir.path().join("config.toml");
        if let Ok(base_config_path) = std::env::var("SPACETIME_SMOKETEST_BASE_CONFIG_PATH") {
            fs::copy(&base_config_path, &config_path)
                .unwrap_or_else(|err| panic!("failed to copy base smoketest config from {base_config_path}: {err:#}"));
        }
        let mut smoketest = Smoketest {
            guard,
            _data_dir_fixture: data_dir_fixture,
            project_dir,
            database_identity: fixture_identity,
            server_url,
            config_path,
            module_name,
            precompiled_wasm_path: precompiled_wasm_path.clone(),
            bindings_features: self.bindings_features.clone(),
            extra_deps: self.extra_deps.clone(),
        };

        // Only set up project structure if not using precompiled module
        if precompiled_wasm_path.is_none() {
            let module_code = self.module_code.unwrap_or_else(|| {
                r#"use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
pub fn noop(_ctx: &ReducerContext) {}
"#
                .to_string()
            });
            smoketest.write_module_code(&module_code).unwrap();

            eprintln!("[TIMING] project setup: {:?}", project_setup_start.elapsed());
        }

        if self.autopublish {
            smoketest.publish().run().expect("Failed to publish module");
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
    ///
    /// # Panics
    ///
    /// Panics if running against a remote server (no local server to restart).
    /// Tests that call this method should use `skip_if_remote!()` at the start.
    pub fn restart_server(&mut self) {
        let guard = self.guard.as_mut().expect(
            "Cannot restart server: running against remote server. Use skip_if_remote!() at the start of this test.",
        );
        guard.restart();
        // Update server_url since the port may have changed
        self.server_url = guard.host_url.clone();
    }

    /// Returns the server host (without protocol), e.g., "127.0.0.1:3000".
    pub fn server_host(&self) -> &str {
        let (_, host) = split_server_url(&self.server_url);
        host
    }

    /// Returns the PostgreSQL wire protocol port, if enabled.
    ///
    /// Returns None if running against a remote server or if PostgreSQL
    /// wire protocol wasn't enabled for the local server.
    pub fn pg_port(&self) -> Option<u16> {
        self.guard.as_ref().and_then(|g| g.pg_port)
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

    pub fn login_with_token(&self, token: &str) -> Result<()> {
        let (protocol, host) = split_server_url(&self.server_url);
        let config_str = format!(
            "default_server = \"localhost\"\n\nspacetimedb_token = \"{}\"\n\n[[server_configs]]\nnickname = \"localhost\"\nhost = \"{}\"\nprotocol = \"{}\"\n",
            token, host, protocol
        );
        fs::write(&self.config_path, config_str).context("Failed to write config.toml")?;
        Ok(())
    }

    /// Runs psql command against the PostgreSQL wire protocol server.
    ///
    /// Returns the output on success, or an error with stderr on failure.
    pub fn psql(&self, database: &str, sql: &str) -> Result<String> {
        let token = self.read_token()?;
        self.psql_with_token(database, &token, sql)
    }

    pub fn psql_with_token(&self, database: &str, token: &str, sql: &str) -> Result<String> {
        let pg_port = self.pg_port().context("PostgreSQL wire protocol not enabled")?;

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
            .env("PGPASSWORD", token)
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

    /// Runs a spacetime CLI command with stdin input.
    ///
    /// Returns the command output. The command is run but not yet asserted.
    /// Uses --config-path to isolate test config from user config.
    /// Callers should pass `--server` explicitly when the command needs it.
    pub fn spacetime_cmd_with_stdin(&self, args: &[&str], stdin_input: &str) -> Output {
        let start = Instant::now();
        let cli_path = ensure_binaries_built();
        let mut child = Command::new(&cli_path)
            .arg("--config-path")
            .arg(&self.config_path)
            .args(args)
            .current_dir(self.project_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn spacetime command");

        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().expect("missing child stdin");
            stdin
                .write_all(stdin_input.as_bytes())
                .expect("Failed to write spacetime stdin");
        }

        let output = child.wait_with_output().expect("Failed to wait for spacetime command");

        let cmd_name = args.first().unwrap_or(&"unknown");
        eprintln!("[TIMING] spacetime {} (stdin): {:?}", cmd_name, start.elapsed());
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

    /// Runs a spacetime CLI command with stdin and returns stdout as a string.
    ///
    /// Panics if the command fails.
    /// Callers should pass `--server` explicitly when the command needs it.
    pub fn spacetime_with_stdin(&self, args: &[&str], stdin_input: &str) -> Result<String> {
        let output = self.spacetime_cmd_with_stdin(args, stdin_input);
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

    fn publish_typescript_module_source_internal(
        &mut self,
        project_dir_name: &str,
        module_name: &str,
        module_source: &str,
        clear: bool,
    ) -> Result<String> {
        let module_root = self.project_dir.path().join(project_dir_name);
        let module_root_str = module_root.to_str().context("Invalid TypeScript project path")?;
        self.spacetime(&[
            "init",
            "--non-interactive",
            "--lang",
            "typescript",
            "--project-path",
            module_root_str,
            module_name,
        ])?;

        let module_path = module_root.join("spacetimedb");
        fs::write(module_path.join("src/index.ts"), module_source).context("Failed to write TypeScript module code")?;

        build_typescript_sdk()?;
        let _ = pnpm(&["uninstall", "spacetimedb"], &module_path);

        let ts_bindings = workspace_root().join("crates/bindings-typescript");
        let ts_bindings_path = ts_bindings.to_str().context("Invalid TypeScript bindings path")?;
        pnpm(&["install", ts_bindings_path], &module_path)?;

        let module_path_str = module_path.to_str().context("Invalid TypeScript module path")?;
        let mut publish_args = vec![
            "publish",
            "--server",
            &self.server_url,
            "--module-path",
            module_path_str,
            "--yes",
        ];
        if clear {
            publish_args.push("--clear-database");
        }
        publish_args.push(module_name);
        let publish_output = self.spacetime(&publish_args)?;

        let re = Regex::new(r"identity: ([0-9a-fA-F]+)").unwrap();
        let identity = re
            .captures(&publish_output)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .context("Failed to parse database identity from publish output")?;
        self.database_identity = Some(identity.clone());

        Ok(identity)
    }

    fn publish_csharp_module_source_internal(
        &mut self,
        project_dir_name: &str,
        module_name: &str,
        module_source: &str,
        clear: bool,
    ) -> Result<String> {
        let module_root = self.project_dir.path().join(project_dir_name);
        let module_root_str = module_root.to_str().context("Invalid C# project path")?;
        self.spacetime(&[
            "init",
            "--non-interactive",
            "--lang",
            "csharp",
            "--project-path",
            module_root_str,
            module_name,
        ])?;

        let module_path = module_root.join("spacetimedb");
        fs::write(module_path.join("Lib.cs"), module_source).context("Failed to write C# module code")?;
        csharp::prepare_csharp_module(&module_path)?;

        let module_path_str = module_path.to_str().context("Invalid C# module path")?;
        let mut publish_args = vec![
            "publish",
            "--server",
            &self.server_url,
            "--module-path",
            module_path_str,
            "--yes",
        ];
        if clear {
            publish_args.push("--clear-database");
        }
        publish_args.push(module_name);
        let publish_output = self.spacetime(&publish_args)?;
        csharp::verify_csharp_module_restore(&module_path)?;

        let identity = parse_identity_from_publish_output(&publish_output)?;
        self.database_identity = Some(identity.clone());

        Ok(identity)
    }

    fn publish_cpp_module_source_internal(
        &mut self,
        project_dir_name: &str,
        module_name: &str,
        module_source: &str,
        clear: bool,
    ) -> Result<String> {
        let module_path = self.project_dir.path().join(project_dir_name);
        let src_dir = module_path.join("src");
        fs::create_dir_all(&src_dir).context("Failed to create C++ source directory")?;

        let bindings_cpp_path = workspace_root()
            .join("crates/bindings-cpp")
            .display()
            .to_string()
            .replace('\\', "/");
        let cmakelists = CPP_SMOKETEST_CMAKELISTS.replace("@SPACETIMEDB_CPP_LIBRARY_PATH@", &bindings_cpp_path);

        fs::write(module_path.join("CMakeLists.txt"), cmakelists).context("Failed to write C++ CMakeLists.txt")?;
        fs::write(src_dir.join("lib.cpp"), module_source).context("Failed to write C++ module code")?;

        let module_path_str = module_path.to_str().context("Invalid C++ module path")?;
        let mut publish_args = vec![
            "publish",
            "--server",
            &self.server_url,
            "--module-path",
            module_path_str,
            "--yes",
        ];
        if clear {
            publish_args.push("--clear-database");
        }
        publish_args.push(module_name);
        let publish_output = self.spacetime(&publish_args)?;

        let identity = parse_identity_from_publish_output(&publish_output)?;
        self.database_identity = Some(identity.clone());

        Ok(identity)
    }

    /// Writes new module code to the project.
    ///
    /// This switches from precompiled mode to runtime compilation mode.
    /// If the project structure doesn't exist (e.g., started with `precompiled_module()`),
    /// it will be created on demand.
    pub fn write_module_code(&mut self, code: &str) -> Result<()> {
        // Clear precompiled module path so we use the source code instead
        self.precompiled_wasm_path = None;

        // Create project structure on demand if it doesn't exist
        // (happens when test started with precompiled_module)
        let src_dir = self.project_dir.path().join("src");
        if !src_dir.exists() {
            fs::create_dir_all(&src_dir).context("Failed to create src directory")?;

            // Write Cargo.toml with default settings
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
                self.module_name, bindings_path_str, features_str, self.extra_deps
            );
            fs::write(self.project_dir.path().join("Cargo.toml"), cargo_toml).context("Failed to write Cargo.toml")?;

            // Copy rust-toolchain.toml
            let toolchain_src = workspace_root.join("rust-toolchain.toml");
            if toolchain_src.exists() {
                fs::copy(&toolchain_src, self.project_dir.path().join("rust-toolchain.toml"))
                    .context("Failed to copy rust-toolchain.toml")?;
            }
        }

        fs::write(self.project_dir.path().join("src/lib.rs"), code).context("Failed to write module code")?;
        Ok(())
    }

    /// Switches to using a precompiled module.
    ///
    /// After calling this, subsequent `publish_module*` calls will use the
    /// precompiled WASM file instead of building from source.
    pub fn use_precompiled_module(&mut self, name: &str) {
        let path = modules::precompiled_module(name);
        if !path.exists() {
            panic!(
                "Pre-compiled module '{}' not found at {:?}. \
                Run `cargo smoketest` to build pre-compiled modules during warmup.",
                name, path
            );
        }
        eprintln!("[PRECOMPILED] Switching to pre-compiled module: {}", name);
        self.precompiled_wasm_path = Some(path);
    }

    /// Switches to using an explicit precompiled WASM path.
    ///
    /// After calling this, subsequent `publish_module*` calls will use this
    /// WASM file instead of building from source.
    pub fn use_precompiled_wasm_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Pre-compiled wasm not found at {}", path.display());
        }
        eprintln!("[PRECOMPILED] Switching to explicit wasm path: {}", path.display());
        self.precompiled_wasm_path = Some(path.to_path_buf());
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
        cmd.args(["build", "--module-path", project_path])
            .current_dir(self.project_dir.path())
            .env("CARGO_TARGET_DIR", shared_target_dir());

        let output = cmd.output().expect("Failed to execute spacetime build");
        eprintln!("[TIMING] spacetime build: {:?}", start.elapsed());
        output
    }

    pub fn publish(&mut self) -> PublishBuilder<'_> {
        PublishBuilder::new(self)
    }

    /// Publishes the module and stores the database identity.
    ///
    /// If `name` is provided, the database will be published with that name.
    /// If `clear` is true, the database will be cleared before publishing.
    /// If `force` is false, the publish command will not pass `--yes`, so interactive prompts are not suppressed.
    /// If `stdin_input` is provided, it will be passed to the CLI for interactive prompts.
    ///
    /// When `name` is an existing database identity, this re-publishes to that database, which is useful for testing
    /// auto-migrations where you want to update the module without clearing the database.
    #[allow(clippy::too_many_arguments)]
    fn publish_module_internal(
        &mut self,
        name: Option<&str>,
        clear: bool,
        break_clients: bool,
        num_replicas: Option<u32>,
        organization: Option<&str>,
        force: bool,
        stdin_input: Option<&str>,
    ) -> Result<String> {
        let start = Instant::now();

        // Determine the WASM path - either precompiled or build it
        let wasm_path_str = if let Some(ref precompiled_path) = self.precompiled_wasm_path {
            // Use pre-compiled WASM directly (no build needed)
            eprintln!("[TIMING] spacetime build: skipped (using precompiled)");
            precompiled_path.to_str().unwrap().to_string()
        } else {
            // Build the WASM module from source
            let project_path = self.project_dir.path().to_str().unwrap().to_string();
            let build_start = Instant::now();
            let cli_path = ensure_binaries_built();
            let target_dir = shared_target_dir();

            let mut build_cmd = Command::new(&cli_path);
            build_cmd
                .args(["build", "--module-path", &project_path])
                .current_dir(self.project_dir.path())
                .env("CARGO_TARGET_DIR", &target_dir);

            let build_output = build_cmd.output().expect("Failed to execute spacetime build");
            eprintln!("[TIMING] spacetime build: {:?}", build_start.elapsed());

            if !build_output.status.success() {
                bail!(
                    "spacetime build failed:\nstdout: {}\nstderr: {}",
                    String::from_utf8_lossy(&build_output.stdout),
                    String::from_utf8_lossy(&build_output.stderr)
                );
            }

            // Construct the wasm path using the unique module name
            let wasm_filename = format!("{}.wasm", self.module_name);
            let wasm_path = target_dir.join("wasm32-unknown-unknown/release").join(&wasm_filename);
            wasm_path.to_str().unwrap().to_string()
        };

        // Now publish with --bin-path to skip rebuild
        let publish_start = Instant::now();
        let mut args = vec!["publish", "--server", &self.server_url, "--bin-path", &wasm_path_str];

        if force {
            args.push("--yes");
        }

        if clear {
            args.push("--clear-database");
        }

        if break_clients {
            args.push("--break-clients");
        }

        let num_replicas_owned = num_replicas.map(|n| n.to_string());
        if let Some(n) = num_replicas_owned.as_ref() {
            args.push("--num-replicas");
            args.push(n);
        }

        if let Some(org) = organization {
            args.push("--organization");
            args.push(org);
        }

        if let Some(n) = name {
            args.push(n);
        }

        let output = match stdin_input {
            Some(stdin_input) => self.spacetime_with_stdin(&args, stdin_input)?,
            None => self.spacetime(&args)?,
        };
        eprintln!(
            "[TIMING] spacetime publish (after build): {:?}",
            publish_start.elapsed()
        );
        eprintln!("[TIMING] publish_module total: {:?}", start.elapsed());

        parse_identity_from_publish_output(&output).inspect(|identity| {
            self.database_identity = Some(identity.clone());
        })
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

    /// Calls a reducer anonymously (without authentication).
    pub fn call_anon(&self, name: &str, args: &[&str]) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        let mut cmd_args = vec![
            "call",
            "--anonymous",
            "--server",
            &self.server_url,
            "--",
            identity.as_str(),
            name,
        ];
        cmd_args.extend(args);

        self.spacetime(&cmd_args)
    }

    /// Describes the database schema.
    pub fn describe(&self) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&["describe", "--server", &self.server_url, identity.as_str()])
    }

    /// Describes the database schema anonymously (requires --json).
    pub fn describe_anon(&self) -> Result<String> {
        let identity = self.database_identity.as_ref().context("No database published")?;

        self.spacetime(&[
            "describe",
            "--anonymous",
            "--json",
            "--server",
            &self.server_url,
            identity.as_str(),
        ])
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
            "true",
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
        // Remote smoketest configs edit the "localhost" server alias to point at the
        // remote URL, matching the old Python smoketest runner.
        let login_server = if is_remote_server() {
            "localhost"
        } else {
            &self.server_url
        };
        let output = Command::new(&cli_path)
            .args([
                "--config-path",
                config_path_str,
                "login",
                "--server-issued-login",
                login_server,
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
        self.api_call_internal(method, path, body, "")
    }

    /// Makes an HTTP API call with a JSON body.
    pub fn api_call_json(&self, method: &str, path: &str, json_body: &str) -> Result<ApiResponse> {
        self.api_call_internal(
            method,
            path,
            Some(json_body.as_bytes()),
            "Content-Type: application/json\r\n",
        )
    }

    /// Internal HTTP API call implementation.
    fn api_call_internal(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        extra_headers: &str,
    ) -> Result<ApiResponse> {
        let token = self.read_token()?;
        let method = reqwest::Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
        let url = format!("{}{}", self.server_url.trim_end_matches('/'), path);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;
        let mut request = client.request(method, url).bearer_auth(token);
        if extra_headers.contains("Content-Type: application/json") {
            request = request.header(reqwest::header::CONTENT_TYPE, "application/json");
        }
        if let Some(body) = body {
            request = request.body(body.to_vec());
        }

        let response = request.send().context("HTTP request failed")?;
        let status_code = response.status().as_u16();
        let body = response.bytes().context("failed to read HTTP response body")?.to_vec();

        Ok(ApiResponse { status_code, body })
    }

    pub fn subscribe(&self, queries: &[&str]) -> SubscribeBuilder<'_> {
        SubscribeBuilder::new(self, queries)
    }

    fn subscribe_on_impl(
        &self,
        database: &str,
        queries: &[&str],
        n: Option<usize>,
        confirmed: Option<bool>,
        start: Instant,
    ) -> Result<Vec<serde_json::Value>> {
        let config_path_str = self.config_path.to_str().unwrap();

        let cli_path = ensure_binaries_built();
        let mut cmd = Command::new(&cli_path);
        let mut args = vec![
            "--config-path".to_string(),
            config_path_str.to_string(),
            "subscribe".to_string(),
            "--server".to_string(),
            self.server_url.to_string(),
            database.to_string(),
            "-t".to_string(),
            "30".to_string(),
            "-n".to_string(),
        ];
        if let Some(n) = n {
            let n_str = n.to_string();
            args.push(n_str);
        }
        args.push("--print-initial-update".to_string());
        if let Some(confirmed) = confirmed {
            args.push("--confirmed".to_string());
            args.push(confirmed.to_string());
        }
        args.push("--".to_string());
        cmd.args(&args)
            .args(queries)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().context("Failed to run subscribe command")?;
        eprintln!("[TIMING] subscribe (n={:?}): {:?}", n, start.elapsed());

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

    fn subscribe_background_on_impl(
        &self,
        database: &str,
        queries: &[&str],
        n: Option<usize>,
        confirmed: Option<bool>,
    ) -> Result<SubscriptionHandle> {
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
            database.to_string(),
            "-t".to_string(),
            "30".to_string(),
            "--print-initial-update".to_string(),
        ];
        if let Some(n) = n {
            args.push("-n".to_string());
            args.push(n.to_string());
        }
        if let Some(confirmed) = confirmed {
            args.push("--confirmed".to_string());
            args.push(confirmed.to_string());
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
    n: Option<usize>,
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
            self.n.map(|n| n.to_string()).unwrap_or_else(|| "none".to_string()),
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

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            Err(_) => {}
        }
    }
}

fn split_server_url(server_url: &str) -> (&str, &str) {
    if let Some(host) = server_url.strip_prefix("http://") {
        ("http", host)
    } else if let Some(host) = server_url.strip_prefix("https://") {
        ("https", host)
    } else {
        ("http", server_url)
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
