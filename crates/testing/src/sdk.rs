use duct::cmd;
use rand::seq::IteratorRandom;
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
    generate_language: String,

    /// A relative path within the `client_project` to place the module bindings.
    ///
    /// Usually `src/module_bindings`
    generate_subdir: String,

    /// Unreal-specific: the target Unreal module name for codegen (e.g., "TestClient").
    /// Required when `generate_language == "unrealcpp"`. Ignored otherwise.
    unreal_module_name: Option<String>,

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

impl Test {
    pub fn builder() -> TestBuilder {
        TestBuilder::default()
    }
    pub fn run(self) {
        let paths = ensure_standalone_process();

        let wasm_file = compile_module(&self.module_name);

        // Determine if this is the Unreal SDK
        let is_unreal = self.generate_language.eq_ignore_ascii_case("unrealcpp");

        // For Unreal: require unreal_module_name and treat client_project as --uproject-dir
        let unreal_module_name_ref = if is_unreal {
            Some(
                self.unreal_module_name
                    .as_deref()
                    .expect("unrealcpp requires `unreal_module_name` to be set on Test"),
            )
        } else {
            None
        };

        generate_bindings(
            &paths,
            &self.generate_language,
            &wasm_file,
            &self.client_project,
            &self.generate_subdir,
            unreal_module_name_ref,
        );

        compile_client(&self.compile_command, &self.client_project);

        let db_name = publish_module(paths, &wasm_file);

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

macro_rules! memoized {
    // Unit arm: no clone, silence unused key.
    (|$key:ident: $key_ty:ty| -> () $body:block) => {{
        static MEMOIZED: Mutex<Option<HashMap<$key_ty, ()>>> = Mutex::new(None);
        {
            let mut map = MEMOIZED.lock().unwrap(); // guard lives for the whole block
            map.get_or_insert_default().entry($key).or_insert_with_key(|__k| {
                let $key = __k;
                let _ = &$key;
                $body
            });
        }
    }};

    // Value arm: clone while guard is still alive.
    (|$key:ident: $key_ty:ty| -> $value_ty:ty $body:block) => {{
        static MEMOIZED: Mutex<Option<HashMap<$key_ty, $value_ty>>> = Mutex::new(None);
        let cloned = {
            let mut map = MEMOIZED.lock().unwrap(); // guard lives for the whole block
            let v = map.get_or_insert_default().entry($key).or_insert_with_key(|__k| {
                let $key = __k;
                let _ = &$key;
                $body
            });
            v.clone()
        };
        cloned
    }};
}

// Note: this function is memoized to ensure we compile each module only once.
// Without this lock, if multiple `Test`s ran concurrently in the same process,
// the test harness would compile each module multiple times concurrently,
// which is bad both for performance reasons as well as can lead to errors
// with toolchains like .NET which don't expect parallel invocations
// of their build tools on the same project folder.
fn compile_module(module: &str) -> String {
    let module = module.to_owned();

    memoized!(|module: String| -> String {
        let module = CompiledModule::compile(module, CompilationMode::Debug);
        module.path().to_str().unwrap().to_owned()
    })
}

// Note: this function does not memoize because we want each test to publish the same
// module as a separate clean database instance for isolation purposes.
fn publish_module(paths: &SpacetimePaths, wasm_file: &str) -> String {
    let name = random_module_name();
    invoke_cli(
        paths,
        &[
            "publish",
            "--anonymous",
            "--server",
            "local",
            "--bin-path",
            wasm_file,
            &name,
        ],
    );
    name
}

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
    client_project: &str,  // For non-Unreal: base out dir. For Unreal: .uproject root dir instead.
    generate_subdir: &str, // Ignored for Unreal.
    module_name: Option<&str>, // Required for Unreal: the Unreal module to generate into.
) {
    let is_unreal = language.eq_ignore_ascii_case("unrealcpp");
    let generate_dir = format!("{client_project}/{generate_subdir}");

    // Memoize on the *actual* output target to avoid redundant runs.
    let memo_key = if is_unreal {
        format!("unreal::{client_project}::{:?}", module_name)
    } else {
        format!("generic::{generate_dir}")
    };

    memoized!(|memo_key: String| -> () {
        if !is_unreal {
            create_dir_all(&generate_dir).unwrap();
        }

        let mut args: Vec<&str> = vec!["generate", "--lang", language];

        // Prefer --project-path/--bin-path behavior you already have; here we show --bin-path.
        // If you dynamically choose between them elsewhere, keep that logic and just insert the Unreal flags.
        args.extend_from_slice(&["--bin-path", wasm_file]);

        if is_unreal {
            let module = module_name.expect("unrealcpp requires --module-name");
            args.extend_from_slice(&["--module-name", module]);
            args.extend_from_slice(&["--uproject-dir", client_project]);
        } else {
            args.extend_from_slice(&["--out-dir", &generate_dir]);
        }

        invoke_cli(paths, &args);
    });
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
    generate_subdir: Option<String>,    // Ignored for unrealcpp
    unreal_module_name: Option<String>, // Required for unrealcpp
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
            unreal_module_name: Some(unreal_module_name.into()),
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
        let is_unreal = generate_language.eq_ignore_ascii_case("unrealcpp");

        // For non-Unreal: require generate_subdir as before.
        // For Unreal: ignore generate_subdir entirely, but still populate with a harmless placeholder.
        let generate_subdir = if is_unreal {
            String::from("_unreal_ignored_")
        } else {
            self.generate_subdir
                .expect("Supply a module_bindings subdirectory using TestBuilder::with_bindings_dir")
        };

        // For Unreal: require unreal_module_name.
        let unreal_module_name = if is_unreal {
            Some(
                self.unreal_module_name
                    .expect("Supply Unreal module using TestBuilder::with_unreal_module for unrealcpp"),
            )
        } else {
            None
        };

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
            unreal_module_name,
            compile_command: self
                .compile_command
                .expect("Supply a compile command using TestBuilder::with_compile_command"),
            run_command: self
                .run_command
                .expect("Supply a run command using TestBuilder::with_run_command"),
        }
    }
}
