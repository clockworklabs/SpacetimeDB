//! `grade`: a Claude Code session that tests the deployed app in Chrome, scores each
//! feature and writes GRADING_RESULTS.md, plus BUG_REPORT.md when something fails.

use super::{detect_backend, subdirs, Backend};
use crate::paths;
use crate::process::Tools;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(app: &Path) -> Result<()> {
    let tools = Tools::new();
    let claude = tools.claude()?;
    let backend = detect_backend(app);
    let port = vite_port(app, backend);

    println!("=== Sequential Upgrade: Grade ===");
    println!("  App dir: {}\n", app.display());
    println!("This launches a Claude Code session with Chrome MCP.");
    println!("It will test the deployed app, write bug reports, and grade features.\n");
    let backend = backend.map_or_else(|| "unknown".to_owned(), |b| b.to_string());
    println!("  Backend:  {backend} (port {port})");

    let prompt = format!(
        "Grade the sequential upgrade app at: {app}

Backend: {backend}

Follow CLAUDE.md Phases 6-8:
1. Open http://localhost:{port} in Chrome and verify the app loads
2. Test each feature using the test plans in test-plans/feature-*.md
3. Score each feature 0-3 based on browser observations
4. If any features score < 3, write a BUG_REPORT.md in the app directory with:
   - Which features failed and why
   - Exact error messages or broken behaviors observed
   - Console errors from read_console_messages
5. Write GRADING_RESULTS.md with scores
6. Write/update ITERATION_LOG.md with this test iteration

After grading, if there are bugs, tell the user to run:
  cargo llm-apps sequential fix {app}",
        app = app.display()
    );
    // Run from the tool directory so the session reads its grading CLAUDE.md.
    let status = claude
        .command(&tools)
        .current_dir(paths::sequential_dir())
        .args(["-p", &prompt])
        .status()
        .context("failed to start Claude Code")?;
    crate::process::check(status, "the grading session")
}

/// The Vite port the app was deployed on: what its newest run recorded, else the
/// backend's run-0 port.
fn vite_port(app: &Path, backend: Option<Backend>) -> u32 {
    let default = backend.unwrap_or(Backend::Spacetime).base_vite_port();
    // <run>/<backend>/results/<app> → <run>/<backend>/telemetry/*/metadata.json
    let Some(telemetry) = app.parent().and_then(Path::parent).map(|b| b.join("telemetry")) else {
        return default;
    };
    subdirs(&telemetry)
        .into_iter()
        .filter_map(|dir| {
            let path = dir.join("metadata.json");
            let modified = fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            Some((modified, path))
        })
        .max()
        .and_then(|(_, path)| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|meta| meta["vitePort"].as_u64())
        .map_or(default, |p| p as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_comes_from_the_newest_run_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("postgres/results/chat-app-1");
        fs::create_dir_all(&app).unwrap();
        assert_eq!(vite_port(&app, Some(Backend::Postgres)), 6273);
        let t = dir.path().join("postgres/telemetry/postgres-level1-x");
        fs::create_dir_all(&t).unwrap();
        fs::write(t.join("metadata.json"), r#"{"vitePort": 6275}"#).unwrap();
        assert_eq!(vite_port(&app, Some(Backend::Postgres)), 6275);
    }
}
