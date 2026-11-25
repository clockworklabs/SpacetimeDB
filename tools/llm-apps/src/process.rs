//! Finding and running the external tools the benchmarks drive: Claude Code, Docker,
//! the SpacetimeDB CLI, git and npx.

use anyhow::{bail, Context, Result};
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// A `PATH` for child processes, extended with the places Windows installs put the
/// Claude Code and SpacetimeDB CLIs, which are often missing from `PATH`.
#[derive(Clone)]
pub struct Tools {
    path: OsString,
}

impl Tools {
    pub fn new() -> Self {
        let mut dirs: Vec<PathBuf> = env::var_os("PATH")
            .map(|p| env::split_paths(&p).collect())
            .unwrap_or_default();
        if let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
            let home = PathBuf::from(home);
            dirs.push(home.join("AppData/Local/SpacetimeDB"));
            let appdata = env::var_os("APPDATA").map_or_else(|| home.join("AppData/Roaming"), PathBuf::from);
            if let Some(latest) = latest_version_dir(&appdata.join("Claude/claude-code")) {
                dirs.push(latest);
            }
        }
        let path = env::join_paths(dirs.into_iter().filter(|d| d.is_dir())).unwrap_or_default();
        Self { path }
    }

    /// A command for `program` that sees the extended `PATH`.
    pub fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(self.resolve(program).unwrap_or_else(|| PathBuf::from(program)));
        cmd.env("PATH", &self.path);
        cmd
    }

    /// Whether `program` is on the extended `PATH`.
    pub fn has(&self, program: &str) -> bool {
        self.resolve(program).is_some()
    }

    fn resolve(&self, program: &str) -> Option<PathBuf> {
        let exts: &[&str] = if cfg!(windows) {
            &["", ".exe", ".cmd", ".bat"]
        } else {
            &[""]
        };
        env::split_paths(&self.path).find_map(|dir| {
            exts.iter()
                .map(|ext| dir.join(format!("{program}{ext}")))
                .find(|candidate| candidate.is_file())
        })
    }

    /// Whether `program args...` runs and exits successfully, with its output discarded.
    pub fn succeeds(&self, program: &str, args: &[&str]) -> bool {
        self.command(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Runs `cmd` and returns its combined stdout and stderr, whatever its exit status.
    pub fn output(cmd: &mut Command) -> Result<String> {
        let out = cmd.output().with_context(|| format!("failed to run {cmd:?}"))?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok(text)
    }

    /// Finds the Claude Code CLI: `claude` on `PATH`, else through `npx`.
    pub fn claude(&self) -> Result<Claude> {
        if self.has("claude") {
            return Ok(Claude {
                program: "claude".into(),
                prefix: vec![],
            });
        }
        if self.succeeds("npx", &["@anthropic-ai/claude-code", "--version"]) {
            return Ok(Claude {
                program: "npx".into(),
                prefix: vec!["@anthropic-ai/claude-code".into()],
            });
        }
        bail!("Claude Code CLI not found (tried claude and npx). Install it with: npm install -g @anthropic-ai/claude-code")
    }
}

/// The newest `<dir>/<version>/` subdirectory, comparing versions numerically.
fn latest_version_dir(dir: &Path) -> Option<PathBuf> {
    let key = |p: &PathBuf| -> Vec<u64> {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|n| n.parse().ok())
            .collect()
    };
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .max_by_key(key)
}

/// How to invoke Claude Code.
pub struct Claude {
    program: String,
    prefix: Vec<String>,
}

impl Claude {
    pub fn command(&self, tools: &Tools) -> Command {
        let mut cmd = tools.command(&self.program);
        cmd.args(&self.prefix);
        cmd
    }

    pub fn describe(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.prefix.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Runs `cmd`, copying its stdout and stderr both to this process's and to `log`.
pub fn run_tee(cmd: &mut Command, log: &Path) -> Result<ExitStatus> {
    let file = Arc::new(Mutex::new(
        File::create(log).with_context(|| format!("failed to create {}", log.display()))?,
    ));
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {cmd:?}"))?;
    let copy = |mut from: Box<dyn Read + Send>, to_stderr: bool| {
        let file = Arc::clone(&file);
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = from.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = if to_stderr {
                    io::stderr().write_all(&buf[..n])
                } else {
                    io::stdout().write_all(&buf[..n])
                };
                let _ = file.lock().unwrap().write_all(&buf[..n]);
            }
        })
    };
    let out = copy(Box::new(child.stdout.take().unwrap()), false);
    let err = copy(Box::new(child.stderr.take().unwrap()), true);
    let status = child.wait()?;
    let _ = out.join();
    let _ = err.join();
    Ok(status)
}

/// Fails with a readable message when a step exits unsuccessfully.
pub fn check(status: ExitStatus, what: &str) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!("{what} failed ({status})")
    }
}

/// The last `n` lines of `text`, for commands whose full output is noise.
pub fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}
