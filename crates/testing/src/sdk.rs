use duct::cmd;
use rand::seq::IteratorRandom;
use spacetimedb::messages::control_db::HostType;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_paths::{RootDir, SpacetimePaths};
use std::fs::create_dir_all;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

use crate::invoke_cli;
use crate::modules::{start_runtime, CompilationMode, CompiledModule};
use tempfile::TempDir;

/// Ensure that the server thread we're testing against is still running, starting
/// it if it hasn't been started yet.
pub fn ensure_standalone_process() -> &'static SpacetimePaths {
    static PATHS: OnceLock<SpacetimePaths> = OnceLock::new();
    static JOIN_HANDLE: OnceLock<Mutex<Option<JoinHandle<anyhow::Result<()>>>>> = OnceLock::new();

    let paths = PATHS.get_or_init(|| {
        let dir = TempDir::with_prefix("stdb-sdk-test")
            .expect("Failed to create tempdir")
            // TODO: This leaks the tempdir.
            //       We need the tempdir to live for the duration of the process,
            //       and all the options for post-`main` cleanup seem sketchy.
            .keep();
        SpacetimePaths::from_root_dir(&RootDir(dir))
    });

    let join_handle = JOIN_HANDLE.get_or_init(|| {
        Mutex::new(Some(std::thread::spawn(move || {
            start_runtime().block_on(spacetimedb_standalone::start_server(
                &paths.data_dir,
                Some(&paths.cli_config_dir.0),
            ))
        })))
    });

    let mut join_handle = join_handle.lock().unwrap_or_else(|e| e.into_inner());

    if join_handle
        .as_ref()
        .expect("Standalone process already finished")
        .is_finished()
    {
        match join_handle.take().unwrap().join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => panic!("standalone process failed: {e:?}"),
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<String>() {
                    s
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s
                } else {
                    "dyn Any"
                };
                panic!("standalone process failed by panic: {msg}")
            }
        }
    }

    paths
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
    run_command: String,
}

pub const TEST_MODULE_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_MODULE_PROJECT";
pub const TEST_DB_NAME_ENV_VAR: &str = "SPACETIME_SDK_TEST_DB_NAME";
pub const TEST_CLIENT_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_CLIENT_PROJECT";

fn language_is_unreal(language: &str) -> bool {
    language.eq_ignore_ascii_case("unrealcpp")
}

impl Test {
    pub fn builder() -> TestBuilder {
        TestBuilder::default()
    }
    pub fn run(self) {
        let paths = ensure_standalone_process();

        let (file, host_type) = compile_module(&self.module_name);

        generate_bindings(
            paths,
            &self.generate_language,
            &file,
            host_type,
            &self.client_project,
            &self.generate_subdir,
        );

        compile_client(&self.compile_command, &self.client_project);

        let db_name = publish_module(paths, &file, host_type);

        run_client(&self.run_command, &self.client_project, &db_name);
    }
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
    let module = module.to_owned();

    memoized!(|module: String| -> (String, HostType) {
        let module = CompiledModule::compile(module, CompilationMode::Debug);
        (module.path().to_str().unwrap().to_owned(), module.host_type)
    })
}

// Note: this function does not memoize because we want each test to publish the same
// module as a separate clean database instance for isolation purposes.
fn publish_module(paths: &SpacetimePaths, wasm_file: &str, host_type: HostType) -> String {
    let name = random_module_name();
    invoke_cli(
        paths,
        &[
            "publish",
            "--anonymous",
            "--server",
            "local",
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
///   because the `BINDINGS_GENERATED` lock is not shared between harness processes.
///   Running multiple test harness processes concurrently will break anyways
///   because each will try to run `spacetime start` as a subprocess and will therefore
///   contend over port 3000.
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

fn run_client(run_command: &str, client_project: &str, db_name: &str) {
    let (exe, args) = split_command_string(run_command);

    let output = cmd(exe, args)
        .dir(client_project)
        .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
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

#[derive(Clone, Default)]
pub struct TestBuilder {
    name: Option<String>,
    module_name: Option<String>,
    client_project: Option<String>,
    generate_language: Option<String>,
    generate_subdir: Option<String>,
    compile_command: Option<String>,
    run_command: Option<String>,
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
            generate_subdir,
            compile_command: self
                .compile_command
                .expect("Supply a compile command using TestBuilder::with_compile_command"),
            run_command: self
                .run_command
                .expect("Supply a run command using TestBuilder::with_run_command"),
        }
    }
}
