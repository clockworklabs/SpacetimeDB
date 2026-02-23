use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};

const MODULE_PATH_FLAG_CUTOFF_COMMIT: &str = "4c962b9170c577b6e6c7afeecf05a60635fa1536";

pub struct PreparedChatWorkspace {
    pub worktree_dir: PathBuf,
    pub cli_path: PathBuf,
    pub client_path: Option<PathBuf>,
    pub module_dir: PathBuf,
    pub publish_path_flag: &'static str,
}

fn log_step(msg: &str) {
    eprintln!("[manual-upgrade] {msg}");
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

pub fn run_cmd(args: &[OsString], cwd: &Path) -> Result<Output> {
    let rendered = args
        .iter()
        .map(|s| s.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    log_step(&format!("run: (cd {}) {rendered}", cwd.display()));
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).current_dir(cwd);
    cmd.output()
        .with_context(|| format!("failed to execute {:?}", args.iter().collect::<Vec<_>>()))
}

pub fn run_cmd_ok(args: &[OsString], cwd: &Path) -> Result<String> {
    let out = run_cmd(args, cwd)?;
    if !out.status.success() {
        bail!(
            "command failed: {:?}\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_cmd_with_stdin(args: &[OsString], cwd: &Path, stdin_input: &str) -> Result<Output> {
    let rendered = args
        .iter()
        .map(|s| s.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    log_step(&format!("run (stdin): (cd {}) {rendered}", cwd.display()));

    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to execute {:?}", args.iter().collect::<Vec<_>>()))?;
    {
        let stdin = child.stdin.as_mut().context("missing child stdin")?;
        stdin.write_all(stdin_input.as_bytes())?;
    }
    child
        .wait_with_output()
        .with_context(|| format!("failed waiting for {:?}", args.iter().collect::<Vec<_>>()))
}

pub fn run_cmd_ok_with_stdin(args: &[OsString], cwd: &Path, stdin_input: &str) -> Result<String> {
    let out = run_cmd_with_stdin(args, cwd, stdin_input)?;
    if !out.status.success() {
        bail!(
            "command failed: {:?}\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stdout.trim().is_empty() {
        log_step(&format!("stdout:\n{}", stdout.trim()));
    }
    if !stderr.trim().is_empty() {
        log_step(&format!("stderr:\n{}", stderr.trim()));
    }
    Ok(stdout)
}

fn is_strictly_before_commit(repo: &Path, candidate_ref: &str, boundary_commit: &str) -> Result<bool> {
    let candidate = run_cmd_ok(
        &[
            OsString::from("git"),
            OsString::from("rev-parse"),
            OsString::from(candidate_ref),
        ],
        repo,
    )?
    .trim()
    .to_string();
    let boundary = run_cmd_ok(
        &[
            OsString::from("git"),
            OsString::from("rev-parse"),
            OsString::from(boundary_commit),
        ],
        repo,
    )?
    .trim()
    .to_string();

    if candidate == boundary {
        return Ok(false);
    }

    let out = run_cmd(
        &[
            OsString::from("git"),
            OsString::from("merge-base"),
            OsString::from("--is-ancestor"),
            OsString::from(&candidate),
            OsString::from(&boundary),
        ],
        repo,
    )?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!(
            "failed checking commit ancestry between {} and {}:\nstdout: {}\nstderr: {}",
            candidate,
            boundary,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

pub fn prepare_pinned_chat_workspace(
    workspace_dir: &Path,
    worktree_dir: &Path,
    git_ref: &str,
    build_client: bool,
) -> Result<PreparedChatWorkspace> {
    if !worktree_dir.exists() {
        let parent = worktree_dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("worktree path has no parent: {}", worktree_dir.display()))?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create worktree parent {}", parent.display()))?;
        run_cmd_ok(
            &[
                OsString::from("git"),
                OsString::from("worktree"),
                OsString::from("add"),
                worktree_dir.to_path_buf().into_os_string(),
                OsString::from(git_ref),
            ],
            workspace_dir,
        )?;
    } else {
        run_cmd_ok(
            &[
                OsString::from("git"),
                OsString::from("-C"),
                worktree_dir.to_path_buf().into_os_string(),
                OsString::from("checkout"),
                OsString::from("--force"),
                OsString::from(git_ref),
            ],
            workspace_dir,
        )?;
    }

    run_cmd_ok(
        &[
            OsString::from("cargo"),
            OsString::from("build"),
            OsString::from("--release"),
            OsString::from("-p"),
            OsString::from("spacetimedb-cli"),
            OsString::from("-p"),
            OsString::from("spacetimedb-standalone"),
        ],
        &worktree_dir,
    )?;

    let cli_path = worktree_dir
        .join("target")
        .join("release")
        .join(exe_name("spacetimedb-cli"));
    anyhow::ensure!(
        cli_path.exists(),
        "pinned CLI binary not found at {}",
        cli_path.display()
    );

    let publish_path_flag = if is_strictly_before_commit(workspace_dir, MODULE_PATH_FLAG_CUTOFF_COMMIT, git_ref)? {
        "--module-path"
    } else {
        "--project-path"
    };

    let client_path: Option<PathBuf>;
    if build_client {
        let chat_client_dir = worktree_dir.join("templates/chat-console-rs");
        run_cmd_ok(
            &[
                cli_path.clone().into_os_string(),
                OsString::from("generate"),
                OsString::from(publish_path_flag),
                OsString::from("spacetimedb/"),
                OsString::from("--out-dir"),
                OsString::from("src/module_bindings/"),
                OsString::from("--lang"),
                OsString::from("rust"),
            ],
            &chat_client_dir,
        )?;
        run_cmd_ok(&[OsString::from("cargo"), OsString::from("build")], &chat_client_dir)?;

        let the_client_path = chat_client_dir
            .join("target")
            .join("debug")
            .join(exe_name("rust-quickstart-chat"));
        anyhow::ensure!(
            the_client_path.exists(),
            "pinned chat client not found at {}",
            the_client_path.display()
        );
        client_path = Some(the_client_path);
    } else {
        client_path = None;
    }

    Ok(PreparedChatWorkspace {
        worktree_dir: worktree_dir.to_path_buf(),
        cli_path,
        client_path,
        module_dir: worktree_dir.join("templates/chat-console-rs/spacetimedb"),
        publish_path_flag,
    })
}
