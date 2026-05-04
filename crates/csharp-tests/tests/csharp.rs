#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use spacetimedb_language_test_support::{print_results, target_dir, Outcome, SpacetimeDbGuard, TestCaseResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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

    prepare_csharp_sdk_solution(&workspace)?;
    if args.list {
        list_dotnet_tests(&workspace.join("sdks/csharp"))?;
        return Ok(());
    }

    run_dotnet_test(
        "csharp sdk",
        &workspace.join("sdks/csharp"),
        &out_dir,
        "sdk.trx",
        args.filter.as_deref(),
        &args.passthrough,
    )?;
    run_regression_tests(&workspace)?;

    Ok(())
}

fn list_dotnet_tests(cwd: &Path) -> Result<()> {
    let list_args = [
        "test".to_string(),
        "--list-tests".to_string(),
        "-warnaserror".to_string(),
        "--no-restore".to_string(),
    ];
    let command_line = shell_line("dotnet", &list_args);
    let status = Command::new("dotnet")
        .args(&list_args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{command_line}` in {}", cwd.display()))?;
    ensure_success(cwd, &command_line, status)
}

fn run_dotnet_test(
    suite: &str,
    cwd: &Path,
    out_dir: &Path,
    report_name: &str,
    filter: Option<&str>,
    passthrough: &[String],
) -> Result<()> {
    let report = out_dir.join(report_name);

    let mut test_args = vec![
        "test".to_string(),
        "-warnaserror".to_string(),
        "--results-directory".to_string(),
        out_dir.display().to_string(),
        "--logger".to_string(),
        format!("trx;LogFileName={report_name}"),
        "--no-restore".to_string(),
    ];
    if let Some(filter) = filter {
        test_args.push("--filter".to_string());
        test_args.push(filter.to_string());
    }
    test_args.extend(passthrough.iter().cloned());

    let command_line = shell_line("dotnet", &test_args);
    let status = Command::new("dotnet")
        .args(&test_args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{command_line}` in {}", cwd.display()))?;
    ensure_success(cwd, &command_line, status)?;
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
    let status = Command::new("dotnet")
        .args(["pack", "crates/bindings-csharp/BSATN.Runtime"])
        .current_dir(workspace)
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `dotnet pack crates/bindings-csharp/BSATN.Runtime` in {}",
                workspace.display()
            )
        })?;
    ensure_success(workspace, "dotnet pack crates/bindings-csharp/BSATN.Runtime", status)?;

    let status = Command::new("dotnet")
        .args(["pack", "crates/bindings-csharp/Runtime"])
        .current_dir(workspace)
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `dotnet pack crates/bindings-csharp/Runtime` in {}",
                workspace.display()
            )
        })?;
    ensure_success(workspace, "dotnet pack crates/bindings-csharp/Runtime", status)?;

    let cwd = workspace.join("sdks/csharp");
    // Write out the NuGet config file to `nuget.config`. This causes the spacetimedb-csharp-sdk repository
    // to be aware of the local versions of the `bindings-csharp` packages in SpacetimeDB, and use them if
    // available. Otherwise, `spacetimedb-csharp-sdk` will use the NuGet versions of the packages.
    // This means that (if version numbers match) we will test the local versions of the C# packages, even
    // if they're not pushed to NuGet.
    // See https://learn.microsoft.com/en-us/nuget/reference/nuget-config-file for more info on the config file.
    let status = Command::new("bash")
        .args(["./tools~/write-nuget-config.sh", "../.."])
        .current_dir(&cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `bash ./tools~/write-nuget-config.sh ../..` in {}",
                cwd.display()
            )
        })?;
    ensure_success(&cwd, "bash ./tools~/write-nuget-config.sh ../..", status)?;

    let status = Command::new("dotnet")
        .args(["restore", "--configfile", "NuGet.Config", "SpacetimeDB.ClientSDK.sln"])
        .current_dir(&cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `dotnet restore --configfile NuGet.Config SpacetimeDB.ClientSDK.sln` in {}",
                cwd.display()
            )
        })?;
    ensure_success(
        &cwd,
        "dotnet restore --configfile NuGet.Config SpacetimeDB.ClientSDK.sln",
        status,
    )?;
    Ok(())
}

fn run_regression_tests(workspace: &Path) -> Result<()> {
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let cwd = workspace.join("sdks/csharp");
    let status = Command::new("bash")
        .args(["tools~/run-regression-tests.sh"])
        .current_dir(&cwd)
        .env("SPACETIMEDB_SERVER_URL", &guard.host_url)
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `bash tools~/run-regression-tests.sh` in {}",
                cwd.display()
            )
        })?;
    ensure_success(&cwd, "bash tools~/run-regression-tests.sh", status)?;
    Ok(())
}

fn ensure_success(cwd: &Path, command_line: &str, status: ExitStatus) -> Result<()> {
    if status.success() {
        return Ok(());
    }

    bail!(
        "command failed in {}:\n  {}\nstatus: {}",
        cwd.display(),
        command_line,
        status
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
