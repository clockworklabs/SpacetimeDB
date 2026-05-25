#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use std::env;
use std::path::{Path, PathBuf};

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
