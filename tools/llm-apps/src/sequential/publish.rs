//! `publish`: copies a graded run, or one backend of it, to
//! spacetimedb-llm-sequential-upgrade-results at the same path, commits and pushes.

use super::{apps_in_variant, subdirs};
use crate::paths;
use crate::process::Tools;
use crate::results_repo::SEQUENTIAL;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(run: &Path, results_repo: Option<&Path>, push: bool) -> Result<()> {
    let run = fs::canonicalize(run).with_context(|| format!("{} does not exist", run.display()))?;
    let tool_dir = fs::canonicalize(paths::sequential_dir())?;
    let relative = published_path(&tool_dir, &run)?;

    let apps = apps_of(&run, relative.components().count() == 3);
    if apps.is_empty() {
        bail!(
            "no generated apps (<backend>/results/chat-app-*) under {}",
            run.display()
        );
    }
    let ungraded: Vec<String> = apps
        .iter()
        .filter(|a| !a.join("GRADING_RESULTS.md").is_file())
        .map(|a| format!("  {}", a.display()))
        .collect();
    if !ungraded.is_empty() {
        bail!(
            "grade these apps before publishing (no GRADING_RESULTS.md):\n{}",
            ungraded.join("\n")
        );
    }
    SEQUENTIAL.publish(&Tools::new(), results_repo, &run, &relative, push)
}

/// `<tool>/<variant>/<variant>-<id>[/<backend>]` → `<variant>/<variant>-<id>[/<backend>]`.
fn published_path(tool_dir: &Path, run: &Path) -> Result<PathBuf> {
    let shape = || {
        format!(
            "expected {}/<variant>/<variant>-<id>[/<backend>], got {}",
            tool_dir.display(),
            run.display()
        )
    };
    let relative = run.strip_prefix(tool_dir).map_err(|_| anyhow::anyhow!(shape()))?;
    let parts: Vec<String> = relative.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let valid = matches!(parts.len(), 2 | 3) && parts[1].starts_with(&format!("{}-", parts[0]));
    if !valid {
        bail!(shape());
    }
    Ok(relative.to_path_buf())
}

/// The generated apps of a run, or of one backend of it.
fn apps_of(dir: &Path, is_backend: bool) -> Vec<PathBuf> {
    if is_backend {
        subdirs(&dir.join("results"))
            .into_iter()
            .filter(|a| {
                a.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("chat-app-"))
            })
            .collect()
    } else {
        // apps_in_variant expects the variant directory, one level above a run.
        apps_in_variant(dir.parent().unwrap_or(dir))
            .into_iter()
            .filter(|a| a.starts_with(dir))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_backends_inside_a_variant_are_publishable() {
        let tool = Path::new("/t");
        assert_eq!(
            published_path(tool, Path::new("/t/sequential-upgrade/sequential-upgrade-20260616")).unwrap(),
            Path::new("sequential-upgrade/sequential-upgrade-20260616")
        );
        assert_eq!(
            published_path(tool, Path::new("/t/one-shot/one-shot-20260616-2/mongodb")).unwrap(),
            Path::new("one-shot/one-shot-20260616-2/mongodb")
        );
        for bad in [
            "/elsewhere/sequential-upgrade/sequential-upgrade-1",
            "/t/sequential-upgrade",
            "/t/backends/spacetime.md",
        ] {
            assert!(published_path(tool, Path::new(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn apps_of_a_run_and_of_one_backend() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("su/su-1");
        for b in ["spacetime", "mongodb"] {
            fs::create_dir_all(run.join(b).join("results/chat-app-1")).unwrap();
        }
        fs::create_dir_all(dir.path().join("su/su-2/postgres/results/chat-app-1")).unwrap();
        assert_eq!(apps_of(&run, false).len(), 2);
        assert_eq!(
            apps_of(&run.join("mongodb"), true),
            vec![run.join("mongodb/results/chat-app-1")]
        );
    }
}
