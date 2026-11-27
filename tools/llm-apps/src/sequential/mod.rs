//! The sequential-upgrade benchmark: headless Claude Code builds a chat app at level 1,
//! a person (or a grading session) grades it in the browser, Claude Code fixes the bugs,
//! and the app is upgraded one level at a time. Cost comes from Claude Code's
//! OpenTelemetry output.
//!
//! Run output is written under the gitignored `tools/llm-sequential-upgrade/<variant>/`:
//!
//! ```text
//! <variant>/<variant>-YYYYMMDD/
//!   <backend>/
//!     results/chat-app-<ts>/      the generated app, with level-N/ snapshots
//!     telemetry/<run-id>/         metadata.json, COST_REPORT.md, cost-summary.json
//!     inputs/                     the prompts and specs the run used
//!   BENCHMARK_REPORT.md
//! ```

use crate::paths;
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

mod benchmark;
mod cleanup;
mod grade;
mod publish;
mod report;
mod reset;
mod run;
mod telemetry;

#[derive(Subcommand)]
pub enum Command {
    /// Generate a new app at a level and deploy it (headless Claude Code).
    Generate {
        #[arg(long, value_enum, default_value_t = Backend::Spacetime)]
        backend: Backend,
        /// Feature level (default: 1, or 12 for --variant one-shot).
        #[arg(long)]
        level: Option<u32>,
        #[command(flatten)]
        options: RunOptions,
    },
    /// Add the next level's features to a graded app (headless Claude Code).
    Upgrade {
        app: PathBuf,
        /// The level to upgrade to.
        #[arg(long)]
        level: u32,
        /// Prompt with the full cumulative spec for the level instead of only the new feature.
        #[arg(long)]
        composed_prompt: bool,
        /// Fork the app's previous Claude Code session, for prompt-cache reuse.
        #[arg(long)]
        resume_session: bool,
        #[command(flatten)]
        options: RunOptions,
    },
    /// Fix the bugs described in an app's BUG_REPORT.md and redeploy (headless Claude Code).
    Fix {
        app: PathBuf,
        /// The level the app is at.
        #[arg(long, default_value_t = 1)]
        level: u32,
        #[command(flatten)]
        options: RunOptions,
    },
    /// Grade a deployed app in the browser with Claude Code and Chrome.
    Grade { app: PathBuf },
    /// Drive one backend through generate, grade and fix, then each upgrade in turn.
    Loop {
        #[arg(long, value_enum, default_value_t = Backend::Spacetime)]
        backend: Backend,
        /// The last level to reach.
        #[arg(long, default_value_t = 7)]
        level: u32,
        /// Fix iterations allowed per level before moving on.
        #[arg(long, default_value_t = 5)]
        max_fixes: u32,
        #[command(flatten)]
        options: RunOptions,
    },
    /// Run several loops in parallel, one per backend and run, then write reports.
    Benchmark {
        /// Loops per backend.
        #[arg(long, default_value_t = 3)]
        runs: u32,
        /// Only this backend (default: spacetime and postgres).
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        /// The last level each loop reaches.
        #[arg(long, default_value_t = 7)]
        level: u32,
        #[arg(long, default_value = "sequential-upgrade")]
        variant: String,
        #[arg(long, value_enum, default_value_t = Rules::Guided)]
        rules: Rules,
    },
    /// Give an app a clean database: a fresh SpacetimeDB module, or empty Postgres/MongoDB.
    Reset { app: PathBuf },
    /// Remove dependencies, build output and logs from finished apps.
    Cleanup {
        /// The apps to clean.
        apps: Vec<PathBuf>,
        /// Clean every app of every variant.
        #[arg(long, conflicts_with_all = ["apps", "variant"])]
        all: bool,
        /// Clean every app of this variant.
        #[arg(long, conflicts_with = "apps")]
        variant: Option<String>,
    },
    /// Turn the shared OpenTelemetry log into a run's COST_REPORT.md and cost-summary.json.
    ParseTelemetry {
        /// <run>/<backend>/telemetry/<run-id>
        run_dir: PathBuf,
        /// The OpenTelemetry log (default: <run-dir>/../logs.jsonl).
        #[arg(long)]
        logs_file: Option<PathBuf>,
        /// Ignore records after this time (default: the run's recorded end time).
        #[arg(long)]
        end_time: Option<String>,
        /// Also save the run's matching records to raw-telemetry.jsonl.
        #[arg(long)]
        extract_raw: bool,
    },
    /// Write BENCHMARK_REPORT.md for a run from its cost summaries and grades.
    Report {
        /// <variant>/<variant>-YYYYMMDD
        run: PathBuf,
    },
    /// Publish a graded run, or one backend of it, to spacetimedb-llm-sequential-upgrade-results.
    Publish {
        /// <variant>/<variant>-<id>, or <variant>/<variant>-<id>/<backend>
        run: PathBuf,
        /// The local results checkout (default: $LLM_SEQUENTIAL_UPGRADE_RESULTS_REPO, then
        /// ../spacetimedb-llm-sequential-upgrade-results next to this SpacetimeDB checkout).
        #[arg(long)]
        results_repo: Option<PathBuf>,
        /// Commit without pushing.
        #[arg(long)]
        no_push: bool,
    },
}

/// Options shared by every Claude Code run.
#[derive(Args, Clone)]
pub struct RunOptions {
    /// sequential-upgrade builds level by level; one-shot builds every feature at once.
    #[arg(long, default_value = "sequential-upgrade")]
    pub variant: String,
    /// How much guidance the generated app's CLAUDE.md gives.
    #[arg(long, value_enum, default_value_t = Rules::Guided)]
    pub rules: Rules,
    /// The model to pin, so every backend and level uses the same one.
    #[arg(long, env = "ANTHROPIC_MODEL", default_value = "claude-sonnet-4-6")]
    pub model: String,
    /// Offsets ports and database names so runs can proceed in parallel.
    #[arg(long, default_value_t = 0)]
    pub run_index: u32,
}

impl RunOptions {
    /// The same options as command-line arguments, for child runs.
    pub fn to_args(&self) -> Vec<String> {
        vec![
            "--variant".into(),
            self.variant.clone(),
            "--rules".into(),
            self.rules.to_string(),
            "--model".into(),
            self.model.clone(),
            "--run-index".into(),
            self.run_index.to_string(),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Backend {
    Spacetime,
    Postgres,
    Mongodb,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Backend::Spacetime => "spacetime",
            Backend::Postgres => "postgres",
            Backend::Mongodb => "mongodb",
        })
    }
}

impl Backend {
    fn parse(s: &str) -> Option<Backend> {
        Backend::from_str(s.trim(), true).ok()
    }

    /// The Vite port for this backend at run index 0. Each backend has its own range.
    pub fn base_vite_port(self) -> u32 {
        match self {
            Backend::Spacetime => 6173,
            Backend::Postgres => 6273,
            Backend::Mongodb => 6373,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Rules {
    /// Step-by-step phases, the SDK skills and code templates.
    Guided,
    /// The SDK skills only.
    Standard,
    /// Just the tech stack.
    Minimal,
}

impl fmt::Display for Rules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Rules::Guided => "guided",
            Rules::Standard => "standard",
            Rules::Minimal => "minimal",
        })
    }
}

/// Ports and database names for one run index.
pub struct Ports {
    pub vite: u32,
    pub express: u32,
}

impl Ports {
    pub fn new(backend: Backend, run_index: u32) -> Ports {
        Ports {
            vite: backend.base_vite_port() + run_index,
            express: 6001 + run_index,
        }
    }
}

/// The marker file `generate` writes so later steps know the app's backend. Postgres and
/// MongoDB apps both have a server/ directory, so the shape alone cannot tell them apart.
const BACKEND_MARKER: &str = ".benchmark-backend";

/// An app's backend, from its marker or, for apps older than the marker, its shape.
pub fn detect_backend(app: &Path) -> Option<Backend> {
    if let Ok(marker) = fs::read_to_string(app.join(BACKEND_MARKER)) {
        return Backend::parse(&marker);
    }
    if app.join("backend/spacetimedb").is_dir() {
        Some(Backend::Spacetime)
    } else if app.join("server").is_dir() {
        Some(Backend::Postgres)
    } else {
        None
    }
}

/// An existing app directory as an absolute path.
fn app_dir(app: &Path) -> Result<PathBuf> {
    fs::canonicalize(app).with_context(|| format!("app directory not found: {}", app.display()))
}

/// The file in `dir` for a level, named `NN_<description>.md`.
pub fn level_prompt(dir: &Path, level: u32) -> Option<PathBuf> {
    let prefix = format!("{level:02}_");
    let mut matches: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".md"))
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

/// The cumulative spec for a level.
pub fn composed_prompt(level: u32) -> Option<PathBuf> {
    level_prompt(&paths::chat_app_prompts_dir().join("composed"), level)
}

/// The generated apps of a variant: `<variant>/<run>/<backend>/results/chat-app-*`.
pub fn apps_in_variant(variant_dir: &Path) -> Vec<PathBuf> {
    let mut apps = Vec::new();
    for run in subdirs(variant_dir) {
        for backend in subdirs(&run) {
            apps.extend(subdirs(&backend.join("results")).into_iter().filter(|a| {
                a.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("chat-app-"))
            }));
        }
    }
    apps
}

/// The subdirectories of `dir` in name order, or none if it is missing.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Generate {
            backend,
            level,
            options,
        } => {
            let level = level.unwrap_or(if options.variant == "one-shot" { 12 } else { 1 });
            run::run(run::Mode::Generate { backend }, level, &options)
        }
        Command::Upgrade {
            app,
            level,
            composed_prompt,
            resume_session,
            options,
        } => {
            if options.variant == "one-shot" {
                bail!(
                    "upgrade is not meaningful with --variant one-shot, which generates every feature in one session"
                );
            }
            if level < 2 {
                bail!("--level must be at least 2 for an upgrade");
            }
            let mode = run::Mode::Upgrade {
                app: app_dir(&app)?,
                composed_prompt,
                resume_session,
            };
            run::run(mode, level, &options)
        }
        Command::Fix { app, level, options } => run::run(run::Mode::Fix { app: app_dir(&app)? }, level, &options),
        Command::Grade { app } => grade::run(&app_dir(&app)?),
        Command::Loop {
            backend,
            level,
            max_fixes,
            options,
        } => benchmark::run_loop(backend, level, max_fixes, &options),
        Command::Benchmark {
            runs,
            backend,
            level,
            variant,
            rules,
        } => benchmark::run_benchmark(runs, backend, level, &variant, rules),
        Command::Reset { app } => reset::run(&app_dir(&app)?),
        Command::Cleanup { apps, all, variant } => cleanup::run(&apps, all, variant.as_deref()),
        Command::ParseTelemetry {
            run_dir,
            logs_file,
            end_time,
            extract_raw,
        } => telemetry::run(&run_dir, logs_file.as_deref(), end_time.as_deref(), extract_raw).map(drop),
        Command::Report { run } => report::run(&run),
        Command::Publish {
            run,
            results_repo,
            no_push,
        } => publish::run(&run, results_repo.as_deref(), !no_push),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_comes_from_the_marker_then_the_shape() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path();
        assert_eq!(detect_backend(app), None);
        fs::create_dir_all(app.join("server")).unwrap();
        assert_eq!(detect_backend(app), Some(Backend::Postgres));
        fs::write(app.join(BACKEND_MARKER), "mongodb\n").unwrap();
        assert_eq!(detect_backend(app), Some(Backend::Mongodb));
    }

    #[test]
    fn level_prompt_picks_the_numbered_file() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["01_basic.md", "02_scheduled.md", "12_full.md", "notes.md"] {
            fs::write(dir.path().join(name), "").unwrap();
        }
        assert_eq!(level_prompt(dir.path(), 2).unwrap(), dir.path().join("02_scheduled.md"));
        assert_eq!(level_prompt(dir.path(), 12).unwrap(), dir.path().join("12_full.md"));
        assert_eq!(level_prompt(dir.path(), 3), None);
    }

    #[test]
    fn real_prompts_exist_for_every_level() {
        for level in 1..=12 {
            assert!(composed_prompt(level).is_some(), "no composed prompt for level {level}");
        }
    }
}
