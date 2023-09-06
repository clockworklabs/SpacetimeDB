use duct::{cmd, Handle};
use rand::distributions::{Alphanumeric, DistString};
use std::{fs::create_dir_all, sync::Mutex};

use crate::modules::{compile, module_path, wasm_path};

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

        compile(&self.module_name);

        generate_bindings(
            &self.generate_language,
            &self.module_name,
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

fn generate_bindings(language: &str, module_name: &str, client_project: &str, generate_subdir: &str, test_name: &str) {
    let generate_dir = format!("{}/{}", client_project, generate_subdir);
    create_dir_all(&generate_dir).expect("Error creating generate subdir");
    let output = cmd!(
        "spacetime",
        "generate",
        "--skip_clippy",
        "--lang",
        language,
        "--wasm-file",
        wasm_path(module_name),
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
