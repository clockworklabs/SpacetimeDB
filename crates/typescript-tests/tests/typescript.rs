#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use spacetimedb_language_test_support::{print_results, target_dir, Outcome, TestCaseResult};
use std::fs;
use std::path::Path;
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
    let cwd = workspace.join("crates/bindings-typescript");
    let out_dir = target_dir().join("typescript-tests");
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {}", out_dir.display()))?;
    let report = out_dir.join("vitest.junit.xml");

    if args.list {
        return list_tests(&cwd, args.filter);
    }

    run_tests(&cwd, &report, args.filter, args.passthrough)
}

fn list_tests(cwd: &Path, filter: Option<String>) -> Result<()> {
    let mut cmd = vec!["vitest".to_string(), "list".to_string()];
    if let Some(filter) = filter {
        cmd.push(filter);
    }
    let command_line = shell_line("pnpm", &cmd);
    let status = Command::new("pnpm")
        .args(&cmd)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{command_line}` in {}", cwd.display()))?;
    ensure_success(cwd, &command_line, status)?;
    Ok(())
}

fn run_tests(cwd: &Path, report: &Path, filter: Option<String>, passthrough: Vec<String>) -> Result<()> {
    let build_args = ["build".to_string()];
    let command_line = shell_line("pnpm", &build_args);
    let status = Command::new("pnpm")
        .args(&build_args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{command_line}` in {}", cwd.display()))?;
    ensure_success(cwd, &command_line, status)?;

    let mut test_args = vec![
        "test".to_string(),
        "--".to_string(),
        "--reporter=default".to_string(),
        "--reporter=junit".to_string(),
        format!("--outputFile={}", report.display()),
    ];
    if let Some(filter) = filter {
        test_args.push("-t".to_string());
        test_args.push(filter);
    }
    test_args.extend(passthrough);
    let command_line = shell_line("pnpm", &test_args);
    let status = Command::new("pnpm")
        .args(&test_args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{command_line}` in {}", cwd.display()))?;
    ensure_success(cwd, &command_line, status)?;

    let results = parse_junit(&report).with_context(|| "failed to parse TypeScript Vitest JUnit report")?;
    print_results("typescript", &report, &results)?;

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

fn parse_junit(path: &Path) -> Result<Vec<TestCaseResult>> {
    let mut reader = Reader::from_file(path).with_context(|| format!("failed to read {}", path.display()))?;
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut results = Vec::new();
    let mut current: Option<TestCaseResult> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => match e.name().as_ref() {
                b"testcase" => {
                    let name = attr(&e, b"name")?.unwrap_or_else(|| "<unknown>".to_string());
                    let class = attr(&e, b"classname")?;
                    let name = class.map(|class| format!("{class}::{name}")).unwrap_or(name);
                    current = Some(TestCaseResult {
                        name,
                        outcome: Outcome::Passed,
                        message: None,
                    });
                    if e.is_empty()
                        && let Some(case) = current.take()
                    {
                        results.push(case);
                    }
                }
                b"failure" | b"error" => {
                    if let Some(case) = current.as_mut() {
                        case.outcome = Outcome::Failed;
                        case.message = attr(&e, b"message")?;
                    }
                }
                b"skipped" => {
                    if let Some(case) = current.as_mut() {
                        case.outcome = Outcome::Skipped;
                        case.message = attr(&e, b"message")?;
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"testcase" => {
                if let Some(case) = current.take() {
                    results.push(case);
                }
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
