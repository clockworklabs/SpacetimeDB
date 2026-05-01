#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub use spacetimedb_guard::SpacetimeDbGuard;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct TestCaseResult {
    pub name: String,
    pub outcome: Outcome,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Parser)]
#[command(disable_help_flag = true, about = "Runs a wrapped non-Rust language test suite")]
pub struct HarnessArgs {
    /// Filter native tests by name.
    #[arg(long)]
    pub filter: Option<String>,

    /// List native tests instead of running them.
    #[arg(long, alias = "list-tests")]
    pub list: bool,

    #[arg(skip)]
    pub passthrough: Vec<String>,

    #[arg()]
    positional: Vec<String>,
}

impl HarnessArgs {
    pub fn parse() -> Self {
        let mut args = env::args().collect::<Vec<_>>();
        let passthrough = args
            .iter()
            .position(|arg| arg == "--")
            .map(|index| args.split_off(index + 1))
            .unwrap_or_default();
        if args.last().is_some_and(|arg| arg == "--") {
            args.pop();
        }

        let mut parsed = <HarnessArgs as Parser>::parse_from(args);
        parsed.passthrough = passthrough;
        if parsed.filter.is_none()
            && let Some(filter) = parsed.positional.first()
        {
            parsed.filter = Some(filter.clone());
        }

        parsed
    }
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("language-test-support should live under <workspace>/crates")
        .to_path_buf()
}

pub fn target_dir() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

pub fn run_command(program: &str, args: &[String], cwd: &Path) -> Result<Output> {
    run_command_inner(program, args, cwd, &[])
}

pub fn run_command_forward(program: &str, args: &[String], cwd: &Path) -> Result<()> {
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
    )?;
    Ok(())
}

pub fn run_command_env(program: &str, args: &[String], cwd: &Path, envs: &[(&str, String)]) -> Result<Output> {
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

pub fn shell_line(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_trx(path: &Path) -> Result<Vec<TestCaseResult>> {
    parse_xml_results(path, XmlKind::Trx)
}

pub fn parse_junit(path: &Path) -> Result<Vec<TestCaseResult>> {
    parse_xml_results(path, XmlKind::Junit)
}

#[derive(Clone, Copy, Debug)]
pub enum XmlKind {
    Trx,
    Junit,
}

pub fn parse_xml_results(path: &Path, kind: XmlKind) -> Result<Vec<TestCaseResult>> {
    let mut reader = Reader::from_file(path).with_context(|| format!("failed to read {}", path.display()))?;
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut results = Vec::new();
    let mut current_junit: Option<TestCaseResult> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => match (e.name().as_ref(), &kind) {
                (b"UnitTestResult", XmlKind::Trx) => {
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
                (b"testcase", XmlKind::Junit) => {
                    let name = attr(&e, b"name")?.unwrap_or_else(|| "<unknown>".to_string());
                    let class = attr(&e, b"classname")?;
                    let name = class.map(|class| format!("{class}::{name}")).unwrap_or(name);
                    current_junit = Some(TestCaseResult {
                        name,
                        outcome: Outcome::Passed,
                        message: None,
                    });
                    if e.is_empty()
                        && let Some(case) = current_junit.take()
                    {
                        results.push(case);
                    }
                }
                (b"failure" | b"error", XmlKind::Junit) => {
                    if let Some(case) = current_junit.as_mut() {
                        case.outcome = Outcome::Failed;
                        case.message = attr(&e, b"message")?;
                    }
                }
                (b"skipped", XmlKind::Junit) => {
                    if let Some(case) = current_junit.as_mut() {
                        case.outcome = Outcome::Skipped;
                        case.message = attr(&e, b"message")?;
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if matches!((&kind, e.name().as_ref()), (XmlKind::Junit, b"testcase")) => {
                if let Some(case) = current_junit.take() {
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

pub fn print_results(suite: &str, report_path: &Path, results: &[TestCaseResult]) -> Result<()> {
    let passed = results.iter().filter(|r| r.outcome == Outcome::Passed).count();
    let failed = results.iter().filter(|r| r.outcome == Outcome::Failed).count();
    let skipped = results.iter().filter(|r| r.outcome == Outcome::Skipped).count();

    println!(
        "{suite}: parsed {} test results from {}",
        results.len(),
        report_path.display()
    );
    for result in results {
        let status = match result.outcome {
            Outcome::Passed => "ok",
            Outcome::Failed => "FAILED",
            Outcome::Skipped => "ignored",
        };
        println!("{status:7} {}", result.name);
        if let Some(message) = &result.message
            && !message.is_empty()
        {
            println!("        {message}");
        }
    }
    println!("{suite}: {passed} passed; {failed} failed; {skipped} skipped");

    if failed > 0 {
        bail!("{suite}: {failed} native tests failed");
    }
    Ok(())
}
