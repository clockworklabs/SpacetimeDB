//! `loop` drives one backend through every step of the benchmark; `benchmark` runs
//! several loops in parallel and then writes each run's report.
//!
//! Each step runs as a child `cargo llm-apps` process, so its output can be saved to a
//! log next to the app as well as shown.

use super::{composed_prompt, report, subdirs, Backend, Ports, Rules, RunOptions};
use crate::paths;
use crate::process::{check, run_tee, Tools};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

/// This binary, to run a step as a child process.
fn this(args: &[String]) -> Result<Command> {
    let mut cmd = Command::new(std::env::current_exe().context("cannot locate this binary")?);
    cmd.arg("sequential").args(args);
    Ok(cmd)
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// Only one grading session runs at a time, because they share one Chrome. A directory
/// is the lock because creating one is atomic.
struct GradeLock(PathBuf);

impl GradeLock {
    fn acquire(path: PathBuf, prefix: &str) -> GradeLock {
        if fs::create_dir(&path).is_err() {
            println!(
                "{prefix} Waiting for the grading lock (remove {} if no grading session is running)...",
                path.display()
            );
            while fs::create_dir(&path).is_err() {
                thread::sleep(Duration::from_secs(5));
            }
            println!("{prefix} Grading lock acquired");
        }
        GradeLock(path)
    }
}

impl Drop for GradeLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

pub fn run_loop(backend: Backend, level: u32, max_fixes: u32, options: &RunOptions) -> Result<()> {
    let tool_dir = paths::sequential_dir();
    let prefix = format!("[run-{}/{backend}]", options.run_index);
    println!("═══════════════════════════════════════════");
    println!("{prefix} Benchmark loop");
    println!("  Backend:   {backend}");
    println!("  Variant:   {}", options.variant);
    println!("  Level:     {level}");
    println!("  Rules:     {}", options.rules);
    println!("  Run index: {}", options.run_index);
    println!("  Max fixes: {max_fixes}");
    println!("═══════════════════════════════════════════");

    let common = options.to_args();
    let one_shot = options.variant == "one-shot";
    let first_level = if one_shot { level } else { 1 };

    let mut generate = args(&[
        "generate",
        "--backend",
        &backend.to_string(),
        "--level",
        &first_level.to_string(),
    ]);
    generate.extend(common.iter().cloned());
    println!("{prefix} Generating level {first_level}...");
    check(this(&generate)?.status()?, "generate")?;
    let app = newest_app(&tool_dir.join(&options.variant), backend).context("could not find the generated app")?;
    println!("{prefix} App dir: {}", app.display());

    // Grade, then fix and re-grade until every feature passes or the fixes run out.
    let settle = |at_level: u32| -> Result<()> {
        for iteration in 1..=max_fixes + 1 {
            {
                let _lock = GradeLock::acquire(tool_dir.join(".grade-lock"), &prefix);
                println!("{prefix} Grading at level {at_level}...");
                let log = app.join(format!("grade-output-level{at_level}.log"));
                check(
                    run_tee(&mut this(&args(&["grade", &app.to_string_lossy()]))?, &log)?,
                    "grade",
                )?;
            }
            if !app.join("BUG_REPORT.md").is_file() {
                println!("{prefix} All features passed at level {at_level}");
                return Ok(());
            }
            if iteration > max_fixes {
                println!("{prefix} Max fixes ({max_fixes}) at level {at_level} — moving on");
                return Ok(());
            }
            println!("{prefix} Bugs found — fix iteration {iteration}...");
            let mut fix = args(&["fix", &app.to_string_lossy(), "--level", &at_level.to_string()]);
            fix.extend(common.iter().cloned());
            check(
                run_tee(
                    &mut this(&fix)?,
                    &app.join(format!("fix-output-level{at_level}-iter{iteration}.log")),
                )?,
                "fix",
            )?;
        }
        Ok(())
    };

    settle(first_level)?;
    if !one_shot {
        for next in 2..=level {
            if composed_prompt(next).is_none() {
                println!("{prefix} No prompt for level {next} — stopping");
                break;
            }
            println!("{prefix} === Upgrading to level {next} ===");
            let mut upgrade = args(&[
                "upgrade",
                &app.to_string_lossy(),
                "--level",
                &next.to_string(),
                "--resume-session",
            ]);
            upgrade.extend(common.iter().cloned());
            check(this(&upgrade)?.status()?, "upgrade")?;
            // Grading covers every feature, so earlier levels are regression-tested too.
            settle(next)?;
        }
    }

    println!("\n═══════════════════════════════════════════");
    println!("{prefix} Benchmark loop complete");
    println!("  App dir: {}", app.display());
    println!("  Variant: {}", options.variant);
    println!("  Backend: {backend}");
    println!("═══════════════════════════════════════════\n");
    println!(
        "When done grading, clean up with: cargo llm-apps sequential cleanup {}",
        app.display()
    );
    Ok(())
}

/// The most recently created app for `backend` in a variant.
fn newest_app(variant_dir: &Path, backend: Backend) -> Option<PathBuf> {
    super::apps_in_variant(variant_dir)
        .into_iter()
        .filter(|a| {
            a.parent()
                .and_then(Path::parent)
                .is_some_and(|b| b.ends_with(backend.to_string()))
        })
        .filter_map(|a| Some((fs::metadata(&a).and_then(|m| m.modified()).ok()?, a)))
        .max()
        .map(|(_, a)| a)
}

struct Instance {
    key: String,
    run_index: u32,
    backend: Backend,
    child: Child,
    log: PathBuf,
    done: bool,
}

pub fn run_benchmark(runs: u32, backend: Option<Backend>, level: u32, variant: &str, rules: Rules) -> Result<()> {
    let tool_dir = paths::sequential_dir();
    let backends = backend.map_or_else(|| vec![Backend::Spacetime, Backend::Postgres], |b| vec![b]);
    let total = runs as usize * backends.len();

    println!("═══════════════════════════════════════════════════");
    println!("  Sequential Upgrade Benchmark");
    println!("═══════════════════════════════════════════════════");
    println!("  Variant:   {variant}");
    println!("  Rules:     {rules}");
    println!("  Level:     {level}");
    println!(
        "  Backends:  {}",
        backends.iter().map(Backend::to_string).collect::<Vec<_>>().join(" ")
    );
    println!("  Runs:      {runs} per backend");
    println!("  Total:     {total} parallel instances\n");

    let tools = Tools::new();
    tools.claude()?;

    let status_path = tool_dir.join("benchmark-status.json");
    let mut status = Map::new();
    let set_status = |status: &mut Map<String, Value>, i: &Instance, state: &str, detail: String| -> Result<()> {
        status.insert(
            i.key.clone(),
            json!({
                "runIndex": i.run_index,
                "backend": i.backend.to_string(),
                "status": state,
                "detail": detail,
                "updatedAt": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            }),
        );
        fs::write(&status_path, serde_json::to_string_pretty(&status)?)?;
        Ok(())
    };

    let mut instances = Vec::new();
    for run in 1..=runs {
        for &backend in &backends {
            let run_index = instances.len() as u32;
            let key = format!("run-{run_index}-{backend}");
            let log = tool_dir.join(format!("benchmark-{key}.log"));
            let ports = Ports::new(backend, run_index);
            println!(
                "[Run {run_index}] {backend} (run {run}/{runs}): Vite {}, Express {} → {}",
                ports.vite,
                ports.express,
                log.display()
            );
            let out = File::create(&log)?;
            let options = RunOptions {
                variant: variant.to_owned(),
                rules,
                model: std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".into()),
                run_index,
            };
            let mut loop_args = args(&["loop", "--backend", &backend.to_string(), "--level", &level.to_string()]);
            loop_args.extend(options.to_args());
            let child = this(&loop_args)?.stdout(out.try_clone()?).stderr(out).spawn()?;
            let instance = Instance {
                key,
                run_index,
                backend,
                child,
                log,
                done: false,
            };
            set_status(&mut status, &instance, "running", variant.to_owned())?;
            instances.push(instance);
        }
    }

    println!("\nAll {total} instances launched.");
    println!("  Status: {}", status_path.display());
    println!("  Logs:   {}/benchmark-run-*.log\n", tool_dir.display());
    println!("Waiting for all runs to complete...");

    let mut failures = 0;
    while instances.iter().any(|i| !i.done) {
        for i in instances.iter_mut().filter(|i| !i.done) {
            if let Some(exit) = i.child.try_wait()? {
                i.done = true;
                let (state, label) = if exit.success() {
                    ("completed", "completed successfully")
                } else {
                    ("failed", "FAILED")
                };
                if !exit.success() {
                    failures += 1;
                }
                println!(
                    "[Run {}] {} {label} ({exit}); log: {}",
                    i.run_index,
                    i.backend,
                    i.log.display()
                );
                set_status(&mut status, i, state, exit.to_string())?;
            }
        }
        thread::sleep(Duration::from_secs(5));
    }

    println!("\n═══════════════════════════════════════════════════");
    println!("  Benchmark Complete");
    println!("  Successful: {}/{total}", total - failures);
    if failures > 0 {
        println!("  Failed: {failures}");
    }
    println!("═══════════════════════════════════════════════════\n");

    println!("Generating reports for each run...");
    let variant_dir = tool_dir.join(variant);
    for run in subdirs(&variant_dir) {
        if subdirs(&run).iter().any(|b| b.join("telemetry").is_dir()) {
            match report::run(&run) {
                Ok(()) => {}
                Err(err) => println!("  WARNING: Report generation failed for {}: {err:#}", run.display()),
            }
        }
    }
    if failures > 0 {
        bail!("{failures} of {total} runs failed");
    }
    Ok(())
}
