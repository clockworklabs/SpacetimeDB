//! Publish-only environment configuration through the real CLI and local server.
use serde_json::{json, Value};
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{modules, random_string, Smoketest};
use std::{
    fs,
    io::{Read as _, Seek as _},
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

const KEYS: &[&str] = &[
    "SMOKE_REQUIRED",
    "SMOKE_MODE",
    "SMOKE_OPTIONAL",
    "SMOKE_EMPTY",
    "SMOKE_NUMBER",
    "SMOKE_FLAG",
];

struct Fixture {
    test: Smoketest,
    database: String,
    wasm: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        // Check before the harness can connect or copy a prior login. These are
        // standalone-only tests, including when accidentally run in a remote job.
        for key in [
            "SPACETIME_REMOTE_SERVER",
            "SPACETIME_USE_AUTH_HOST",
            "SPACETIME_SMOKETEST_BASE_CONFIG_PATH",
        ] {
            assert!(
                std::env::var_os(key).is_none(),
                "ENV smoke test requires isolated local settings ({key})"
            );
        }
        let test = Smoketest::builder()
            .precompiled_module("environment-publish")
            .autopublish(false)
            .build();
        assert!(test.guard.is_some());
        let address = test
            .server_url
            .strip_prefix("http://")
            .unwrap()
            .parse::<std::net::SocketAddr>()
            .unwrap();
        assert_eq!(address.ip(), std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        assert_ne!(address.port(), 0);
        let wasm = test.project_dir.path().join("published.wasm");
        fs::copy(modules::precompiled_module("environment-publish"), &wasm).unwrap();
        // A misleading local source proves --bin-path reads declarations from
        // these exact bytes, rather than trusting nearby source/config metadata.
        fs::create_dir(test.project_dir.path().join("src")).unwrap();
        fs::write(
            test.project_dir.path().join("src/lib.rs"),
            "#[spacetimedb::env] pub struct Env { pub WRONG_SOURCE_DECLARATION: String }",
        )
        .unwrap();
        let fixture = Self {
            test,
            database: format!("environment-{}", random_string()),
            wasm,
        };
        fixture.success(&["login", "--server-issued-login", &fixture.test.server_url], &[]);
        fixture
    }

    fn command(&self, args: &[&str], shell: &[(&str, &str)]) -> Output {
        let mut command = Command::new(ensure_binaries_built());
        command.env_clear();
        // Runtime executables are already built. No user credentials, remote
        // settings, or ambient module variables enter these child processes.
        for key in ["PATH", "SystemRoot", "WINDIR", "TMP", "TEMP", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("HOME", self.test.project_dir.path())
            .env("USERPROFILE", self.test.project_dir.path())
            .env("XDG_CONFIG_HOME", self.test.project_dir.path())
            .env("NO_PROXY", "*")
            .env("no_proxy", "*")
            .envs(shell.iter().copied())
            .arg("--config-path")
            .arg(&self.test.config_path)
            .args(args)
            .current_dir(self.test.project_dir.path())
            .stdin(Stdio::null());
        bounded_output(command)
    }

    fn success(&self, args: &[&str], shell: &[(&str, &str)]) -> String {
        let output = self.command(args, shell);
        assert!(
            output.status.success(),
            "local ENV CLI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn config(&self, environment: Option<Value>) {
        for file in [
            "spacetime.local.json",
            "spacetime.prod.json",
            "spacetime.prod.local.json",
        ] {
            let path = self.test.project_dir.path().join(file);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        let mut config = json!({"database": self.database});
        if let Some(environment) = environment {
            config["env"] = environment;
        }
        self.write("spacetime.json", config);
    }

    fn write(&self, file: &str, value: Value) {
        fs::write(
            self.test.project_dir.path().join(file),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    fn publish(&self, shell: &[(&str, &str)], extra: &[&str]) -> Output {
        let mut args = vec![
            "publish",
            &self.database,
            "--bin-path",
            self.wasm.to_str().unwrap(),
            "--server",
            &self.test.server_url,
            "--yes",
        ];
        args.extend_from_slice(extra);
        self.command(&args, shell)
    }

    fn published(&self, shell: &[(&str, &str)], extra: &[&str]) -> String {
        let before = fs::read(&self.wasm).unwrap();
        let output = self.publish(shell, extra);
        assert!(
            output.status.success(),
            "publish failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&self.wasm).unwrap(), before);
        String::from_utf8(output.stdout).unwrap()
    }

    fn get(&self, key: &str) -> String {
        self.success(
            &[
                "env",
                "get",
                &self.database,
                key,
                "--server",
                &self.test.server_url,
                "--no-config",
            ],
            &[],
        )
    }

    fn list(&self) -> String {
        self.success(
            &[
                "env",
                "list",
                &self.database,
                "--server",
                &self.test.server_url,
                "--no-config",
            ],
            &[],
        )
    }

    fn typed(&self, required: &str, mode: &str, rest: [Option<&str>; 4]) {
        let option = |value: Option<&str>| match value {
            Some(value) => json!({"some": value}),
            None => json!({"none": []}),
        };
        let arguments = [
            json!(required),
            json!(mode),
            option(rest[0]),
            option(rest[1]),
            option(rest[2]),
            option(rest[3]),
        ]
        .map(|value| value.to_string());
        let mut args = vec![
            "call",
            &self.database,
            "check_environment",
            "--no-config",
            "--server",
            &self.test.server_url,
        ];
        args.extend(arguments.iter().map(String::as_str));
        self.success(&args, &[]);
    }

    fn sql(&self, statement: &str) -> Output {
        self.command(
            &[
                "sql",
                &self.database,
                statement,
                "--server",
                &self.test.server_url,
                "--no-config",
            ],
            &[],
        )
    }
}

// Keep ownership through failure/timeout and avoid pipe backpressure. Output is
// generated fixture data; the cap also prevents accidental unbounded diagnostics.
fn bounded_output(mut command: Command) -> Output {
    struct OwnedChild(Option<Child>);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if let Some(child) = self.0.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut child = OwnedChild(Some(
        command
            .stdout(Stdio::from(stdout.try_clone().unwrap()))
            .stderr(Stdio::from(stderr.try_clone().unwrap()))
            .spawn()
            .unwrap(),
    ));
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        assert!(
            stdout.metadata().unwrap().len() <= 1024 * 1024 && stderr.metadata().unwrap().len() <= 1024 * 1024,
            "local ENV CLI output exceeds bound"
        );
        if let Some(status) = child.0.as_mut().unwrap().try_wait().unwrap() {
            child.0.take(); // Already reaped: never signal this process identifier again.
            break status;
        }
        assert!(Instant::now() < deadline, "local ENV CLI command timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    stdout.rewind().unwrap();
    stderr.rewind().unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout.read_to_end(&mut out).unwrap();
    stderr.read_to_end(&mut err).unwrap();
    Output {
        status,
        stdout: out,
        stderr: err,
    }
}

#[test]
fn cli_environment_layers_shell_and_exact_precompiled_declarations() {
    let f = Fixture::new();
    f.write(
        "spacetime.json",
        json!({"database":"unused-parent", "env":{
        "SMOKE_REQUIRED":"base-required", "SMOKE_MODE":"ready", "SMOKE_OPTIONAL":"base-optional",
        "SMOKE_EMPTY":"base-empty", "SMOKE_FLAG": false
    }, "children":[{"database":f.database,"env":{"SMOKE_OPTIONAL":"child-optional"}}]}),
    );
    // Use a raw JSON number to verify there is no f64 round trip.
    fs::write(f.test.project_dir.path().join("spacetime.local.json"),
        r#"{"env":{"SMOKE_REQUIRED":"local-required","SMOKE_NUMBER":9007199254740993123456789},"children":[{"env":{"SMOKE_OPTIONAL":"child-local"}}]}"#).unwrap();
    f.write(
        "spacetime.prod.json",
        json!({"env":{"SMOKE_REQUIRED":"prod-required","SMOKE_MODE":"other"},
        "children":[{"env":{"SMOKE_EMPTY":"child-prod"}}]}),
    );
    f.write(
        "spacetime.prod.local.json",
        json!({"env":{"SMOKE_REQUIRED":"prod-local-required"},
        "children":[{"env":{"SMOKE_OPTIONAL":"child-final"}}]}),
    );
    let output = f.published(
        &[
            ("SMOKE_REQUIRED", "shell-required"),
            ("SMOKE_EMPTY", ""),
            ("SMOKE_UNDECLARED", "ambient-not-published"),
        ],
        &["--env", "prod"],
    );
    for key in KEYS {
        assert!(output.contains(key));
    }
    for value in [
        "shell-required",
        "child-final",
        "9007199254740993123456789",
        "ambient-not-published",
    ] {
        assert!(!output.contains(value), "publish display leaked a fixture value");
    }
    assert!(output.contains("SMOKE_REQUIRED (shell)"));
    assert!(output.contains("SMOKE_NUMBER (config)"));
    assert_eq!(f.get("SMOKE_EMPTY"), "\n");
    assert_eq!(f.get("SMOKE_NUMBER"), "9007199254740993123456789\n");
    assert_eq!(f.get("SMOKE_FLAG"), "false\n");
    f.typed(
        "shell-required",
        "other",
        [
            Some("child-final"),
            Some(""),
            Some("9007199254740993123456789"),
            Some("false"),
        ],
    );
    let mut keys = KEYS.to_vec();
    keys.sort_unstable();
    assert_eq!(f.list(), format!("{}\n", keys.join("\n")));
    let initial = f.sql("SELECT required, mode FROM initial_environment");
    assert!(initial.status.success());
    let initial = String::from_utf8(initial.stdout).unwrap();
    assert!(initial.contains("shell-required") && initial.contains("other"));
}

#[test]
fn cli_environment_replacement_rejection_and_read_only_commands() {
    let f = Fixture::new();
    f.config(Some(
        json!({"SMOKE_REQUIRED":"initial-sentinel","SMOKE_MODE":"ready","SMOKE_OPTIONAL":"remove-me"}),
    ));
    f.published(&[], &[]);
    f.config(Some(
        json!({"SMOKE_REQUIRED":"replacement-sentinel","SMOKE_MODE":"other"}),
    ));
    f.published(&[], &[]);
    f.typed("replacement-sentinel", "other", [None; 4]);
    assert_eq!(f.list(), "SMOKE_MODE\nSMOKE_REQUIRED\n");
    assert!(!f
        .command(
            &[
                "env",
                "get",
                &f.database,
                "SMOKE_OPTIONAL",
                "--server",
                &f.test.server_url,
                "--no-config"
            ],
            &[]
        )
        .status
        .success());
    for input in [
        json!({"SMOKE_MODE":"ready"}),
        json!({"SMOKE_REQUIRED":"rejected-sentinel","SMOKE_MODE":"invalid-sentinel"}),
        json!({"SMOKE_REQUIRED":"rejected-sentinel","SMOKE_MODE":"ready","UNKNOWN":"unknown-sentinel"}),
        json!({"SMOKE_REQUIRED":"rejected-sentinel","SMOKE_MODE":"ready","SMOKE_OPTIONAL":{}}),
    ] {
        f.config(Some(input));
        let output = f.publish(&[], &[]);
        assert!(!output.status.success());
        for value in ["rejected-sentinel", "invalid-sentinel", "unknown-sentinel"] {
            assert!(!String::from_utf8_lossy(&output.stdout).contains(value));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(value));
        }
        assert_eq!(f.get("SMOKE_REQUIRED"), "replacement-sentinel\n");
        f.typed("replacement-sentinel", "other", [None; 4]);
    }
    // Invalid local configuration must not prevent an explicit read-only target.
    assert_eq!(f.list(), "SMOKE_MODE\nSMOKE_REQUIRED\n");
    for statement in [
        "SET env.SMOKE_REQUIRED = 'bypass'",
        "DELETE env.SMOKE_REQUIRED",
        "INSERT INTO st_env (key, value) VALUES ('BYPASS', 'value')",
        "UPDATE st_env SET value = 'bypass'",
        "DELETE FROM st_env",
    ] {
        assert!(!f.sql(statement).status.success());
        assert_eq!(f.get("SMOKE_REQUIRED"), "replacement-sentinel\n");
    }
    for operation in ["set", "delete", "unset"] {
        assert!(!f
            .command(
                &[
                    "env",
                    operation,
                    &f.database,
                    "SMOKE_REQUIRED",
                    "--server",
                    &f.test.server_url
                ],
                &[]
            )
            .status
            .success());
    }
}

#[test]
fn cli_environment_initial_rejection_clear_and_omitted_payload() {
    let mut f = Fixture::new();
    f.config(None);
    assert!(!f.publish(&[], &[]).status.success());
    f.config(Some(json!({"SMOKE_REQUIRED":"clear-initial","SMOKE_MODE":"ready"})));
    f.published(&[], &[]);
    f.config(Some(
        json!({"SMOKE_REQUIRED":"clear-replaced","SMOKE_MODE":"other","SMOKE_EMPTY":""}),
    ));
    f.published(&[], &["--delete-data"]);
    f.typed("clear-replaced", "other", [None, Some(""), None, None]);
    let initial = f.sql("SELECT required FROM initial_environment");
    assert!(initial.status.success());
    let initial = String::from_utf8(initial.stdout).unwrap();
    assert!(initial.contains("clear-replaced") && !initial.contains("clear-initial"));
    // A legacy module with no ENV declaration receives an empty complete input.
    f.config(None);
    f.wasm = modules::precompiled_module("noop");
    f.published(&[("SMOKE_REQUIRED", "must-not-be-ambient")], &["--delete-data"]);
    assert_eq!(f.list(), "");
}
