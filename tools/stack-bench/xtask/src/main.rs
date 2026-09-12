//! stack-bench runner.
//!
//! Drives the cross-backend agentic benchmark via Harbor, so the whole workflow
//! is `cargo stack-bench …` instead of shell glue. The only shell that remains is
//! the per-task `solve.sh`/`test.sh`, which Harbor *requires* as its oracle/verifier
//! entry points (on Linux it discovers only `*.sh`); those are thin launchers.
//!
//!   cargo stack-bench build                 # inject the shared grader into tasks
//!   cargo stack-bench list                  # list backends
//!   cargo stack-bench oracle spacetimedb    # one backend, oracle (expect reward 1.0)
//!   cargo stack-bench agent convex --model anthropic/claude-opus-4-6
//!   cargo stack-bench all --model anthropic/claude-opus-4-6   # every backend + compare
//!
//! Extra args after `--` are forwarded to `harbor run`, e.g.
//!   cargo stack-bench oracle convex -- -n 1

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Parser)]
#[command(name = "stack-bench", about = "Run the stack-bench cross-backend agentic benchmark (Harbor)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inject the shared grader (_shared/harness) into each task's tests/harness.
    Build,
    /// List available backends (task variants under tasks/realtime-chat/).
    List,
    /// Run one backend with the oracle solution (sanity check; expect reward 1.0).
    Oracle {
        /// Backend, e.g. `spacetimedb` or `convex`.
        backend: String,
        /// Task family under tasks/ (e.g. `team-chat`, `realtime-chat`).
        #[arg(long, default_value = "team-chat")]
        task: String,
        /// Args forwarded verbatim to `harbor run` (after `--`).
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Run one backend with a real agent.
    Agent {
        /// Backend, e.g. `spacetimedb` or `convex`.
        backend: String,
        /// Task family under tasks/ (e.g. `team-chat`, `realtime-chat`).
        #[arg(long, default_value = "team-chat")]
        task: String,
        /// Harbor agent name.
        #[arg(long, default_value = "claude-code")]
        agent: String,
        /// Model, e.g. `anthropic/claude-opus-4-6` or `openrouter/anthropic/claude-opus-4-6`.
        #[arg(long)]
        model: Option<String>,
        /// Args forwarded verbatim to `harbor run` (after `--`).
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Run EVERY backend with the same agent+model and print a comparison table.
    /// This is the product view: rank backends for a fixed agent.
    All {
        /// Harbor agent name (default: oracle — the harness self-test).
        #[arg(long, default_value = "oracle")]
        agent: String,
        /// Model, e.g. `anthropic/claude-opus-4-6` (ignored by the oracle agent).
        #[arg(long)]
        model: Option<String>,
        /// Base directory for per-backend job results (default: <stack-bench>/jobs).
        #[arg(long)]
        jobs_dir: Option<PathBuf>,
        /// Args forwarded verbatim to each `harbor run` (after `--`).
        #[arg(last = true)]
        extra: Vec<String>,
    },
}

/// Benchmark root = this crate's parent dir (crate lives at <root>/xtask).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

fn tasks_dir() -> PathBuf {
    root().join("tasks")
}

/// All (task, backend) pairs: every tasks/<task>/<backend>/ with a task.toml.
fn variants() -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for task in fs::read_dir(tasks_dir()).context("read tasks/")? {
        let task = task?;
        if !task.path().is_dir() {
            continue;
        }
        let task_name = task.file_name().to_string_lossy().to_string();
        for backend in fs::read_dir(task.path())? {
            let backend = backend?;
            if backend.path().join("task.toml").exists() {
                out.push((task_name.clone(), backend.file_name().to_string_lossy().to_string()));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn task_path(task: &str, backend: &str) -> Result<PathBuf> {
    let p = tasks_dir().join(task).join(backend);
    if !p.join("task.toml").exists() {
        let avail = variants()?
            .iter()
            .map(|(t, b)| format!("{t}/{b}"))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("unknown task/backend '{task}/{backend}'. available: {avail}");
    }
    Ok(p)
}

/// Recursively copy `src` into `dst`, skipping build/vendor dirs.
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some("node_modules" | "dist" | ".git")) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| format!("copy {} -> {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Inject _shared/harness into every task's tests/harness (replaces build-task.sh).
/// Harbor tasks must be self-contained, so we keep one source of truth and copy it in.
fn inject_harness() -> Result<()> {
    let harness = root().join("_shared/harness");
    if !harness.is_dir() {
        bail!("missing shared harness at {}", harness.display());
    }
    let mut n = 0;
    for (task, backend) in variants()? {
        let tests = tasks_dir().join(&task).join(&backend).join("tests");
        if !tests.is_dir() {
            continue;
        }
        let dest = tests.join("harness");
        if dest.exists() {
            fs::remove_dir_all(&dest).with_context(|| format!("rm {}", dest.display()))?;
        }
        copy_dir(&harness, &dest)?;
        println!("injected harness -> tasks/{task}/{backend}/tests/harness");
        n += 1;
    }
    println!("done: injected harness into {n} task(s)");
    Ok(())
}

fn harbor_run(
    task: &Path,
    agent: &str,
    model: Option<&str>,
    out_dir: Option<&Path>,
    extra: &[String],
) -> Result<()> {
    let mut cmd = Command::new("harbor");
    cmd.arg("run").arg("-p").arg(task).arg("-a").arg(agent).arg("-y");
    if let Some(m) = model {
        cmd.arg("-m").arg(m);
    }
    if let Some(o) = out_dir {
        cmd.arg("-o").arg(o);
    }
    cmd.args(extra);
    eprintln!(
        "+ harbor run -p {} -a {agent}{}",
        task.display(),
        model.map(|m| format!(" -m {m}")).unwrap_or_default()
    );
    let status = cmd
        .status()
        .context("failed to spawn `harbor` — is it installed and on PATH? (`uv tool install harbor`)")?;
    if !status.success() {
        bail!("harbor exited with {status}");
    }
    Ok(())
}

/// One backend's run summary, parsed from Harbor's result.json.
struct Summary {
    reward: Option<f64>,
    errored: i64,
    in_tokens: Option<i64>,
    out_tokens: Option<i64>,
}

/// Find the most-recently-written result.json under `dir` and parse the reward + token stats.
fn read_latest_summary(dir: &Path) -> Result<Summary> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()) == Some("result.json") {
                let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                if newest.as_ref().is_none_or(|(t, _)| mtime >= *t) {
                    newest = Some((mtime, p));
                }
            }
        }
    }
    let (_, path) = newest.with_context(|| format!("no result.json found under {}", dir.display()))?;
    let v: Value = serde_json::from_str(&fs::read_to_string(&path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    // reward = mean of the (single) eval in stats.evals
    let reward = v["stats"]["evals"]
        .as_object()
        .and_then(|m| m.values().next())
        .and_then(|eval| eval["metrics"].get(0))
        .and_then(|m| m["mean"].as_f64());
    Ok(Summary {
        reward,
        errored: v["stats"]["n_errored_trials"].as_i64().unwrap_or(0),
        in_tokens: v["stats"]["n_input_tokens"].as_i64(),
        out_tokens: v["stats"]["n_output_tokens"].as_i64(),
    })
}

fn run_all(agent: &str, model: Option<&str>, jobs_dir: Option<PathBuf>, extra: &[String]) -> Result<()> {
    inject_harness()?;
    let base = jobs_dir.unwrap_or_else(|| root().join("jobs"));
    let mut rows: Vec<(String, Result<Summary>)> = Vec::new();
    for (task, backend) in variants()? {
        let path = task_path(&task, &backend)?;
        let out = base.join(&task).join(&backend);
        let summary = harbor_run(&path, agent, model, Some(&out), extra).and_then(|()| read_latest_summary(&out));
        rows.push((format!("{task}/{backend}"), summary));
    }
    print_comparison(agent, model, &rows);
    Ok(())
}

fn print_comparison(agent: &str, model: Option<&str>, rows: &[(String, Result<Summary>)]) {
    let tok = |t: Option<i64>| t.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
    println!("\n=== stack-bench comparison — agent={agent} model={} ===", model.unwrap_or("-"));
    println!("{:<24} {:>8} {:>7} {:>10} {:>10}", "backend", "reward", "errors", "in_tok", "out_tok");
    println!("{:-<24} {:->8} {:->7} {:->10} {:->10}", "", "", "", "", "");
    for (backend, res) in rows {
        match res {
            Ok(s) => println!(
                "{:<24} {:>8} {:>7} {:>10} {:>10}",
                backend,
                s.reward.map(|r| format!("{r:.3}")).unwrap_or_else(|| "?".to_string()),
                s.errored,
                tok(s.in_tokens),
                tok(s.out_tokens),
            ),
            Err(e) => println!("{backend:<24} {:>8}   ({e})", "ERR"),
        }
    }
    println!();
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Build => inject_harness()?,
        Cmd::List => {
            for (t, b) in variants()? {
                println!("{t}/{b}");
            }
        }
        Cmd::Oracle { backend, task, extra } => {
            let path = task_path(&task, &backend)?;
            inject_harness()?;
            harbor_run(&path, "oracle", None, None, &extra)?;
        }
        Cmd::Agent { backend, task, agent, model, extra } => {
            let path = task_path(&task, &backend)?;
            inject_harness()?;
            harbor_run(&path, &agent, model.as_deref(), None, &extra)?;
        }
        Cmd::All { agent, model, jobs_dir, extra } => {
            run_all(&agent, model.as_deref(), jobs_dir, &extra)?;
        }
    }
    Ok(())
}
