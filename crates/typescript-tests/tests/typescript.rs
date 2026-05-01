#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use spacetimedb_language_test_support::{
    parse_junit, print_results, run_command, run_command_forward, target_dir, HarnessArgs,
};
use std::fs;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = HarnessArgs::parse();

    let workspace = spacetimedb_language_test_support::workspace_root();
    let cwd = workspace.join("crates/bindings-typescript");
    let out_dir = target_dir().join("typescript-tests");
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {}", out_dir.display()))?;
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
        "test".to_string(),
        "--".to_string(),
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

    Ok(())
}
