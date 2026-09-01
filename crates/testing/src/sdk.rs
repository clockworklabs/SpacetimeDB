use duct::cmd;
use rand::seq::IteratorRandom;
use spacetimedb::messages::control_db::HostType;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_guard::SpacetimeDbGuard;
use spacetimedb_paths::{RootDir, SpacetimePaths};
use std::fs::{copy, create_dir_all, read_dir};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::invoke_cli;
use crate::modules::{CompilationMode, CompiledModule};
use tempfile::TempDir;

struct SdkTestPaths {
    paths: SpacetimePaths,
    _root: TempDir,
}

impl SdkTestPaths {
    fn new() -> Self {
        let root = TempDir::with_prefix("stdb-sdk-test").expect("Failed to create tempdir");
        let paths = SpacetimePaths::from_root_dir(&RootDir(root.path().to_path_buf()));
        Self { paths, _root: root }
    }
}

pub struct Test {
    /// A human-readable name for this test.
    #[allow(dead_code)] // TODO: should we just remove this now that it's unused?
    name: String,

    /// Must name a module in the SpacetimeDB/modules directory.
    module_name: String,

    /// An arbitrary path to the client project.
    /// For unrealcpp this should be the .uproject root directory.
    client_project: String,

    /// A language suitable for the `spacetime generate` CLI command.
    ///
    /// The string `"unrealcpp"` is recognized and treated differently here
    /// because code-generation takes different arguments for Unreal client projects.
    /// Tests written for the Unreal client SDK must specify exactly `"unrealcpp"`,
    /// not any of the aliases the SpacetimeDB CLI's `generate` command would accept.
    generate_language: String,

    /// If true, pass `--include-private` to `spacetime generate` to include bindings for private items.
    generate_include_private: bool,

    /// A relative path within the `client_project` to place the module bindings.
    ///
    /// Usually `src/module_bindings`.
    ///
    /// For Unreal tests (i.e. when `generate_language == "unrealcpp"`),
    /// this is instead the Unreal module name, and so should be a non-path string.
    /// In this case, it will usually be `"TestClient"`.
    generate_subdir: String,

    /// A shell command to compile the client project.
    ///
    /// Will run with access to the env var `SPACETIME_SDK_TEST_CLIENT_PROJECT`
    /// bound to the `client_project` path.
    compile_command: String,

    /// A shell command to run the client project.
    ///
    /// Will run with access to the env vars:
    /// - `SPACETIME_SDK_TEST_CLIENT_PROJECT` bound to the `client_project` path.
    /// - `SPACETIME_SDK_TEST_DB_NAME` bound to the database identity or name.
    /// - `SPACETIME_SDK_TEST_SERVER_URL` bound to the server URL for this test.
    run_command: String,

    prepared_client: Option<PreparedClient>,
    prepared_client_key: Option<String>,
}

#[derive(Clone)]
// These are artifact execution strategies, not SDK test modes. The test suite
// still has only native and browser modes; `Node` packages existing TypeScript
// clients that run in both modes.
enum PreparedClient {
    Native { binary_name: String, args: Vec<String> },
    Browser { artifact_name: String, selector: String },
    Node { entrypoint: PathBuf, args: Vec<String> },
}

pub const TEST_MODULE_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_MODULE_PROJECT";
pub const TEST_DB_NAME_ENV_VAR: &str = "SPACETIME_SDK_TEST_DB_NAME";
pub const TEST_SERVER_URL_ENV_VAR: &str = "SPACETIME_SDK_TEST_SERVER_URL";
pub const TEST_CLIENT_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_CLIENT_PROJECT";
pub const PRECOMPILED_MODULE_DIR_ENV_VAR: &str = "SPACETIME_SDK_TEST_MODULE_DIR";
pub const PREPARED_CLIENT_DIR_ENV_VAR: &str = "SPACETIME_SDK_TEST_CLIENT_DIR";
pub const PREPARE_CLIENT_DIR_ENV_VAR: &str = "SPACETIME_SDK_TEST_PREPARE_CLIENT_DIR";
pub const TEST_WORKSPACE_ROOT_ENV_VAR: &str = "SPACETIME_SDK_TEST_WORKSPACE_ROOT";

fn language_is_unreal(language: &str) -> bool {
    language.eq_ignore_ascii_case("unrealcpp")
}

impl Test {
    pub fn builder() -> TestBuilder {
        TestBuilder::default()
    }
    pub fn run(self) {
        let sdk_paths = SdkTestPaths::new();
        let paths = &sdk_paths.paths;

        let (file, host_type) = compile_module(&self.module_name);

        let prepared_client_dir = std::env::var_os(PREPARED_CLIENT_DIR_ENV_VAR).map(PathBuf::from);
        if prepared_client_dir.is_none() {
            self.generate_bindings(paths, &file, host_type);
            compile_client(&self.compile_command, &self.client_project, &self.module_name);
        }

        let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
        let server_url = guard.host_url.as_str();
        let db_name = publish_module(paths, server_url, &file, host_type);

        if let Some(prepared_client_dir) = prepared_client_dir {
            self.run_prepared_client(&prepared_client_dir, server_url, &db_name);
        } else {
            run_client(&self.run_command, &self.client_project, server_url, &db_name);
        }
    }

    pub fn prepare(self) {
        let output_dir = PathBuf::from(
            std::env::var_os(PREPARE_CLIENT_DIR_ENV_VAR)
                .unwrap_or_else(|| panic!("{PREPARE_CLIENT_DIR_ENV_VAR} is not set")),
        );
        let sdk_paths = SdkTestPaths::new();
        let (file, host_type) = compile_module(&self.module_name);
        self.generate_bindings(&sdk_paths.paths, &file, host_type);
        compile_client(&self.compile_command, &self.client_project, &self.module_name);
        self.export_prepared_client(&output_dir);
    }

    fn generate_bindings(&self, paths: &SpacetimePaths, file: &str, host_type: HostType) {
        generate_bindings(
            paths,
            &self.generate_language,
            file,
            host_type,
            &self.client_project,
            &self.generate_subdir,
            self.generate_include_private,
        );
    }

    fn client_artifact_dir(&self, root: &Path) -> PathBuf {
        let key = self.prepared_client_key.as_deref().unwrap_or_else(|| {
            Path::new(&self.client_project)
                .file_name()
                .and_then(|name| name.to_str())
                .expect("SDK client project should end in a UTF-8 directory name")
        });
        root.join(key)
    }

    fn export_prepared_client(&self, output_dir: &Path) {
        let prepared = self
            .prepared_client
            .as_ref()
            .expect("SDK test does not describe how to prepare its client");
        let artifact_dir = self.client_artifact_dir(output_dir);
        match prepared {
            PreparedClient::Native { binary_name, .. } => {
                let source = cargo_target_dir().join("debug").join(binary_name);
                copy_file(&source, &artifact_dir.join("bin").join(binary_name));
            }
            PreparedClient::Browser { artifact_name, .. } => {
                let package_name = Path::new(&self.client_project).file_name().unwrap();
                let source = Path::new(&self.client_project)
                    .join("target/sdk-test-web-bindgen")
                    .join(package_name);
                copy_dir(&source, &artifact_dir.join("web"));
                assert!(
                    artifact_dir.join("web").join(format!("{artifact_name}.cjs")).is_file(),
                    "Prepared browser client is missing its CommonJS entrypoint"
                );
            }
            PreparedClient::Node { entrypoint, .. } => {
                let source = Path::new(&self.client_project).join(entrypoint);
                copy_file(&source, &artifact_dir.join(entrypoint));
                copy_file(
                    &Path::new(&self.client_project).join("package.json"),
                    &artifact_dir.join("package.json"),
                );
            }
        }
    }

    fn run_prepared_client(&self, root: &Path, server_url: &str, db_name: &str) {
        let prepared = self
            .prepared_client
            .as_ref()
            .expect("SDK test does not describe how to run its prepared client");
        let artifact_dir = self.client_artifact_dir(root);
        let (exe, args) = match prepared {
            PreparedClient::Native { binary_name, args } => (
                artifact_dir.join("bin").join(binary_name).into_os_string(),
                args.iter().map(Into::into).collect(),
            ),
            PreparedClient::Browser {
                artifact_name,
                selector,
            } => {
                let js_module = artifact_dir.join("web").join(format!("{artifact_name}.cjs"));
                let script = browser_node_script(&js_module, selector);
                (
                    "node".into(),
                    vec!["--experimental-websocket".into(), "-e".into(), script.into()],
                )
            }
            PreparedClient::Node { entrypoint, args } => {
                let mut node_args = vec![artifact_dir.join(entrypoint).into_os_string()];
                node_args.extend(args.iter().map(Into::into));
                ("node".into(), node_args)
            }
        };

        run_client_command(exe, args, &self.client_project, server_url, db_name, &self.run_command);
    }
}

pub fn workspace_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.exists() {
        return path.to_path_buf();
    }

    let build_workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("spacetimedb-testing should be two directories below the workspace root");
    match path.strip_prefix(build_workspace_root) {
        Ok(relative) => runtime_workspace_root().join(relative),
        Err(_) => path.to_path_buf(),
    }
}

fn runtime_workspace_root() -> PathBuf {
    std::env::var_os(TEST_WORKSPACE_ROOT_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .expect("spacetimedb-testing should be two directories below the workspace root")
                .to_path_buf()
        })
}

fn cargo_target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_workspace_root().join("target"))
}

fn copy_file(source: &Path, destination: &Path) {
    create_dir_all(destination.parent().unwrap()).unwrap();
    copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "Failed to copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    });
}

fn copy_dir(source: &Path, destination: &Path) {
    create_dir_all(destination).unwrap();
    for entry in read_dir(source).unwrap_or_else(|error| panic!("Failed to read {}: {error}", source.display())) {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&source_path, &destination_path);
        } else {
            copy_file(&source_path, &destination_path);
        }
    }
}

fn browser_node_script(js_module: &Path, selector: &str) -> String {
    format!(
        "(async () => {{ \
         const m = require({js_module:?}); \
         if (m.default) {{ await m.default(); }} \
         const run = m.run || m.main || m.start; \
         if (!run) throw new Error(\"No exported run/main/start function from wasm module\"); \
         const dbName = process.env.SPACETIME_SDK_TEST_DB_NAME; \
         if (!dbName) throw new Error(\"Missing SPACETIME_SDK_TEST_DB_NAME\"); \
         const serverUrl = process.env.SPACETIME_SDK_TEST_SERVER_URL; \
         if (!serverUrl) throw new Error(\"Missing SPACETIME_SDK_TEST_SERVER_URL\"); \
         await run({selector:?}, dbName, serverUrl); \
         process.exit(0); \
         }})().catch((e) => {{ console.error(e); process.exit(1); }});"
    )
}

fn status_ok_or_panic(output: std::process::Output, command: &str, test_name: &str) {
    if !output.status.success() {
        panic!(
            "{}: Error running {:?}: exited with non-zero exit status {}. Output:\n{}",
            test_name,
            command,
            output.status,
            String::from_utf8_lossy(&output.stdout),
        );
    }
}

fn random_module_name() -> String {
    let mut rng = rand::rng();
    std::iter::repeat_with(|| ('a'..='z').chain('0'..='9').choose(&mut rng).unwrap())
        .take(16)
        .collect()
}

/// Memoize computing `body` based on `key` by storing the result in a [`HashMap`].
///
/// The hash map is protected by a [`Mutex`].
/// Only a single operator may be computing a value at a time.
/// Computing the values must not be re-entrant / recursive.
///
/// The key(s) of the hash map must already be in scope as variables.
///
/// The keys may be either a single variable or a tuple of variables.
///
/// The key types must be `'static`, `Clone`, `Eq` and `Hash`, as they'll be stored in a [`HashMap`].
///
/// Used in this file primarily for running expensive and side-effecting subprocesses
/// like compilation or code generation.
macro_rules! memoized {
    // Recursive case: rewrite a single `key` to be a 1-tuple `(key,)`.
    (|$key:ident: $key_ty:ty| -> $value_ty:ty $body:block) => {{
        memoized!(|($key,): ($key_ty,)| -> $value_ty $body)
    }};

    // Base case: keys are a tuple.
    (|($($key_tuple:ident),* $(,)?): $key_ty:ty| -> $value_ty:ty $body:block) => {{
        static MEMOIZED: Mutex<Option<HashMap<$key_ty, $value_ty>>> = Mutex::new(None);

        MEMOIZED
            .lock()
            .unwrap()
            .get_or_insert_default()
            .entry(($($key_tuple,)*))
            .or_insert_with_key(|($($key_tuple,)*)| -> $value_ty { $body })
            .clone()
    }};
}

// Note: this function is memoized to ensure we compile each module only once.
// Without this lock, if multiple `Test`s ran concurrently in the same process,
// the test harness would compile each module multiple times concurrently,
// which is bad both for performance reasons as well as can lead to errors
// with toolchains like .NET which don't expect parallel invocations
// of their build tools on the same project folder.
fn compile_module(module: &str) -> (String, HostType) {
    if let Some(module_dir) = std::env::var_os(PRECOMPILED_MODULE_DIR_ENV_VAR) {
        let module_dir = PathBuf::from(module_dir);
        for (extension, host_type) in [("wasm", HostType::Wasm), ("js", HostType::Js)] {
            let path = module_dir.join(module).with_extension(extension);
            if path.is_file() {
                return (path.to_string_lossy().into_owned(), host_type);
            }
        }
        panic!(
            "No precompiled SDK test module found for {module:?} in {}",
            module_dir.display()
        );
    }

    let module = module.to_owned();

    memoized!(|module: String| -> (String, HostType) {
        let module = CompiledModule::compile(module, CompilationMode::Debug);
        (module.path().to_str().unwrap().to_owned(), module.host_type)
    })
}

/// Compile all SDK test modules into a stable directory for archived CI tests.
pub fn build_precompiled_modules(output_dir: &Path) -> anyhow::Result<usize> {
    let workspace_root = std::env::current_dir()?;
    let modules_dir = workspace_root.join("modules");
    let mut module_names = std::fs::read_dir(&modules_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("sdk-test"))
        .collect::<Vec<_>>();
    module_names.sort();

    create_dir_all(output_dir)?;
    for module_name in &module_names {
        eprintln!("Building precompiled SDK test module {module_name}...");
        let module = CompiledModule::compile(module_name, CompilationMode::Debug);
        let extension = match module.host_type {
            HostType::Wasm => "wasm",
            HostType::Js => "js",
        };
        let destination = output_dir.join(module_name).with_extension(extension);
        std::fs::copy(module.path(), &destination)?;
    }

    Ok(module_names.len())
}

// Note: this function does not memoize because we want each test to publish the same
// module as a separate clean database instance for isolation purposes.
fn publish_module(paths: &SpacetimePaths, server_url: &str, wasm_file: &str, host_type: HostType) -> String {
    let name = random_module_name();
    invoke_cli(
        paths,
        &[
            "publish",
            "--anonymous",
            "--server",
            server_url,
            match host_type {
                HostType::Wasm => "--bin-path",
                HostType::Js => "--js-path",
            },
            wasm_file,
            &name,
        ],
    );
    name
}

/// Run `spacetime generate` to generate client bindings into the `client_project`.
///
/// `language` should be a string suitable for the `--lang` argument to `spacetime generate`.
/// `"unrealcpp"` is special-cased to account for the CLI taking different arguments.
/// Tests of the Unreal client SDK must use exactly that string, not any alias accepted by the CLI.
///
/// `wasm_file` is a path to a compiled WASM blob, as returned by [`compile_module`].
///
/// `client_project` and `generate_subdir` will be the values set in the [`Test`].
/// These have different semantics depending on whether `language` is `"unrealcpp"`.
///
/// For Unreal SDK tests, the `client_project` should be the directory which contains the `.uproject` file,
/// and `generate_subdir` should be the Unreal module name.
///
/// For non-unreal SDK tests, the `client_project` may be an arbitrary path,
/// and the `generate_subdir` an arbitrary relative path within it.
/// These will be combined as `"{client_project}/{generate_subdir}"` to produce the `--out-dir`.
///
/// This function memoizes the complete generation input, not just the output directory.
///
/// Without this lock, if multiple `Test`s ran concurrently in the same process
/// with the same `client_project` and `generate_subdir`,
/// the test harness would run `spacetime generate` multiple times concurrently,
/// each of which would remove and re-populate the bindings directory,
/// potentially sweeping them out from under a compile or run process.
///
/// This lock ensures that only one `spacetime generate` process runs at a time.
///
/// Circumstances where this will still break:
/// - If multiple distinct test harness processes run concurrently,
///   they will encounter the race condition described above,
///   because the binding-generation lock is not shared between harness processes.
///   Prefer constructing multiple `Test`s and `Test::run`ing them
///   from within the same harness process.
//
// I (pgoldman 2023-09-11) considered, as an alternative to this lock,
// having `Test::run` copy the `client_project` into a fresh temporary directory.
// That would be more complicated, as we'd need to re-write dependencies
// on the client language's SpacetimeDB SDK to use a local absolute path.
// Doing so portably across all our SDK languages seemed infeasible.
fn generate_bindings(
    paths: &SpacetimePaths,
    language: &str,
    wasm_file: &str,
    host_type: HostType,
    client_project: &str,
    generate_subdir: &str,
    generate_include_private: bool,
) {
    // We need these to be owned values so we can memoize on them.
    let client_project = client_project.to_owned();
    let generate_subdir = generate_subdir.to_owned();
    let language = language.to_owned();
    let wasm_file = wasm_file.to_owned();
    let host_is_js = matches!(host_type, HostType::Js);

    // Codegen is side-effecting and doesn't meaningfully return a Rust value,
    // so our memoization has unit as the value.
    // This makes it run at most once for each key.
    memoized!(
        |(client_project, generate_subdir, language, wasm_file, host_is_js, generate_include_private): (
            String,
            String,
            String,
            String,
            bool,
            bool,
        )|
         -> () {
            let mut args: Vec<&str> = vec![
                "generate",
                "--yes",
                "--lang",
                &language,
                if *host_is_js { "--js-path" } else { "--bin-path" },
                &wasm_file,
            ];

            if *generate_include_private {
                args.push("--include-private");
            }

            let generate_dir: String;

            // `generate --lang unrealcpp` takes different arguments from non-Unreal languages
            // to account for some quirks of Unreal project structure.
            if language_is_unreal(language) {
                // For unreal, we use `client_project` as the uproject directory,
                // and `generate_subdir` as the module name.
                args.extend_from_slice(&["--uproject-dir", client_project]);
                args.extend_from_slice(&["--module-name", generate_subdir]);
            } else {
                generate_dir = format!("{client_project}/{generate_subdir}");
                create_dir_all(&generate_dir).unwrap();
                args.extend_from_slice(&["--out-dir", &generate_dir]);
            }

            invoke_cli(paths, &args);
        }
    )
}

fn split_command_string(command: &str) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    for c in command.chars() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            '"' | '\'' if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    let mut iter = parts.into_iter();
    let exe = iter.next().expect("Command should have at least a program name");
    (exe, iter.collect())
}

// Note: this function is memoized to ensure we only compile each client once.
fn compile_client(compile_command: &str, client_project: &str, module_name: &str) {
    let client_project = client_project.to_owned();
    let module_name = module_name.to_owned();

    memoized!(|(client_project, module_name): (String, String)| -> () {
        let _ = module_name;
        let (exe, args) = split_command_string(compile_command);

        let output = cmd(exe, args)
            .dir(client_project)
            .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .expect("Error running compile command");

        status_ok_or_panic(output, compile_command, "(compiling)");
    })
}

fn run_client(run_command: &str, client_project: &str, server_url: &str, db_name: &str) {
    let (exe, args) = split_command_string(run_command);

    run_client_command(
        exe.into(),
        args.into_iter().map(Into::into).collect(),
        client_project,
        server_url,
        db_name,
        run_command,
    );
}

fn run_client_command(
    exe: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
    client_project: &str,
    server_url: &str,
    db_name: &str,
    command_description: &str,
) {
    let output = cmd(exe, args)
        .dir(client_project)
        .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
        .env(TEST_SERVER_URL_ENV_VAR, server_url)
        .env(TEST_DB_NAME_ENV_VAR, db_name)
        .env(
            "RUST_LOG",
            "spacetimedb=debug,spacetimedb_client_api=debug,spacetimedb_lib=debug,spacetimedb_standalone=debug",
        )
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .expect("Error running run command");

    status_ok_or_panic(output, command_description, "(running)");
}

#[derive(Clone, Default)]
pub struct TestBuilder {
    name: Option<String>,
    module_name: Option<String>,
    client_project: Option<String>,
    generate_language: Option<String>,
    generate_include_private: bool,
    generate_subdir: Option<String>,
    compile_command: Option<String>,
    run_command: Option<String>,
    prepared_client: Option<PreparedClient>,
    prepared_client_key: Option<String>,
}

impl TestBuilder {
    pub fn with_name(self, name: impl Into<String>) -> Self {
        TestBuilder {
            name: Some(name.into()),
            ..self
        }
    }

    pub fn with_module(self, module_name: impl Into<String>) -> Self {
        TestBuilder {
            module_name: Some(module_name.into()),
            ..self
        }
    }

    pub fn with_client(self, client_project: impl Into<String>) -> Self {
        let client_project = client_project.into();
        TestBuilder {
            client_project: Some(workspace_path(client_project).to_string_lossy().into_owned()),
            ..self
        }
    }

    pub fn with_language(self, generate_language: impl Into<String>) -> Self {
        TestBuilder {
            generate_language: Some(generate_language.into()),
            ..self
        }
    }

    pub fn with_bindings_dir(self, generate_subdir: impl Into<String>) -> Self {
        TestBuilder {
            generate_subdir: Some(generate_subdir.into()),
            ..self
        }
    }

    // Unreal-only: names the Unreal module into which bindings are generated.
    pub fn with_unreal_module(self, unreal_module_name: impl Into<String>) -> Self {
        TestBuilder {
            generate_subdir: Some(unreal_module_name.into()),
            ..self
        }
    }

    pub fn with_compile_command(self, compile_command: impl Into<String>) -> Self {
        TestBuilder {
            compile_command: Some(compile_command.into()),
            ..self
        }
    }

    pub fn with_run_command(self, run_command: impl Into<String>) -> Self {
        TestBuilder {
            run_command: Some(run_command.into()),
            ..self
        }
    }

    pub fn with_prepared_native_client(
        self,
        binary_name: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        TestBuilder {
            prepared_client: Some(PreparedClient::Native {
                binary_name: binary_name.into(),
                args: args.into_iter().map(Into::into).collect(),
            }),
            ..self
        }
    }

    pub fn with_prepared_client_key(self, key: impl Into<String>) -> Self {
        TestBuilder {
            prepared_client_key: Some(key.into()),
            ..self
        }
    }

    pub fn with_prepared_browser_client(self, artifact_name: impl Into<String>, selector: impl Into<String>) -> Self {
        TestBuilder {
            prepared_client: Some(PreparedClient::Browser {
                artifact_name: artifact_name.into(),
                selector: selector.into(),
            }),
            ..self
        }
    }

    pub fn with_prepared_node_client(
        self,
        entrypoint: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        TestBuilder {
            prepared_client: Some(PreparedClient::Node {
                entrypoint: entrypoint.into(),
                args: args.into_iter().map(Into::into).collect(),
            }),
            ..self
        }
    }

    pub fn with_generate_private_items(self, include_private: bool) -> Self {
        TestBuilder {
            generate_include_private: include_private,
            ..self
        }
    }

    pub fn build(self) -> Test {
        let generate_language = self
            .generate_language
            .expect("Supply a client language using TestBuilder::with_language");

        // For non-Unreal: require generate_subdir as before.
        // For Unreal: ignore generate_subdir entirely, but still populate with a harmless placeholder.
        let msg = if language_is_unreal(&generate_language) {
            "Supply an Unreal module name using TestBuilder::with_unreal_module"
        } else {
            "Supply a module_bindings subdirectory using TestBuilder::with_bindings_dir"
        };
        let generate_subdir = self.generate_subdir.expect(msg);

        Test {
            name: self.name.expect("Supply a test name using TestBuilder::with_name"),
            module_name: self
                .module_name
                .expect("Supply a module name using TestBuilder::with_module"),
            client_project: self
                .client_project
                .expect("Supply a client project directory using TestBuilder::with_client"),
            generate_language,
            generate_include_private: self.generate_include_private,
            generate_subdir,
            compile_command: self
                .compile_command
                .expect("Supply a compile command using TestBuilder::with_compile_command"),
            run_command: self
                .run_command
                .expect("Supply a run command using TestBuilder::with_run_command"),
            prepared_client: self.prepared_client,
            prepared_client_key: self.prepared_client_key,
        }
    }
}
