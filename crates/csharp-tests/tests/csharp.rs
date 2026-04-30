#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use spacetimedb_language_test_support::{
    artifact_dir, parse_trx, print_results, require_tool, run_command, run_command_env, run_command_forward,
    HarnessArgs, SpacetimeDbGuard,
};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = HarnessArgs::parse();
    require_tool("dotnet")?;

    let workspace = spacetimedb_language_test_support::workspace_root();
    let out_dir = artifact_dir("csharp")?;

    run_bindings_tests(&workspace, &out_dir, &args)?;
    run_sdk_tests(&workspace, &out_dir, &args)?;
    if !args.list {
        run_regression_tests(&workspace)?;
    }

    Ok(())
}

fn run_bindings_tests(workspace: &Path, out_dir: &Path, args: &HarnessArgs) -> Result<()> {
    let cwd = workspace.join("crates/bindings-csharp");
    run_dotnet_test("csharp bindings", &cwd, out_dir, "bindings.trx", args, &[])?;
    Ok(())
}

fn run_sdk_tests(workspace: &Path, out_dir: &Path, args: &HarnessArgs) -> Result<()> {
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
    args: &HarnessArgs,
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
        &[
            "pack".to_string(),
            "crates/bindings-csharp/BSATN.Runtime".to_string(),
            "-c".to_string(),
            "Release".to_string(),
        ],
        workspace,
    )?;
    run_command(
        "dotnet",
        &[
            "pack".to_string(),
            "crates/bindings-csharp/Runtime".to_string(),
            "-c".to_string(),
            "Release".to_string(),
        ],
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
    require_tool("bash")?;
    // The regression module itself still performs an HTTP egress call to
    // localhost:3000, so this specific suite needs the fixed listen address.
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir_with_listen_addr("127.0.0.1:3000");
    run_command_env(
        "bash",
        &["tools~/run-regression-tests.sh".to_string()],
        &workspace.join("sdks/csharp"),
        &[("SPACETIMEDB_SERVER_URL", guard.host_url.clone())],
    )?;
    Ok(())
}
