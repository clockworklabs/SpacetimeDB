//! The one-shot benchmark: a model builds an app from one prompt inside Cursor, following
//! the rules in `tools/llm-oneshot/.cursor/rules`. The app is generated and graded under
//! the gitignored `tools/llm-oneshot/apps/<app>/staging/`, then published.

use crate::process::Tools;
use crate::results_repo::ONESHOT;
use anyhow::{bail, Result};
use clap::Subcommand;
use std::path::{Component, Path, PathBuf};

mod summarize;

#[derive(Subcommand)]
pub enum Command {
    /// Publish a graded app from staging to spacetimedb-llm-oneshot-results.
    ///
    /// The app must contain GRADING_RESULTS.md. It is copied to
    /// apps/<app>/<lang>/<llm>/<backend>/<run>/ in the results repo, committed and pushed.
    Publish {
        /// apps/<app>/staging/<lang>/<llm>/<backend>/<run>
        app: PathBuf,
        /// The local results checkout (default: $LLM_ONESHOT_RESULTS_REPO, then
        /// ../spacetimedb-llm-oneshot-results next to this SpacetimeDB checkout).
        #[arg(long)]
        results_repo: Option<PathBuf>,
        /// Commit without pushing.
        #[arg(long)]
        no_push: bool,
    },
    /// Summarize every graded app in the results repo into docs/llms/oneshot-summary.md
    /// and docs/llms/oneshot-grades.json.
    Summarize {
        /// Only include this app (for example chat-app).
        #[arg(long)]
        app: Option<String>,
        /// Only include this model (for example opus-4-5).
        #[arg(long)]
        llm: Option<String>,
        /// The local results checkout (see `publish`).
        #[arg(long)]
        results_repo: Option<PathBuf>,
    },
}

pub fn run(command: Command) -> Result<()> {
    let tools = Tools::new();
    match command {
        Command::Publish {
            app,
            results_repo,
            no_push,
        } => {
            let relative = published_path(&app)?;
            if !app.join("GRADING_RESULTS.md").is_file() {
                bail!(
                    "{} has no GRADING_RESULTS.md; grade it before publishing",
                    app.display()
                );
            }
            ONESHOT.publish(&tools, results_repo.as_deref(), &app, &relative, !no_push)
        }
        Command::Summarize { app, llm, results_repo } => {
            let repo = ONESHOT.checkout(&tools, results_repo.as_deref())?;
            summarize::run(&repo.join("apps"), app.as_deref(), llm.as_deref())
        }
    }
}

/// Maps `.../apps/<app>/staging/<lang>/<llm>/<backend>/<run>` to
/// `apps/<app>/<lang>/<llm>/<backend>/<run>`.
fn published_path(app: &Path) -> Result<PathBuf> {
    let parts: Vec<String> = app
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let n = parts.len();
    if n < 7 || parts[n - 7] != "apps" || parts[n - 5] != "staging" {
        bail!(
            "expected apps/<app>/staging/<lang>/<llm>/<backend>/<run>, got {}",
            app.display()
        );
    }
    let mut out = PathBuf::from("apps");
    out.push(&parts[n - 6]);
    for part in &parts[n - 4..] {
        out.push(part);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_path_maps_to_published_path() {
        let staged = Path::new(
            "/x/tools/llm-oneshot/apps/chat-app/staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000",
        );
        assert_eq!(
            published_path(staged).unwrap(),
            Path::new("apps/chat-app/typescript/opus-4-5/spacetime/chat-app-20260108-120000")
        );
    }

    #[test]
    fn paths_outside_staging_are_rejected() {
        assert!(published_path(Path::new("apps/chat-app/typescript/opus-4-5/spacetime/run")).is_err());
        assert!(published_path(Path::new("apps/chat-app/staging/typescript/opus-4-5/spacetime")).is_err());
    }
}
