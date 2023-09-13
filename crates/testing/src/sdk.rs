use duct::{cmd, Handle};
use lazy_static::lazy_static;
use rand::distributions::{Alphanumeric, DistString};
use std::{collections::HashSet, fs::create_dir_all, sync::Mutex};

use crate::modules::{module_path, CompiledModule};
use std::path::Path;

struct StandaloneProcess {
    handle: Handle,
    num_using: usize,
}

impl StandaloneProcess {
    fn start() -> Self {
        let handle = cmd!("spacetime", "start")
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .start()
            .expect("Failed to run `spacetime start`");

        StandaloneProcess { handle, num_using: 1 }
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        assert!(self.num_using == 0);

        self.handle.kill()?;

        Ok(())
    }

    fn running_or_err(&self) -> anyhow::Result<()> {
        if let Some(output) = self
            .handle
            .try_wait()
            .expect("Error from spacetime standalone subprocess")
        {
            let code = output.status;
            let output = String::from_utf8_lossy(&output.stdout);
            Err(anyhow::anyhow!(
                "spacetime start exited unexpectedly. Exit status: {}. Output:\n{}",
                code,
                output,
            ))
        } else {
            Ok(())
        }
    }

    fn add_user(&mut self) -> anyhow::Result<()> {
        self.running_or_err()?;
        self.num_using += 1;
        Ok(())
    }

    /// Returns true if the process was stopped because no one is using it.
    fn sub_user(&mut self) -> anyhow::Result<bool> {
        self.num_using -= 1;
        if self.num_using == 0 {
            self.stop()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

static STANDALONE_PROCESS: Mutex<Option<StandaloneProcess>> = Mutex::new(None);

/// An RAII handle on the `STANDALONE_PROCESS`.
///
/// On construction, ensures that the `STANDALONE_PROCESS` is running.
///
/// On drop, checks to see if it was the last `StandaloneHandle`, and if so,
/// terminates the `STANDALONE_PROCESS`.
pub struct StandaloneHandle {
    _hidden: (),
}

impl Default for StandaloneHandle {
    fn default() -> Self {
        let mut process = STANDALONE_PROCESS.lock().expect("STANDALONE_PROCESS Mutex is poisoned");
        if let Some(proc) = &mut *process {
            proc.add_user()
                .expect("Failed to add user for running spacetime standalone process");
        } else {
            *process = Some(StandaloneProcess::start());
        }
        StandaloneHandle { _hidden: () }
    }
}

impl Drop for StandaloneHandle {
    fn drop(&mut self) {
        let mut process = STANDALONE_PROCESS.lock().expect("STANDALONE_PROCESS Mutex is poisoned");
        if let Some(proc) = &mut *process {
            if proc
                .sub_user()
                .expect("Failed to remove user for running spacetime standalone process")
            {
                *process = None;
            }
        }
    }
}

lazy_static! {
    /// An exclusive lock which ensures we only run `spacetime generate` once for each target directory.
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
    static ref BINDINGS_GENERATED: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

pub struct Test {
    /// A human-readable name for this test.
    name: String,

    /// Must name a module in the SpacetimeDB/modules directory.
    module_name: String,

    /// An arbitrary path to the client project.
    client_project: String,

    /// A language suitable for the `spacetime generate` CLI command.
    generate_language: String,

    /// A relative path within the `client_project` to place the module bindings.
    ///
    /// Usually `src/module_bindings`
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
    /// - `SPACETIME_SDK_TEST_DB_ADDR` bound to the database address.
    run_command: String,
}

pub const TEST_MODULE_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_MODULE_PROJECT";
pub const TEST_DB_NAME_ENV_VAR: &str = "SPACETIME_SDK_TEST_DB_NAME";
pub const TEST_CLIENT_PROJECT_ENV_VAR: &str = "SPACETIME_SDK_TEST_CLIENT_PROJECT";

impl Test {
    pub fn builder() -> TestBuilder {
        TestBuilder::default()
    }
    pub fn run(&self) {
        let _handle = StandaloneHandle::default();

        let compiled = CompiledModule::compile(&self.module_name);

        generate_bindings(
            &self.generate_language,
            compiled.path(),
            &self.client_project,
            &self.generate_subdir,
            &self.name,
        );

        compile_client(&self.compile_command, &self.client_project, &self.name);

        let db_name = publish_module(&self.module_name, &self.name);

        run_client(&self.run_command, &self.client_project, &db_name, &self.name);
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
    Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
}

fn publish_module(module: &str, test_name: &str) -> String {
    let name = random_module_name();
    let output = cmd!("spacetime", "publish", "--skip_clippy", name.clone(),)
        .stderr_to_stdout()
        .stdout_capture()
        .dir(module_path(module))
        .unchecked()
        .run()
        .expect("Error running spacetime publish");

    status_ok_or_panic(output, "spacetime publish", test_name);

    name
}

fn generate_bindings(language: &str, path: &Path, client_project: &str, generate_subdir: &str, test_name: &str) {
    let generate_dir = format!("{}/{}", client_project, generate_subdir);

    let mut bindings_lock = BINDINGS_GENERATED.lock().expect("BINDINGS_GENERATED Mutex is poisoned");

    // If we've already generated bindings in this directory,
    // return early.
    // Otherwise, we'll hold the lock for the duration of the subprocess,
    // so other tests will wait before overwriting our output.
    if !bindings_lock.insert(generate_dir.clone()) {
        return;
    }

    create_dir_all(&generate_dir).expect("Error creating generate subdir");
    let output = cmd!(
        "spacetime",
        "generate",
        "--skip_clippy",
        "--lang",
        language,
        "--wasm-file",
        path,
        "--out-dir",
        generate_dir
    )
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run()
    .expect("Error running spacetime generate");

    status_ok_or_panic(output, "spacetime generate", test_name);
}

fn split_command_string(command: &str) -> (&str, Vec<&str>) {
    let mut iter = command.split(' ');
    let exe = iter.next().expect("Command should have at least a program name");
    let args = iter.collect();
    (exe, args)
}

fn compile_client(compile_command: &str, client_project: &str, test_name: &str) {
    let (exe, args) = split_command_string(compile_command);

    let output = cmd(exe, args)
        .dir(client_project)
        .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .expect("Error running compile command");

    status_ok_or_panic(output, compile_command, test_name);
}

fn run_client(run_command: &str, client_project: &str, db_name: &str, test_name: &str) {
    let (exe, args) = split_command_string(run_command);

    let output = cmd(exe, args)
        .dir(client_project)
        .env(TEST_CLIENT_PROJECT_ENV_VAR, client_project)
        .env(TEST_DB_NAME_ENV_VAR, db_name)
        .env("RUST_LOG", "info")
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .expect("Error running run command");

    status_ok_or_panic(output, run_command, test_name);
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
        Test {
            name: self.name.expect("Supply a test name using TestBuilder::with_name"),
            module_name: self
                .module_name
                .expect("Supply a module name using TestBuilder::with_module"),
            client_project: self
                .client_project
                .expect("Supply a client project directory using TestBuilder::with_client"),
            generate_language: self
                .generate_language
                .expect("Supply a client language using TestBuilder::with_language"),
            generate_subdir: self
                .generate_subdir
                .expect("Supply a module_bindings subdirectory using TestBuilder::with_bindings_dir"),
            compile_command: self
                .compile_command
                .expect("Supply a compile command using TestBuilder::with_compile_command"),
            run_command: self
                .run_command
                .expect("Supply a run command using TestBuilder::with_run_command"),
        }
    }
}
