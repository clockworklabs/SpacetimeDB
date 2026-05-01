#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use spacetimedb_language_test_support::{print_results, target_dir, Outcome, SpacetimeDbGuard, TestCaseResult};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Clone, Debug, Default, Parser)]
#[command(disable_help_flag = true)]
struct Args {
    #[arg(long)]
    filter: Option<String>,

    #[arg(long, alias = "list-tests")]
    list: bool,

    #[arg(skip)]
    passthrough: Vec<String>,

    #[arg()]
    positional: Vec<String>,
}

impl Args {
    fn parse() -> Self {
        let mut args = env::args().collect::<Vec<_>>();
        let passthrough = args
            .iter()
            .position(|arg| arg == "--")
            .map(|index| args.split_off(index + 1))
            .unwrap_or_default();
        if args.last().is_some_and(|arg| arg == "--") {
            args.pop();
        }

        let mut parsed = <Args as Parser>::parse_from(args);
        parsed.passthrough = passthrough;
        if parsed.filter.is_none()
            && let Some(filter) = parsed.positional.first()
        {
            parsed.filter = Some(filter.clone());
        }
        parsed
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    let workspace = spacetimedb_language_test_support::workspace_root();
    let out_dir = target_dir().join("csharp-tests");
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {}", out_dir.display()))?;

    run_sdk_tests(&workspace, &out_dir, &args)?;
    if !args.list {
        run_regression_tests(&workspace)?;
    }

    Ok(())
}

fn run_sdk_tests(workspace: &Path, out_dir: &Path, args: &Args) -> Result<()> {
    prepare_csharp_sdk_solution(workspace)?;
    let cwd = workspace.join("sdks/csharp");
    run_dotnet_test(
        "csharp sdk",
        &cwd,
        out_dir,
        "sdk.trx",
        args,
        &["-warnaserror".to_string(), "--no-restore".to_string()],
    )?;
    Ok(())
}

fn run_dotnet_test(
    suite: &str,
    cwd: &Path,
    out_dir: &Path,
    report_name: &str,
    args: &Args,
    extra_args: &[String],
) -> Result<()> {
    let report = out_dir.join(report_name);

    if args.list {
        let mut list_args = vec!["test".to_string(), "--list-tests".to_string()];
        list_args.extend(extra_args.iter().cloned());
        run_command_forward("dotnet", &list_args, cwd)?;
        return Ok(());
    }

    let mut test_args = vec![
        "test".to_string(),
        "-warnaserror".to_string(),
        "--results-directory".to_string(),
        out_dir.display().to_string(),
        "--logger".to_string(),
        format!("trx;LogFileName={report_name}"),
    ];
    test_args.extend(extra_args.iter().filter(|arg| arg.as_str() != "-warnaserror").cloned());
    if let Some(filter) = &args.filter {
        test_args.push("--filter".to_string());
        test_args.push(filter.clone());
    }
    test_args.extend(args.passthrough.iter().cloned());

    run_command("dotnet", &test_args, cwd)?;
    let actual_report = find_trx(&report, cwd).with_context(|| format!("failed to locate TRX report for {suite}"))?;
    let results = parse_trx(&actual_report).with_context(|| format!("failed to parse {suite} TRX report"))?;
    print_results(suite, &actual_report, &results)?;
    Ok(())
}

fn find_trx(preferred: &Path, cwd: &Path) -> Result<PathBuf> {
    if preferred.exists() {
        return Ok(preferred.to_path_buf());
    }
    let name = preferred
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid TRX report name")?;
    let test_results = cwd.join("TestResults");
    for entry in fs::read_dir(&test_results).with_context(|| format!("failed to read {}", test_results.display()))? {
        let entry = entry?;
        let path = entry.path().join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!("TRX report {} not found", preferred.display())
}

fn prepare_csharp_sdk_solution(workspace: &Path) -> Result<()> {
    run_command(
        "dotnet",
        &["pack".to_string(), "crates/bindings-csharp/BSATN.Runtime".to_string()],
        workspace,
    )?;
    run_command(
        "dotnet",
        &["pack".to_string(), "crates/bindings-csharp/Runtime".to_string()],
        workspace,
    )?;
    run_command(
        "bash",
        &["./tools~/write-nuget-config.sh".to_string(), "../..".to_string()],
        &workspace.join("sdks/csharp"),
    )?;
    run_command(
        "dotnet",
        &[
            "restore".to_string(),
            "--configfile".to_string(),
            "NuGet.Config".to_string(),
            "SpacetimeDB.ClientSDK.sln".to_string(),
        ],
        &workspace.join("sdks/csharp"),
    )?;
    Ok(())
}

fn run_regression_tests(workspace: &Path) -> Result<()> {
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
    run_command_env(
        "bash",
        &["tools~/run-regression-tests.sh".to_string()],
        &workspace.join("sdks/csharp"),
        &[("SPACETIMEDB_SERVER_URL", guard.host_url.clone())],
    )?;
    Ok(())
}

fn run_command(program: &str, args: &[String], cwd: &Path) -> Result<Output> {
    run_command_inner(program, args, cwd, &[])
}

fn run_command_env(program: &str, args: &[String], cwd: &Path, envs: &[(&str, String)]) -> Result<Output> {
    run_command_inner(program, args, cwd, envs)
}

fn run_command_inner(program: &str, args: &[String], cwd: &Path, envs: &[(&str, String)]) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(envs.iter().map(|(k, v)| (OsStr::new(k), OsStr::new(v))))
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line(program, args), cwd.display()))?;
    ensure_success(cwd, program, args, &output)?;
    Ok(output)
}

fn run_command_forward(program: &str, args: &[String], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line(program, args), cwd.display()))?;
    ensure_success(
        cwd,
        program,
        args,
        &Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        },
    )
}

fn ensure_success(cwd: &Path, program: &str, args: &[String], output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "command failed in {}:\n  {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        cwd.display(),
        shell_line(program, args),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn shell_line(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_trx(path: &Path) -> Result<Vec<TestCaseResult>> {
    let mut reader = Reader::from_file(path).with_context(|| format!("failed to read {}", path.display()))?;
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut results = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.name().as_ref() == b"UnitTestResult" => {
                let name = attr(&e, b"testName")?.unwrap_or_else(|| "<unknown>".to_string());
                let outcome = match attr(&e, b"outcome")?.as_deref() {
                    Some("Passed") => Outcome::Passed,
                    Some("NotExecuted") => Outcome::Skipped,
                    Some("Failed") => Outcome::Failed,
                    _ => Outcome::Failed,
                };
                results.push(TestCaseResult {
                    name,
                    outcome,
                    message: None,
                });
            }
            Ok(Event::Eof) => break,
            Err(err) => bail!("failed to parse {}: {err}", path.display()),
            _ => {}
        }
        buf.clear();
    }

    Ok(results)
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Result<Option<String>> {
    for attr in e.attributes() {
        let attr = attr?;
        if attr.key.as_ref() == key {
            return Ok(Some(String::from_utf8_lossy(attr.value.as_ref()).to_string()));
        }
    }
    Ok(None)
}
