#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::process::Command;

#[derive(Parser)]
#[command(about = "Waits for a GitHub Actions workflow run to complete.")]
struct Cli {
    /// Repository containing the workflow run, in owner/repo form.
    #[arg(long)]
    repo: String,

    /// GitHub Actions workflow run ID.
    #[arg(long)]
    run_id: u64,

    /// Seconds to sleep between polls.
    #[arg(long, default_value_t = 30)]
    interval_seconds: u64,

    /// Maximum number of polls before timing out. Polls forever by default.
    #[arg(long)]
    max_attempts: Option<u64>,
}

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

fn main() -> Result<()> {
    let args = Cli::parse();

    println!(
        "Waiting for workflow result... https://github.com/{}/actions/runs/{}",
        args.repo, args.run_id
    );

    let mut attempts = 0;
    loop {
        attempts += 1;
        let run = get_workflow_run(&args.repo, args.run_id)?;
        if run.status == "completed" {
            let conclusion = run.conclusion.as_deref().unwrap_or("success");
            if conclusion == "success" {
                return Ok(());
            }
            bail!("workflow run {} completed with conclusion: {conclusion}", args.run_id);
        }

        if args.max_attempts.is_some_and(|max| attempts >= max) {
            bail!("timed out waiting for workflow run {} to complete", args.run_id)
        }

        std::thread::sleep(std::time::Duration::from_secs(args.interval_seconds));
    }
}
