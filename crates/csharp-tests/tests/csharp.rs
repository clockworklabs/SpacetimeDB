#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use spacetimedb_language_test_support::{print_results, target_dir, Outcome, SpacetimeDbGuard, TestCaseResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

#[derive(Clone, Debug, Default, Parser)]
struct Args {
    #[arg(long)]
    filter: Option<String>,

    #[arg(long, alias = "list-tests")]
    list: bool,

    #[arg(last = true)]
    passthrough: Vec<String>,
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
        let status = Command::new("dotnet")
            .args(&list_args)
            .current_dir(cwd)
            .status()
            .with_context(|| {
                format!(
                    "failed to spawn `{}` in {}",
                    shell_line("dotnet", &list_args),
                    cwd.display()
                )
            })?;
        ensure_status_success(cwd, "dotnet", &list_args, status)?;
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

    let output = Command::new("dotnet")
        .args(&test_args)
        .current_dir(cwd)
        .output()
        .with_context(|| {
            format!(
                "failed to spawn `{}` in {}",
                shell_line("dotnet", &test_args),
                cwd.display()
            )
        })?;
    ensure_success(cwd, "dotnet", &test_args, &output)?;
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
    let args = ["pack".to_string(), "crates/bindings-csharp/BSATN.Runtime".to_string()];
    let output = Command::new("dotnet")
        .args(&args)
        .current_dir(workspace)
        .output()
        .with_context(|| {
            format!(
                "failed to spawn `{}` in {}",
                shell_line("dotnet", &args),
                workspace.display()
            )
        })?;
    ensure_success(workspace, "dotnet", &args, &output)?;

    let args = ["pack".to_string(), "crates/bindings-csharp/Runtime".to_string()];
    let output = Command::new("dotnet")
        .args(&args)
        .current_dir(workspace)
        .output()
        .with_context(|| {
            format!(
                "failed to spawn `{}` in {}",
                shell_line("dotnet", &args),
                workspace.display()
            )
        })?;
    ensure_success(workspace, "dotnet", &args, &output)?;

    let cwd = workspace.join("sdks/csharp");
    let args = ["./tools~/write-nuget-config.sh".to_string(), "../..".to_string()];
    let output = Command::new("bash")
        .args(&args)
        .current_dir(&cwd)
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line("bash", &args), cwd.display()))?;
    ensure_success(&cwd, "bash", &args, &output)?;

    let args = [
        "restore".to_string(),
        "--configfile".to_string(),
        "NuGet.Config".to_string(),
        "SpacetimeDB.ClientSDK.sln".to_string(),
    ];
    let output = Command::new("dotnet")
        .args(&args)
        .current_dir(&cwd)
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line("dotnet", &args), cwd.display()))?;
    ensure_success(&cwd, "dotnet", &args, &output)?;
    Ok(())
}

fn run_regression_tests(workspace: &Path) -> Result<()> {
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let cwd = workspace.join("sdks/csharp");
    let args = ["tools~/run-regression-tests.sh".to_string()];
    let output = Command::new("bash")
        .args(&args)
        .current_dir(&cwd)
        .env("SPACETIMEDB_SERVER_URL", &guard.host_url)
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line("bash", &args), cwd.display()))?;
    ensure_success(&cwd, "bash", &args, &output)?;
    Ok(())
}

fn ensure_status_success(cwd: &Path, program: &str, args: &[String], status: ExitStatus) -> Result<()> {
    if status.success() {
        return Ok(());
    }

    bail!(
        "command failed in {}:\n  {}\nstatus: {}",
        cwd.display(),
        shell_line(program, args),
        status,
    );
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
