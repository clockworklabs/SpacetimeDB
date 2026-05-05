#![allow(clippy::disallowed_macros)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::Value;
use spacetimedb_language_test_support::{
    print_results, target_dir, workspace_root, Outcome, SpacetimeDbGuard, TestCaseResult,
};
use tempfile::TempDir;

const UNITY_VERSION: &str = "2022.3.32f1";
const SDK_PACKAGE: &str = "com.clockworklabs.spacetimedbsdk";
const SDK_PACKAGE_PATH: &str = "file:../../../../sdks/csharp";

#[derive(Parser)]
struct Args {
    #[arg(long)]
    unity_path: Option<PathBuf>,
    #[arg(long)]
    filter: Option<String>,
    #[arg(long, alias = "list-tests")]
    list: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    passthrough: Vec<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let workspace = workspace_root();
    let server_dir = workspace.join("demo/Blackholio/server-rust");
    let unity_project_dir = workspace.join("demo/Blackholio/client-unity");

    if args.list {
        list_unity_tests(&unity_project_dir)?;
        return Ok(());
    }

    let unity = find_unity(args.unity_path.as_deref())?;
    let spacetime_bin = SpacetimeBin::prepare()?;
    let _server_cargo_restore = patch_blackholio_server_manifest(&server_dir.join("Cargo.toml"))?;
    let _unity_manifest_restore = patch_unity_package_manifest(&unity_project_dir.join("Packages/manifest.json"))?;

    run_command(
        &server_dir,
        "bash",
        &["./generate.sh".into(), "-y".into()],
        Some(spacetime_bin.path_env()),
        &[],
    )
    .context("failed to generate Blackholio Unity bindings")?;

    run_command(
        &workspace,
        "bash",
        &[
            "tools/check-diff.sh".into(),
            "demo/Blackholio/client-unity/Assets/Scripts/autogen".into(),
        ],
        None,
        &[],
    )
    .context("generated Blackholio Unity bindings differ from the checked-in files")?;

    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();

    run_command(
        &server_dir,
        "spacetime",
        &["logout".into()],
        Some(spacetime_bin.path_env()),
        &[],
    )
    .context("failed to log out of local SpacetimeDB")?;
    run_command(
        &server_dir,
        "spacetime",
        &["login".into(), "--server-issued-login".into(), server.host_url.clone()],
        Some(spacetime_bin.path_env()),
        &[],
    )
    .context("failed to log in to local SpacetimeDB")?;
    run_command(
        &server_dir,
        "bash",
        &["./publish.sh".into()],
        Some(spacetime_bin.path_env()),
        &[("SPACETIMEDB_SERVER_URL", server.host_url.as_str())],
    )
    .context("failed to publish the Blackholio module")?;

    run_unity_tests(
        &unity,
        &unity_project_dir,
        &server.host_url,
        args.filter.as_deref(),
        &args.passthrough,
    )
}

fn run_unity_tests(
    unity: &UnityRunner,
    project_dir: &Path,
    server_url: &str,
    filter: Option<&str>,
    passthrough: &[String],
) -> Result<()> {
    let out_dir = target_dir().join("unity-tests");
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {}", out_dir.display()))?;
    let results_path = out_dir.join("results.xml");
    let log_path = out_dir.join("unity.log");
    remove_file_if_exists(&results_path)?;
    remove_file_if_exists(&log_path)?;

    let planned_tests = discover_unity_tests(project_dir)?;
    println!("unity playmode: discovered {} tests before run", planned_tests.len());
    for test in &planned_tests {
        println!("planned {test}");
    }

    let mut args = vec![
        "-batchmode".to_string(),
        "-nographics".to_string(),
        "-quit".to_string(),
        "-projectPath".to_string(),
        project_dir.display().to_string(),
        "-runTests".to_string(),
        "-testPlatform".to_string(),
        "playmode".to_string(),
        "-testResults".to_string(),
        results_path.display().to_string(),
        "-logFile".to_string(),
        log_path.display().to_string(),
    ];
    if let Some(filter) = filter {
        args.push("-testFilter".to_string());
        args.push(filter.to_string());
    }
    args.extend_from_slice(passthrough);

    let (status, command_line) = unity
        .run(&args, server_url)
        .context("failed to run Unity playmode tests")?;

    if results_path.exists() {
        let results = parse_unity_results(&results_path)?;
        print_results("unity playmode", &results_path, &results)?;
    } else {
        if log_path.exists() {
            print_log_excerpt(&log_path)?;
        }
        bail!(
            "Unity exited with {status} but did not write a test result file at {}; cannot determine which tests actually ran. Command: {command_line}",
            results_path.display(),
        );
    }

    ensure_success(status, &command_line)
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

enum UnityRunner {
    Native(PathBuf),
    Docker { image: String },
}

impl UnityRunner {
    fn run(&self, unity_args: &[String], server_url: &str) -> Result<(ExitStatus, String)> {
        match self {
            UnityRunner::Native(path) => {
                let mut args = unity_args.to_vec();
                add_unity_license_args(&mut args);
                let status = Command::new(path)
                    .args(&args)
                    .env("SPACETIMEDB_SERVER_URL", server_url)
                    .status()
                    .with_context(|| format!("failed to run {}", path.display()))?;
                Ok((status, shell_line(path, &redact_unity_args(&args))))
            }
            UnityRunner::Docker { image } => {
                let workspace = workspace_root();
                let mut docker_args = vec![
                    "run".to_string(),
                    "--rm".to_string(),
                    "--network".to_string(),
                    "host".to_string(),
                    "-e".to_string(),
                    format!("SPACETIMEDB_SERVER_URL={server_url}"),
                ];
                for var in ["UNITY_EMAIL", "UNITY_PASSWORD", "UNITY_SERIAL", "UNITY_LICENSE"] {
                    if env::var_os(var).is_some() {
                        docker_args.push("-e".to_string());
                        docker_args.push(var.to_string());
                    }
                }
                docker_args.extend([
                    "-v".to_string(),
                    format!("{}:{}", workspace.display(), workspace.display()),
                    "-w".to_string(),
                    workspace.display().to_string(),
                    image.clone(),
                    "/opt/unity/Editor/Unity".to_string(),
                ]);

                let mut args = unity_args.to_vec();
                add_unity_license_args(&mut args);
                docker_args.extend(args);

                let status = Command::new("docker")
                    .args(&docker_args)
                    .status()
                    .context("failed to run docker")?;
                Ok((status, shell_line("docker", &redact_unity_args(&docker_args))))
            }
        }
    }
}

fn add_unity_license_args(args: &mut Vec<String>) {
    if let (Ok(email), Ok(password), Ok(serial)) = (
        env::var("UNITY_EMAIL"),
        env::var("UNITY_PASSWORD"),
        env::var("UNITY_SERIAL"),
    ) {
        args.extend([
            "-username".to_string(),
            email,
            "-password".to_string(),
            password,
            "-serial".to_string(),
            serial,
        ]);
    }
}

fn redact_unity_args(args: &[String]) -> Vec<String> {
    let mut redacted = args.to_vec();
    for index in 1..redacted.len() {
        if matches!(redacted[index - 1].as_str(), "-username" | "-password" | "-serial") {
            redacted[index] = "<redacted>".to_string();
        }
    }
    redacted
}

fn find_unity(explicit_path: Option<&Path>) -> Result<UnityRunner> {
    let version = env::var("UNITY_VERSION").unwrap_or_else(|_| UNITY_VERSION.to_string());

    if let Some(path) = explicit_path {
        if path.exists() {
            return Ok(UnityRunner::Native(path.to_path_buf()));
        }
        bail!("Unity executable does not exist: {}", path.display());
    }

    for var in ["UNITY_PATH", "UNITY_EXECUTABLE"] {
        if let Some(path) = env::var_os(var).map(PathBuf::from).filter(|path| path.exists()) {
            return Ok(UnityRunner::Native(path));
        }
    }

    for name in ["unity", "Unity", "unity-editor"] {
        if let Some(path) = find_on_path(name) {
            return Ok(UnityRunner::Native(path));
        }
    }

    let mut candidates = vec![
        PathBuf::from(format!("/opt/unity/editors/{version}/Editor/Unity")),
        PathBuf::from(format!("/opt/Unity/Hub/Editor/{version}/Editor/Unity")),
        PathBuf::from("/opt/unity/Editor/Unity"),
        PathBuf::from("/opt/Unity/Editor/Unity"),
        PathBuf::from(format!(
            "/Applications/Unity/Hub/Editor/{version}/Unity.app/Contents/MacOS/Unity"
        )),
    ];
    if let Some(home) = env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(format!("Unity/Hub/Editor/{version}/Editor/Unity")));
    }

    for path in candidates {
        if path.exists() {
            return Ok(UnityRunner::Native(path));
        }
    }

    if let Some(image) = unity_docker_image(&version) {
        return Ok(UnityRunner::Docker { image });
    }

    bail!(
        "could not find Unity. Pass --unity-path, set UNITY_PATH or UNITY_EXECUTABLE, install Unity {version} in a standard GitHub runner path, or set UNITY_USE_DOCKER=1"
    )
}

fn unity_docker_image(version: &str) -> Option<String> {
    if let Ok(image) = env::var("UNITY_DOCKER_IMAGE")
        && !image.is_empty()
    {
        return Some(image);
    }
    if env::var_os("UNITY_USE_DOCKER").as_deref() == Some(std::ffi::OsStr::new("1")) {
        return Some(format!("unityci/editor:ubuntu-{version}-base-3"));
    }
    None
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.exists())
}

struct SpacetimeBin {
    _temp_dir: TempDir,
    path_env: OsString,
}

impl SpacetimeBin {
    fn prepare() -> Result<Self> {
        let temp_dir = tempfile::tempdir().context("failed to create temporary bin directory")?;
        let release_dir = target_dir().join("release");
        let exe = env::consts::EXE_SUFFIX;
        let cli = release_dir.join(format!("spacetimedb-cli{exe}"));
        let standalone = release_dir.join(format!("spacetimedb-standalone{exe}"));

        ensure_exists(&cli, "release spacetimedb-cli")?;
        ensure_exists(&standalone, "release spacetimedb-standalone")?;

        link_or_copy(&cli, &temp_dir.path().join(format!("spacetime{exe}")))?;
        link_or_copy(&cli, &temp_dir.path().join(format!("spacetimedb-cli{exe}")))?;
        link_or_copy(
            &standalone,
            &temp_dir.path().join(format!("spacetimedb-standalone{exe}")),
        )?;

        let mut paths = vec![temp_dir.path().to_path_buf()];
        if let Some(path) = env::var_os("PATH") {
            paths.extend(env::split_paths(&path));
        }
        let path_env = env::join_paths(paths).context("failed to build PATH for SpacetimeDB binaries")?;

        Ok(Self {
            _temp_dir: temp_dir,
            path_env,
        })
    }

    fn path_env(&self) -> &OsString {
        &self.path_env
    }
}

fn ensure_exists(path: &Path, label: &str) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        bail!(
            "missing {label} at {}. Run this through `cargo ci unity-tests` so CargoCI builds the required binaries first",
            path.display()
        )
    }
}

#[cfg(unix)]
fn link_or_copy(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(not(unix))]
fn link_or_copy(src: &Path, dst: &Path) -> io::Result<()> {
    fs::copy(src, dst).map(|_| ())
}

struct FileRestore {
    path: PathBuf,
    original: String,
}

impl Drop for FileRestore {
    fn drop(&mut self) {
        let _ = fs::write(&self.path, &self.original);
    }
}

fn patch_blackholio_server_manifest(path: &Path) -> Result<FileRestore> {
    let original = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let replacement = r#"spacetimedb = { path = "../../../crates/bindings" }"#;
    let mut replaced = false;
    let mut patched = String::new();

    for line in original.lines() {
        if line.trim_start().starts_with("spacetimedb =") {
            let indent_len = line.len() - line.trim_start().len();
            patched.push_str(&line[..indent_len]);
            patched.push_str(replacement);
            replaced = true;
        } else {
            patched.push_str(line);
        }
        patched.push('\n');
    }

    if !replaced {
        bail!("could not find spacetimedb dependency in {}", path.display());
    }

    fs::write(path, patched).with_context(|| format!("failed to patch {}", path.display()))?;
    Ok(FileRestore {
        path: path.to_path_buf(),
        original,
    })
}

fn patch_unity_package_manifest(path: &Path) -> Result<FileRestore> {
    let original = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut manifest: Value =
        serde_json::from_str(&original).with_context(|| format!("failed to parse {}", path.display()))?;

    let dependencies = manifest
        .get_mut("dependencies")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("{} does not contain a dependencies object", path.display()))?;
    dependencies.insert(SDK_PACKAGE.to_string(), Value::String(SDK_PACKAGE_PATH.to_string()));

    let patched = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    fs::write(path, patched).with_context(|| format!("failed to patch {}", path.display()))?;
    Ok(FileRestore {
        path: path.to_path_buf(),
        original,
    })
}

fn run_command(
    cwd: &Path,
    program: &str,
    args: &[String],
    path_env: Option<&OsString>,
    envs: &[(&str, &str)],
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    if let Some(path_env) = path_env {
        command.env("PATH", path_env);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run {}", shell_line(program, args)))?;
    ensure_success(status, &shell_line(program, args))
}

fn ensure_success(status: ExitStatus, command: &str) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!("command failed with {status}: {command}")
    }
}

fn parse_unity_results(path: &Path) -> Result<Vec<TestCaseResult>> {
    let mut reader = Reader::from_file(path).with_context(|| format!("failed to read {}", path.display()))?;
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut cases = Vec::new();
    let mut current_case: Option<TestCaseResult> = None;
    let mut in_message = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(event) if event.name().as_ref() == b"test-case" => {
                current_case = Some(test_case_from_event(&event)?);
            }
            Event::Empty(event) if event.name().as_ref() == b"test-case" => {
                cases.push(test_case_from_event(&event)?);
            }
            Event::Start(event) if event.name().as_ref() == b"message" && current_case.is_some() => {
                in_message = true;
            }
            Event::Text(event) if in_message => {
                if let Some(case) = &mut current_case {
                    let message = String::from_utf8_lossy(event.as_ref()).into_owned();
                    if !message.is_empty() {
                        case.message = Some(message);
                    }
                }
            }
            Event::End(event) if event.name().as_ref() == b"message" => {
                in_message = false;
            }
            Event::End(event) if event.name().as_ref() == b"test-case" => {
                if let Some(case) = current_case.take() {
                    cases.push(case);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(cases)
}

fn test_case_from_event(event: &BytesStart<'_>) -> Result<TestCaseResult> {
    let name = attr(event, b"fullname")?
        .or(attr(event, b"name")?)
        .unwrap_or_else(|| "<unknown Unity test>".to_string());
    let result = attr(event, b"result")?.unwrap_or_else(|| "Unknown".to_string());
    let outcome = match result.as_str() {
        "Passed" => Outcome::Passed,
        "Skipped" | "Inconclusive" => Outcome::Skipped,
        _ => Outcome::Failed,
    };

    Ok(TestCaseResult {
        name,
        outcome,
        message: None,
    })
}

fn attr(event: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in event.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == key {
            return Ok(Some(String::from_utf8_lossy(&attr.value).into_owned()));
        }
    }
    Ok(None)
}

fn print_log_excerpt(path: &Path) -> Result<()> {
    let log = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    eprintln!(
        "Unity did not write a test result file. Full log from {}:",
        path.display()
    );
    eprint!("{log}");
    if !log.ends_with('\n') {
        eprintln!();
    }

    Ok(())
}

fn list_unity_tests(project_dir: &Path) -> Result<()> {
    for test in discover_unity_tests(project_dir)? {
        println!("{test}");
    }
    Ok(())
}

fn discover_unity_tests(project_dir: &Path) -> Result<Vec<String>> {
    let tests_dir = project_dir.join("Assets/PlayModeTests");
    let mut tests = Vec::new();
    collect_unity_tests(&tests_dir, &mut tests)?;
    tests.sort();
    Ok(tests)
}

fn collect_unity_tests(dir: &Path, tests: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_unity_tests(&path, tests)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("cs") {
            collect_unity_tests_from_file(&path, tests)?;
        }
    }
    Ok(())
}

fn collect_unity_tests_from_file(path: &Path, tests: &mut Vec<String>) -> Result<()> {
    let source = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut class_name = None;
    let mut pending_test = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("public class ") {
            class_name = rest
                .split(|ch: char| ch == ':' || ch.is_whitespace())
                .next()
                .map(str::to_string);
        }
        if trimmed.contains("[UnityTest]") || trimmed.contains("[Test]") {
            pending_test = true;
            continue;
        }
        if pending_test && trimmed.starts_with("public ") {
            if let Some(name) = method_name(trimmed) {
                if let Some(class_name) = &class_name {
                    tests.push(format!("{class_name}.{name}"));
                } else {
                    tests.push(name.to_string());
                }
            }
            pending_test = false;
        }
    }
    Ok(())
}

fn method_name(line: &str) -> Option<&str> {
    let before_args = line.split_once('(')?.0.trim_end();
    before_args.split_whitespace().last()
}

fn shell_line(program: impl AsRef<Path>, args: &[String]) -> String {
    let mut command = shell_escape(program.as_ref().as_os_str().to_string_lossy().as_ref());
    for arg in args {
        command.push(' ');
        command.push_str(&shell_escape(arg));
    }
    command
}

fn shell_escape(arg: &str) -> String {
    if arg.is_empty()
        || arg
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '\'' | '"' | '$' | '\\'))
    {
        format!("'{}'", arg.replace('\'', "'\\''"))
    } else {
        arg.to_string()
    }
}
