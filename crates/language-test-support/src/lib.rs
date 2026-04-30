#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

#[derive(Clone, Debug, Default)]
pub struct HarnessArgs {
    pub filter: Option<String>,
    pub list: bool,
    pub passthrough: Vec<String>,
}

impl HarnessArgs {
    pub fn parse() -> Self {
        let mut args = env::args().skip(1).peekable();
        let mut parsed = HarnessArgs::default();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--filter" => {
                    parsed.filter = args.next();
                }
                "--list" | "--list-tests" => {
                    parsed.list = true;
                }
                "--" => {
                    parsed.passthrough.extend(args);
                    break;
                }
                other if other.starts_with("--filter=") => {
                    parsed.filter = Some(other.trim_start_matches("--filter=").to_string());
                }
                other if other.starts_with("--") => {
                    parsed.passthrough.push(other.to_string());
                }
                other => {
                    // Match libtest's common shorthand: `cargo test foo`.
                    if parsed.filter.is_none() {
                        parsed.filter = Some(other.to_string());
                    } else {
                        parsed.passthrough.push(other.to_string());
                    }
                }
            }
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

pub fn artifact_dir(suite: &str) -> Result<PathBuf> {
    let dir = target_dir().join("language-tests").join(suite);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir)
}

pub fn require_tool(tool: &str) -> Result<()> {
    if find_on_path(tool).is_some() {
        Ok(())
    } else {
        bail!("required tool `{tool}` was not found on PATH")
    }
}

fn find_on_path(tool: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let candidates = env::split_paths(&path).flat_map(|dir| executable_candidates(&dir, tool));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn executable_candidates(dir: &Path, tool: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut candidates = vec![dir.join(tool)];
        if Path::new(tool).extension().is_none() {
            let pathext = env::var_os("PATHEXT")
                .map(|v| {
                    env::split_paths(&v)
                        .filter_map(|p| p.as_os_str().to_str().map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![".exe".to_string(), ".bat".to_string(), ".cmd".to_string()]);
            candidates.extend(pathext.into_iter().map(|ext| dir.join(format!("{tool}{ext}"))));
        }
        candidates
    }

    #[cfg(not(windows))]
    {
        vec![dir.join(tool)]
    }
}

pub fn run_command(program: &str, args: &[String], cwd: &Path) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line(program, args), cwd.display()))?;

    if !output.status.success() {
        bail!(
            "command failed in {}:\n  {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            cwd.display(),
            shell_line(program, args),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output)
}

pub fn run_command_forward(program: &str, args: &[String], cwd: &Path) -> Result<()> {
    let output = run_command(program, args, cwd)?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

pub fn run_command_env(program: &str, args: &[String], cwd: &Path, envs: &[(&str, String)]) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(envs.iter().map(|(k, v)| (OsStr::new(k), OsStr::new(v))))
        .output()
        .with_context(|| format!("failed to spawn `{}` in {}", shell_line(program, args), cwd.display()))?;

    if !output.status.success() {
        bail!(
            "command failed in {}:\n  {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            cwd.display(),
            shell_line(program, args),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output)
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

enum XmlKind {
    Trx,
    Junit,
}

fn parse_xml_results(path: &Path, kind: XmlKind) -> Result<Vec<TestCaseResult>> {
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
