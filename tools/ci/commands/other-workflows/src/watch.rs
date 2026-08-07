use anyhow::{bail, Context, Result};
use duct::cmd;
use serde::Deserialize;
use std::time::Duration;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Repository containing the workflow run, in owner/repo form.
    #[arg(long)]
    repo: String,

    /// GitHub Actions workflow run ID.
    #[arg(long)]
    run_id: u64,

    /// Optional URL printed while waiting for the run.
    #[arg(long)]
    run_url: Option<String>,

    /// Seconds to sleep between polls.
    #[arg(long, default_value_t = 30)]
    interval_seconds: u64,

    /// Maximum number of polls before timing out.
    #[arg(long, default_value_t = 240)]
    max_attempts: u64,
}

#[derive(Deserialize)]
struct WorkflowRun {
    status: String,
    conclusion: Option<String>,
    url: Option<String>,
    jobs: Vec<WorkflowJob>,
}

#[derive(Deserialize)]
struct WorkflowJob {
    name: String,
    status: String,
    conclusion: Option<String>,
}

fn get_workflow_run(repo: &str, run_id: u64) -> Result<WorkflowRun> {
    let raw = cmd!(
        "gh",
        "run",
        "view",
        run_id.to_string(),
        "--repo",
        repo,
        "--json",
        "status,conclusion,url,jobs",
    )
    .read()
    .with_context(|| format!("failed to read workflow run {run_id} in {repo}"))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse workflow run {run_id} in {repo}"))
}

fn print_job_summary(run: &WorkflowRun) {
    println!("Job summary:");
    for job in &run.jobs {
        let result = job.conclusion.as_deref().unwrap_or(&job.status);
        println!("  {result:>11} {}", job.name);
    }
}

pub(crate) fn run(args: Args) -> Result<()> {
    let run_url = args
        .run_url
        .clone()
        .or_else(|| get_workflow_run(&args.repo, args.run_id).ok().and_then(|run| run.url));

    if let Some(run_url) = run_url {
        println!("Waiting for workflow result... {run_url}");
    } else {
        println!(
            "Waiting for workflow result: {}/actions/runs/{}",
            args.repo, args.run_id
        );
    }

    for _ in 0..args.max_attempts {
        let run = get_workflow_run(&args.repo, args.run_id)?;
        if run.status == "completed" {
            print_job_summary(&run);
            let conclusion = run.conclusion.as_deref().unwrap_or("success");
            if conclusion == "success" {
                return Ok(());
            }
            bail!("workflow run {} completed with conclusion: {conclusion}", args.run_id);
        }

        println!("workflow run {} status: {}", args.run_id, run.status);
        std::thread::sleep(Duration::from_secs(args.interval_seconds));
    }

    bail!("timed out waiting for workflow run {} to complete", args.run_id)
}
