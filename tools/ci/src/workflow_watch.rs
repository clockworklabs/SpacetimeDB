use anyhow::{bail, Context, Result};
use duct::cmd;
use serde::Deserialize;

#[derive(Deserialize)]
struct WorkflowRunView {
    status: String,
    conclusion: Option<String>,
}

fn get_workflow_run(repo: &str, run_id: u64) -> Result<WorkflowRunView> {
    let raw = cmd!("gh", "api", format!("repos/{repo}/actions/runs/{run_id}"))
        .read()
        .with_context(|| format!("failed to read workflow run {run_id} in {repo}"))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse workflow run {run_id} in {repo}"))
}

pub(crate) fn watch_workflow_run(
    repo: &str,
    run_id: u64,
    interval_seconds: u64,
    max_attempts: Option<u64>,
) -> Result<()> {
    println!("Waiting for workflow result... https://github.com/{repo}/actions/runs/{run_id}");

    let mut attempts = 0;
    loop {
        attempts += 1;
        let run = get_workflow_run(repo, run_id)?;
        if run.status == "completed" {
            let conclusion = run.conclusion.as_deref().unwrap_or("success");
            if conclusion == "success" {
                return Ok(());
            }
            bail!("workflow run {run_id} completed with conclusion: {conclusion}");
        }

        if max_attempts.is_some_and(|max| attempts >= max) {
            bail!("timed out waiting for workflow run {run_id} to complete")
        }

        std::thread::sleep(std::time::Duration::from_secs(interval_seconds));
    }
}
