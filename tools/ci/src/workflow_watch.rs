use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct WorkflowRunView {
    status: String,
    conclusion: Option<String>,
}

fn get_workflow_run(repo: &str, run_id: u64) -> Result<WorkflowRunView> {
    let path = format!("repos/{repo}/actions/runs/{run_id}");
    let output = Command::new("gh")
        .args(["api", "--include", &path])
        .output()
        .with_context(|| format!("failed to run gh api for workflow run {run_id} in {repo}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        eprintln!("gh api failed while reading workflow run {run_id} in {repo}:");
        if !stdout.is_empty() {
            eprintln!("--- gh api stdout ---\n{stdout}");
        }
        if !stderr.is_empty() {
            eprintln!("--- gh api stderr ---\n{stderr}");
        }
        bail!("gh api {path} exited with {}", output.status);
    }

    let body = response_body(&stdout);
    serde_json::from_str(body).with_context(|| format!("failed to parse workflow run {run_id} in {repo}"))
}

fn response_body(response: &str) -> &str {
    response
        .rsplit_once("\r\n\r\n")
        .or_else(|| response.rsplit_once("\n\n"))
        .map_or(response, |(_, body)| body)
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
