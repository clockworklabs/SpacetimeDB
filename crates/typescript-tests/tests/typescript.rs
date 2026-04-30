#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use spacetimedb_language_test_support::{
    artifact_dir, parse_junit, print_results, require_tool, run_command, run_command_forward, HarnessArgs,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = HarnessArgs::parse();
    require_tool("pnpm")?;

    let workspace = spacetimedb_language_test_support::workspace_root();
    let cwd = workspace.join("crates/bindings-typescript");
    let out_dir = artifact_dir("typescript")?;
    let report = out_dir.join("vitest.junit.xml");

    if args.list {
        let mut cmd = vec!["vitest".to_string(), "list".to_string()];
        if let Some(filter) = args.filter {
            cmd.push(filter);
        }
        run_command_forward("pnpm", &cmd, &cwd)?;
        return Ok(());
    }

    run_command("pnpm", &["build".to_string()], &cwd)?;

    let mut test_args = vec![
        "vitest".to_string(),
        "run".to_string(),
        "--reporter=default".to_string(),
        "--reporter=junit".to_string(),
        format!("--outputFile={}", report.display()),
    ];
    if let Some(filter) = args.filter {
        test_args.push("-t".to_string());
        test_args.push(filter);
    }
    test_args.extend(args.passthrough);
    run_command("pnpm", &test_args, &cwd)?;

    let results = parse_junit(&report).with_context(|| "failed to parse TypeScript Vitest JUnit report")?;
    print_results("typescript", &report, &results)?;

    let typecheck_report = out_dir.join("vitest-typecheck.junit.xml");
    let typecheck_args = vec![
        "vitest".to_string(),
        "typecheck".to_string(),
        "--run".to_string(),
        "--reporter=default".to_string(),
        "--reporter=junit".to_string(),
        format!("--outputFile={}", typecheck_report.display()),
    ];
    run_command("pnpm", &typecheck_args, &cwd)?;
    if typecheck_report.exists() {
        let results = parse_junit(&typecheck_report)
            .with_context(|| "failed to parse TypeScript Vitest typecheck JUnit report")?;
        print_results("typescript typecheck", &typecheck_report, &results)?;
    }

    Ok(())
}
