use duct::cmd;
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use spacetimedb::messages::control_db::HostType;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_guard::SpacetimeDbGuard;
use spacetimedb_paths::{RootDir, SpacetimePaths};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::invoke_cli;
use crate::modules::{CompilationMode, CompiledModule};
use tempfile::TempDir;

pub const SDK_TEST_MODE_ENV: &str = "SPACETIME_SDK_TEST_MODE";
pub const SDK_TEST_ARTIFACT_DIR_ENV: &str = "SPACETIME_SDK_TEST_ARTIFACT_DIR";

#[derive(Clone)]
pub enum PrebuiltClient {
    NativeRust {
        binary_name: String,
        args: Vec<String>,
    },
    BrowserRust {
        package_name: String,
        run_selector: String,
    },
    TypeScript {
        package_name: String,
        entrypoint: PathBuf,
        args: Vec<String>,
    },
}

impl PrebuiltClient {
    fn key(&self) -> String {
        match self {
            Self::NativeRust { binary_name, .. } => format!("native-{binary_name}"),
            Self::BrowserRust { package_name, .. } => format!("browser-{package_name}"),
            Self::TypeScript { package_name, .. } => format!("typescript-{package_name}"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SdkTestMode {
    Local,
    Prepare,
    Prebuilt,
}

impl SdkTestMode {
    fn from_env() -> Self {
        match env::var(SDK_TEST_MODE_ENV).as_deref() {
            Err(env::VarError::NotPresent) => Self::Local,
            Ok("prepare") => Self::Prepare,
            Ok("prebuilt") => Self::Prebuilt,
            Ok(value) => panic!("invalid {SDK_TEST_MODE_ENV} value {value:?}; expected `prepare` or `prebuilt`"),
            Err(err) => panic!("failed to read {SDK_TEST_MODE_ENV}: {err}"),
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
struct ArtifactManifest {
    modules: BTreeMap<String, ModuleArtifact>,
    clients: BTreeMap<String, ClientArtifact>,
}

#[derive(Clone, Deserialize, Serialize)]
struct ModuleArtifact {
    path: PathBuf,
    host_type: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct ClientArtifact {
    path: PathBuf,
}

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

    /// Describes the client output that CI prepares once and shares with nextest shards.
    /// Local runs continue to use `compile_command` and `run_command`.
    prebuilt_client: Option<PrebuiltClient>,
}

pub const TEST_MODULE_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_MODULE_PROJECT";
pub const TEST_DB_NAME_ENV_VAR: &str = "SPACETIME_SDK_TEST_DB_NAME";
pub const TEST_SERVER_URL_ENV_VAR: &str = "SPACETIME_SDK_TEST_SERVER_URL";
pub const TEST_CLIENT_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_CLIENT_PROJECT";

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

        match SdkTestMode::from_env() {
            SdkTestMode::Local => self.run_local(paths),
            SdkTestMode::Prepare => self.prepare(paths),
            SdkTestMode::Prebuilt => self.run_prebuilt(paths),
        }
    }

    fn run_local(&self, paths: &SpacetimePaths) {
        let (file, host_type) = compile_module(&self.module_name);

        generate_bindings(
            paths,
            &self.generate_language,
            &file,
            host_type,
            &self.client_project,
            &self.generate_subdir,
            self.generate_include_private,
        );

        compile_client(&self.compile_command, &self.client_project);
        self.run_with_artifacts(paths, &file, host_type, &self.run_command);
    }

    fn prepare(&self, paths: &SpacetimePaths) {
        let artifact_dir = sdk_artifact_dir();
        let (file, host_type) = prepare_module_artifact(&artifact_dir, &self.module_name);

        generate_bindings(
            paths,
            &self.generate_language,
            &file,
            host_type,
            &self.client_project,
            &self.generate_subdir,
            self.generate_include_private,
        );

        let prebuilt_client = self.prebuilt_client.as_ref().unwrap_or_else(|| {
            panic!(
                "SDK test client {} has no prebuilt artifact description",
                self.client_project
            )
        });
        prepare_client_artifact(
            &artifact_dir,
            prebuilt_client,
            &self.compile_command,
            &self.client_project,
        );
    }

    fn run_prebuilt(&self, paths: &SpacetimePaths) {
        let artifact_dir = sdk_artifact_dir();
        let manifest = read_manifest(&artifact_dir);
        let module = manifest
            .modules
            .get(&self.module_name)
            .unwrap_or_else(|| panic!("SDK support manifest has no module entry for {}", self.module_name));
        let file = resolve_artifact(&artifact_dir, &module.path, "module", &self.module_name);
        let host_type = parse_host_type(&module.host_type);

        let prebuilt_client = self.prebuilt_client.as_ref().unwrap_or_else(|| {
            panic!(
                "SDK test client {} has no prebuilt artifact description",
                self.client_project
            )
        });
        let client_key = prebuilt_client.key();
        let client = manifest
            .clients
            .get(&client_key)
            .unwrap_or_else(|| panic!("SDK support manifest has no client entry for {client_key}"));
        let client_path = resolve_artifact(&artifact_dir, &client.path, "client", &client_key);

        let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
        let server_url = guard.host_url.as_str();
        let db_name = publish_module(
            paths,
            server_url,
            file.to_str().expect("module artifact path is not UTF-8"),
            host_type,
        );
        run_prebuilt_client(
            prebuilt_client,
            &client_path,
            &self.client_project,
            server_url,
            &db_name,
        );
    }

    fn run_with_artifacts(&self, paths: &SpacetimePaths, file: &str, host_type: HostType, run_command: &str) {
        let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
        let server_url = guard.host_url.as_str();
        let db_name = publish_module(paths, server_url, file, host_type);

        run_client(run_command, &self.client_project, server_url, &db_name);
    }
}

fn sdk_artifact_dir() -> PathBuf {
    env::var_os(SDK_TEST_ARTIFACT_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{SDK_TEST_ARTIFACT_DIR_ENV} must be set in SDK test prepare/prebuilt mode"))
}

fn client_target_dir(client_project: &str) -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => Path::new(client_project).join(path),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
    }
}

fn manifest_path(artifact_dir: &Path) -> PathBuf {
    artifact_dir.join("manifest.json")
}

fn read_manifest(artifact_dir: &Path) -> ArtifactManifest {
    let path = manifest_path(artifact_dir);
    let bytes =
        fs::read(&path).unwrap_or_else(|err| panic!("failed to read SDK support manifest {}: {err}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("failed to parse SDK support manifest {}: {err}", path.display()))
}

fn read_manifest_or_default(artifact_dir: &Path) -> ArtifactManifest {
    let path = manifest_path(artifact_dir);
    if path.exists() {
        read_manifest(artifact_dir)
    } else {
        ArtifactManifest::default()
    }
}

fn write_manifest(artifact_dir: &Path, manifest: &ArtifactManifest) {
    fs::create_dir_all(artifact_dir).expect("failed to create SDK support directory");
    let path = manifest_path(artifact_dir);
    let bytes = serde_json::to_vec_pretty(manifest).expect("failed to serialize SDK support manifest");
    fs::write(&path, bytes).expect("failed to write SDK support manifest");
}

fn host_type_name(host_type: HostType) -> &'static str {
    match host_type {
        HostType::Wasm => "wasm",
        HostType::Js => "js",
    }
}

fn parse_host_type(host_type: &str) -> HostType {
    match host_type {
        "wasm" => HostType::Wasm,
        "js" => HostType::Js,
        value => panic!("invalid SDK support module host type {value:?}"),
    }
}

fn resolve_artifact(artifact_dir: &Path, relative_path: &Path, kind: &str, key: &str) -> PathBuf {
    assert!(
        relative_path.is_relative(),
        "SDK support manifest {kind} path for {key} must be relative"
    );
    let path = artifact_dir.join(relative_path);
    assert!(
        path.exists(),
        "SDK support {kind} artifact for {key} is missing at {}",
        path.display()
    );
    path
}

fn safe_artifact_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn prepare_module_artifact(artifact_dir: &Path, module_name: &str) -> (String, HostType) {
    let mut manifest = read_manifest_or_default(artifact_dir);
    if let Some(module) = manifest.modules.get(module_name) {
        let path = resolve_artifact(artifact_dir, &module.path, "module", module_name);
        return (
            path.to_str().expect("module artifact path is not UTF-8").to_owned(),
            parse_host_type(&module.host_type),
        );
    }

    let (source, host_type) = compile_module(module_name);
    let extension = Path::new(&source)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_else(|| host_type_name(host_type));
    let relative_path = PathBuf::from("modules").join(format!("{}.{}", safe_artifact_name(module_name), extension));
    let destination = artifact_dir.join(&relative_path);
    fs::create_dir_all(destination.parent().unwrap()).expect("failed to create SDK module artifact directory");
    fs::copy(&source, &destination).unwrap_or_else(|err| {
        panic!(
            "failed to copy SDK module artifact from {} to {}: {err}",
            source,
            destination.display()
        )
    });
    manifest.modules.insert(
        module_name.to_owned(),
        ModuleArtifact {
            path: relative_path,
            host_type: host_type_name(host_type).to_owned(),
        },
    );
    write_manifest(artifact_dir, &manifest);
    (
        destination
            .to_str()
            .expect("module artifact path is not UTF-8")
            .to_owned(),
        host_type,
    )
}

fn copy_dir_recursive(source: &Path, destination: &Path) {
    if destination.exists() {
        fs::remove_dir_all(destination).expect("failed to replace SDK client artifact directory");
    }
    fs::create_dir_all(destination).expect("failed to create SDK client artifact directory");
    for entry in fs::read_dir(source).unwrap_or_else(|err| panic!("failed to read {}: {err}", source.display())) {
        let entry = entry.expect("failed to read SDK client artifact entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .expect("failed to inspect SDK client artifact")
            .is_dir()
        {
            copy_dir_recursive(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|err| {
                panic!(
                    "failed to copy SDK client artifact from {} to {}: {err}",
                    source_path.display(),
                    destination_path.display()
                )
            });
        }
    }
}

fn prepare_client_artifact(artifact_dir: &Path, client: &PrebuiltClient, compile_command: &str, client_project: &str) {
    let key = client.key();
    let mut manifest = read_manifest_or_default(artifact_dir);
    if let Some(client) = manifest.clients.get(&key) {
        resolve_artifact(artifact_dir, &client.path, "client", &key);
        return;
    }

    compile_client(compile_command, client_project);

    let relative_path = PathBuf::from("clients").join(&key);
    let destination = artifact_dir.join(&relative_path);
    match client {
        PrebuiltClient::NativeRust { binary_name, .. } => {
            fs::create_dir_all(destination.parent().unwrap()).expect("failed to create SDK client artifact directory");
            let executable_name = if cfg!(windows) {
                format!("{binary_name}.exe")
            } else {
                binary_name.clone()
            };
            let source = client_target_dir(client_project).join("debug").join(executable_name);
            fs::copy(&source, &destination).unwrap_or_else(|err| {
                panic!(
                    "failed to copy native SDK client from {} to {}: {err}",
                    source.display(),
                    destination.display()
                )
            });
        }
        PrebuiltClient::BrowserRust { package_name, .. } => {
            let source = Path::new(client_project)
                .join("target/sdk-test-web-bindgen")
                .join(package_name);
            copy_dir_recursive(&source, &destination);
        }
        PrebuiltClient::TypeScript { entrypoint, .. } => {
            let source = Path::new(client_project).join(entrypoint.parent().unwrap_or_else(|| {
                panic!(
                    "TypeScript SDK client entrypoint {} has no parent",
                    entrypoint.display()
                )
            }));
            copy_dir_recursive(&source, &destination);
        }
    }

    manifest.clients.insert(key, ClientArtifact { path: relative_path });
    write_manifest(artifact_dir, &manifest);
}

fn status_ok_or_panic(output: std::process::Output, command: &str, test_name: &str) {
    if !output.status.success() {
        panic!(
            "{}: Error running {:?}: exited with non-zero exit status {}.\nStdout:\n{}\nStderr:\n{}",
            test_name,
            command,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
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
    let module = module.to_owned();

    memoized!(|module: String| -> (String, HostType) {
        let module = CompiledModule::compile(module, CompilationMode::Debug);
        (module.path().to_str().unwrap().to_owned(), module.host_type)
    })
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
/// Note: this function is memoized to ensure we only run `spacetime generate` once for each target directory.
///
/// Without this lock, if multiple `Test`s ran concurrently in the same process
/// with the same `client_project` and `generate_subdir`,
/// the test harness would run `spacetime generate` multiple times concurrently,
/// each of which would remove and re-populate the bindings directory,
/// potentially sweeping them out from under a compile or run process.
///
/// This lock ensures that only one `spacetime generate` process runs at a time,
/// and the `HashSet` ensures that we run `spacetime generate` only once for each output directory.
///
/// Circumstances where this will still break:
/// - If multiple tests want to use the same client_project/generate_subdir pair,
///   but for different modules' bindings, only one module's bindings will ever be generated.
///   If you need bindings for multiple different modules, put them in different subdirs.
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
    // We need these to be owned `String`s so we can memoize on them.
    let client_project = client_project.to_owned();
    let generate_subdir = generate_subdir.to_owned();

    // Codegen is side-effecting and doesn't meaningfully return a Rust value,
    // so our memoization has unit as the value.
    // This makes it run at most once for each key.
    memoized!(|(client_project, generate_subdir): (String, String)| -> () {
        let mut args: Vec<&str> = vec![
            "generate",
            "--yes",
            "--lang",
            language,
            match host_type {
                HostType::Wasm => "--bin-path",
                HostType::Js => "--js-path",
            },
            wasm_file,
        ];

        if generate_include_private {
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
    })
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
fn compile_client(compile_command: &str, client_project: &str) {
    let client_project = client_project.to_owned();

    memoized!(|client_project: String| -> () {
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

    status_ok_or_panic(output, run_command, "(running)");
}

fn run_prebuilt_client(
    client: &PrebuiltClient,
    artifact_path: &Path,
    client_project: &str,
    server_url: &str,
    db_name: &str,
) {
    let (program, args): (PathBuf, Vec<String>) = match client {
        PrebuiltClient::NativeRust { args, .. } => (artifact_path.to_owned(), args.clone()),
        PrebuiltClient::BrowserRust {
            package_name,
            run_selector,
        } => {
            let artifact_name = package_name.replace('-', "_");
            let js_module = artifact_path.join(format!("{artifact_name}.cjs"));
            assert!(
                js_module.exists(),
                "browser SDK client is missing {}",
                js_module.display()
            );
            let js_module = serde_json::to_string(&js_module).expect("failed to quote browser SDK client path");
            let run_selector = serde_json::to_string(run_selector).expect("failed to quote browser SDK selector");
            let script = format!(
                "(async () => {{ \
                 const m = require({js_module}); \
                 if (m.default) {{ await m.default(); }} \
                 const run = m.run || m.main || m.start; \
                 if (!run) throw new Error('No exported run/main/start function from wasm module'); \
                 await run({run_selector}, process.env.{TEST_DB_NAME_ENV_VAR}, process.env.{TEST_SERVER_URL_ENV_VAR}); \
                 process.exit(0); \
                 }})().catch((e) => {{ console.error(e); process.exit(1); }});"
            );
            (
                PathBuf::from("node"),
                vec!["--experimental-websocket".into(), "-e".into(), script],
            )
        }
        PrebuiltClient::TypeScript { entrypoint, args, .. } => {
            let entrypoint = artifact_path.join(entrypoint.file_name().unwrap_or_else(|| {
                panic!(
                    "TypeScript SDK client entrypoint {} has no filename",
                    entrypoint.display()
                )
            }));
            assert!(
                entrypoint.exists(),
                "TypeScript SDK client is missing {}",
                entrypoint.display()
            );
            let mut command_args = vec![entrypoint.to_string_lossy().into_owned()];
            command_args.extend(args.iter().cloned());
            (PathBuf::from("node"), command_args)
        }
    };

    let command = format!("{} {}", program.display(), args.join(" "));
    let output = std::process::Command::new(&program)
        .args(&args)
        .current_dir(client_project)
        .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
        .env(TEST_SERVER_URL_ENV_VAR, server_url)
        .env(TEST_DB_NAME_ENV_VAR, db_name)
        .env(
            "RUST_LOG",
            "spacetimedb=debug,spacetimedb_client_api=debug,spacetimedb_lib=debug,spacetimedb_standalone=debug",
        )
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .expect("Error running prebuilt SDK client");

    status_ok_or_panic(output, &command, "(running prebuilt client)");
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
    prebuilt_client: Option<PrebuiltClient>,
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
        TestBuilder {
            client_project: Some(client_project.into()),
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

    pub fn with_prebuilt_client(self, prebuilt_client: PrebuiltClient) -> Self {
        TestBuilder {
            prebuilt_client: Some(prebuilt_client),
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
            prebuilt_client: self.prebuilt_client,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_manifest_uses_relative_paths() {
        let artifact_dir = tempfile::tempdir().unwrap();
        let module_path = PathBuf::from("modules/sdk-test.wasm");
        fs::create_dir_all(artifact_dir.path().join("modules")).unwrap();
        fs::write(artifact_dir.path().join(&module_path), []).unwrap();

        let mut manifest = ArtifactManifest::default();
        manifest.modules.insert(
            "sdk-test".into(),
            ModuleArtifact {
                path: module_path.clone(),
                host_type: "wasm".into(),
            },
        );
        write_manifest(artifact_dir.path(), &manifest);

        let manifest = read_manifest(artifact_dir.path());
        let module = manifest.modules.get("sdk-test").unwrap();
        assert_eq!(module.path, module_path);
        assert_eq!(
            resolve_artifact(artifact_dir.path(), &module.path, "module", "sdk-test"),
            artifact_dir.path().join("modules/sdk-test.wasm"),
        );
    }

    #[test]
    #[should_panic(expected = "is missing")]
    fn missing_support_artifact_fails_instead_of_falling_back() {
        let artifact_dir = tempfile::tempdir().unwrap();
        resolve_artifact(artifact_dir.path(), Path::new("clients/missing"), "client", "missing");
    }
}
