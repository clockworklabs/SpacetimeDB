#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use spacetimedb_language_test_support::{print_results, target_dir, Outcome, TestCaseResult};
use std::env;
use std::fs;
use std::path::Path;
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

fn run_command(program: &str, args: &[String], cwd: &Path) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
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
