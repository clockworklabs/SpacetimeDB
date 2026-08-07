use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use duct::{cmd, Expression};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

const API_VERSION: &str = "2022-11-28";
const PUBLIC_REPO: &str = "clockworklabs/SpacetimeDB";
const PRIVATE_REPO: &str = "clockworklabs/SpacetimeDBPrivate";
const PRIVATE_WORKFLOW: &str = "ci.yml";
const PRIVATE_DEFAULT_BRANCH: &str = "master";

#[derive(Args)]
/// Selects or starts the private workflow for a public Internal Tests run.
pub(crate) struct CoordinateArgs {
    /// Immutable public commit to test.
    #[arg(long)]
    public_sha: String,

    /// Public pull request number, when coordinating a pull request run.
    #[arg(long)]
    public_pr_number: Option<u64>,
}

#[derive(Debug)]
enum PrivateSource {
    LinkedPrivatePr { pull: PullRequest },
    PrivateMaster { sha: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedRun {
    id: u64,
    status: String,
    conclusion: Option<String>,
    attempt: u64,
    url: String,
}

struct CoordinatedRun {
    selected: SelectedRun,
    did_start: bool,
}

impl From<WorkflowRun> for SelectedRun {
    fn from(run: WorkflowRun) -> Self {
        Self {
            id: run.id,
            status: run.status,
            conclusion: run.conclusion,
            attempt: run.run_attempt,
            url: run.html_url,
        }
    }
}

impl From<DispatchResponse> for SelectedRun {
    fn from(response: DispatchResponse) -> Self {
        Self {
            id: response.workflow_run_id,
            status: "queued".to_owned(),
            conclusion: None,
            attempt: 1,
            url: response.html_url,
        }
    }
}

fn api(args: impl IntoIterator<Item = String>) -> Expression {
    let mut gh_args = vec![
        "api".to_owned(),
        "-H".to_owned(),
        "Accept: application/vnd.github+json".to_owned(),
        "-H".to_owned(),
        format!("X-GitHub-Api-Version: {API_VERSION}"),
    ];
    gh_args.extend(args);
    cmd("gh", gh_args)
}

fn get<T: DeserializeOwned>(path: &str) -> Result<T> {
    let output = api(["--method".to_owned(), "GET".to_owned(), path.to_owned()])
        .read()
        .with_context(|| format!("GitHub API GET {path} failed"))?;
    serde_json::from_str(&output).context("failed to parse GitHub response")
}

fn get_paginated<P: DeserializeOwned>(path: &str) -> Result<Vec<P>> {
    let output = api([
        "--method".to_owned(),
        "GET".to_owned(),
        "--paginate".to_owned(),
        "--slurp".to_owned(),
        path.to_owned(),
    ])
    .read()
    .with_context(|| format!("paginated GitHub API GET {path} failed"))?;
    serde_json::from_str(&output).context("failed to parse paginated GitHub response")
}

fn post<I: Serialize, O: DeserializeOwned>(path: &str, input: &I) -> Result<O> {
    let body = serde_json::to_vec(input).context("failed to serialize GitHub request")?;
    let output = api([
        "--method".to_owned(),
        "POST".to_owned(),
        "--input".to_owned(),
        "-".to_owned(),
        path.to_owned(),
    ])
    .stdin_bytes(body)
    .read()
    .with_context(|| format!("GitHub API POST {path} failed"))?;
    serde_json::from_str(&output).context("failed to parse GitHub response")
}

fn pull_request(repo: &str, number: u64) -> Result<PullRequest> {
    get(&format!("/repos/{repo}/pulls/{number}"))
}

fn branch(repo: &str, branch: &str) -> Result<Branch> {
    get(&format!("/repos/{repo}/branches/{branch}"))
}

fn git_tree(repo: &str, sha: &str) -> Result<GitTree> {
    get(&format!("/repos/{repo}/git/trees/{sha}"))
}

/// Returns runs for an exact event and private SHA.
fn workflow_runs(event: &str, head_sha: &str) -> Result<Vec<WorkflowRun>> {
    let path = format!(
        "/repos/{PRIVATE_REPO}/actions/workflows/{PRIVATE_WORKFLOW}/runs?event={event}&head_sha={head_sha}&per_page=100"
    );
    let pages: Vec<WorkflowRunsPage> = get_paginated(&path)?;
    Ok(pages.into_iter().flat_map(|page| page.workflow_runs).collect())
}

fn workflow_run(run_id: u64) -> Result<WorkflowRunStatus> {
    get(&format!("/repos/{PRIVATE_REPO}/actions/runs/{run_id}"))
}

fn rerun_failed_jobs(run_id: u64) -> Result<()> {
    cmd!(
        "gh",
        "run",
        "rerun",
        run_id.to_string(),
        "--failed",
        "--repo",
        PRIVATE_REPO
    )
    .run()
    .with_context(|| format!("failed to rerun unsuccessful jobs in private run {run_id}"))?;
    Ok(())
}

fn dispatch_workflow(public_sha: &str) -> Result<DispatchResponse> {
    post(
        &format!("/repos/{PRIVATE_REPO}/actions/workflows/{PRIVATE_WORKFLOW}/dispatches"),
        &DispatchWorkflow {
            ref_name: PRIVATE_DEFAULT_BRANCH,
            inputs: DispatchInputs { public_ref: public_sha },
            return_run_details: true,
        },
    )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Repository {
    full_name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PullRequestRef {
    sha: String,
    repo: Option<Repository>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PullRequest {
    number: u64,
    state: String,
    head: PullRequestRef,
}

#[derive(Deserialize)]
struct TimelineIssue {
    number: u64,
    repository: Option<Repository>,
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TimelineSource {
    issue: Option<TimelineIssue>,
}

#[derive(Deserialize)]
struct TimelineEvent {
    event: Option<String>,
    source: Option<TimelineSource>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Branch {
    commit: Commit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Commit {
    sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct GitTree {
    tree: Vec<GitTreeEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct WorkflowRunsPage {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct WorkflowRunPullRequest {
    number: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct WorkflowRun {
    id: u64,
    display_title: String,
    status: String,
    conclusion: Option<String>,
    run_attempt: u64,
    html_url: String,
    created_at: String,
    #[serde(default)]
    pull_requests: Vec<WorkflowRunPullRequest>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct WorkflowRunStatus {
    id: u64,
    status: String,
    conclusion: Option<String>,
    run_attempt: u64,
    html_url: String,
}

#[derive(Serialize)]
struct DispatchInputs<'a> {
    public_ref: &'a str,
}

#[derive(Serialize)]
struct DispatchWorkflow<'a> {
    #[serde(rename = "ref")]
    ref_name: &'a str,
    inputs: DispatchInputs<'a>,
    return_run_details: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct DispatchResponse {
    workflow_run_id: u64,
    html_url: String,
}

fn related_private_pr(public_pr_number: Option<u64>) -> Result<Option<PullRequest>> {
    let Some(public_pr_number) = public_pr_number else {
        return Ok(None);
    };

    let pages: Vec<Vec<TimelineEvent>> =
        get_paginated(&format!("/repos/{PUBLIC_REPO}/issues/{public_pr_number}/timeline"))?;
    let numbers = pages
        .into_iter()
        .flatten()
        .filter(|event| event.event.as_deref() == Some("cross-referenced"))
        .filter_map(|event| event.source.and_then(|source| source.issue))
        .filter(|issue| issue.repository.as_ref().map(|repo| repo.full_name.as_str()) == Some(PRIVATE_REPO))
        .filter(|issue| issue.pull_request.is_some())
        .map(|issue| issue.number)
        .collect::<BTreeSet<_>>();

    let mut pulls = Vec::new();
    for number in numbers {
        let pull = pull_request(PRIVATE_REPO, number)?;
        if pull.state == "open" && pull.head.repo.as_ref().map(|repo| repo.full_name.as_str()) == Some(PRIVATE_REPO) {
            pulls.push(pull);
        }
    }
    if pulls.len() > 1 {
        bail!("found multiple open linked private PRs");
    }
    Ok(pulls.pop())
}

fn resolve_private_source(public_pr_number: Option<u64>) -> Result<PrivateSource> {
    if let Some(pull) = related_private_pr(public_pr_number)? {
        println!("Found a linked private PR.");
        return Ok(PrivateSource::LinkedPrivatePr { pull });
    }

    println!("No linked private PR; using private master.");
    let sha = branch(PRIVATE_REPO, PRIVATE_DEFAULT_BRANCH)?.commit.sha;
    Ok(PrivateSource::PrivateMaster { sha })
}

fn public_submodule_sha(private_sha: &str) -> Result<String> {
    git_tree(PRIVATE_REPO, private_sha)?
        .tree
        .into_iter()
        .find(|entry| entry.path == "public" && entry.kind == "commit")
        .map(|entry| entry.sha)
        .context("the linked private PR does not contain a public submodule entry")
}

fn ensure_public_submodule_matches(pull_number: u64, actual: &str, expected: &str) -> Result<()> {
    ensure!(
        actual == expected,
        "private PR #{pull_number} has public SHA {actual}; expected {expected}. Update its public submodule"
    );
    Ok(())
}

fn newest_run(runs: impl IntoIterator<Item = WorkflowRun>) -> Option<WorkflowRun> {
    runs.into_iter().max_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    })
}

fn matching_pull_request_run(runs: Vec<WorkflowRun>, pull: &PullRequest) -> Option<WorkflowRun> {
    newest_run(
        runs.into_iter()
            .filter(|run| run.pull_requests.iter().any(|run_pull| run_pull.number == pull.number)),
    )
}

fn required_pull_request_run(pull: &PullRequest) -> Result<WorkflowRun> {
    let runs = workflow_runs("pull_request", &pull.head.sha)?;
    matching_pull_request_run(runs, pull).with_context(|| {
        format!(
            "no pull_request CI run found for private PR #{} at {}",
            pull.number, pull.head.sha
        )
    })
}

fn dispatch_title(public_sha: &str) -> String {
    format!("CI [public_ref={public_sha}]")
}

fn select_matching_dispatch_run(runs: Vec<WorkflowRun>, public_sha: &str) -> Option<WorkflowRun> {
    let expected_title = dispatch_title(public_sha);
    newest_run(runs.into_iter().filter(|run| run.display_title == expected_title))
}

fn matching_dispatch_run(private_sha: &str, public_sha: &str) -> Result<Option<WorkflowRun>> {
    Ok(select_matching_dispatch_run(
        workflow_runs("workflow_dispatch", private_sha)?,
        public_sha,
    ))
}

fn should_rerun_failed_jobs(run: &SelectedRun) -> bool {
    run.status == "completed" && run.conclusion.as_deref() != Some("success")
}

fn prepare_existing_run(run: WorkflowRun) -> Result<CoordinatedRun> {
    let selected = SelectedRun::from(run);
    if !should_rerun_failed_jobs(&selected) {
        return Ok(CoordinatedRun {
            selected,
            did_start: false,
        });
    }

    println!("Re-running unsuccessful jobs in the existing private run.");
    rerun_failed_jobs(selected.id)?;
    Ok(CoordinatedRun {
        selected: wait_for_rerun(&selected)?,
        did_start: true,
    })
}

/// Waits for GitHub to expose the new attempt so callers do not receive the stale completed result.
fn wait_for_rerun(run: &SelectedRun) -> Result<SelectedRun> {
    for _ in 0..30 {
        let current = workflow_run(run.id)?;
        if current.run_attempt > run.attempt {
            return Ok(SelectedRun {
                id: current.id,
                status: current.status,
                conclusion: current.conclusion,
                attempt: current.run_attempt,
                url: current.html_url,
            });
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    bail!("timed out waiting for the private run to start its rerun")
}

/// Reuses or reruns `pull_request` CI for a public PR with a linked private PR.
fn coordinate_linked_private_pr(public_sha: &str, pull: &PullRequest) -> Result<CoordinatedRun> {
    let private_public_sha = public_submodule_sha(&pull.head.sha)?;
    ensure_public_submodule_matches(pull.number, &private_public_sha, public_sha)?;
    let run = required_pull_request_run(pull)?;
    println!("Found the linked private PR run for this public/private SHA pair.");
    prepare_existing_run(run)
}

/// Reuses, reruns, or dispatches CI on private master for a public-only change.
fn coordinate_public_only(public_sha: &str, private_master_sha: &str) -> Result<CoordinatedRun> {
    if let Some(run) = matching_dispatch_run(private_master_sha, public_sha)? {
        println!("Found an existing private run for this public/private SHA pair.");
        return prepare_existing_run(run);
    }

    println!("Dispatching a new private run for this public/private SHA pair.");
    Ok(CoordinatedRun {
        selected: dispatch_workflow(public_sha)?.into(),
        did_start: true,
    })
}

fn write_github_output(name: &str, value: impl std::fmt::Display) -> Result<()> {
    let Ok(output_path) = std::env::var("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut output = OpenOptions::new()
        .append(true)
        .open(&output_path)
        .with_context(|| format!("failed to open GITHUB_OUTPUT at {output_path}"))?;
    writeln!(output, "{name}={value}").context("failed to write GITHUB_OUTPUT")
}

/// Coordinates the public Internal Tests run without checking out or executing private code.
pub(crate) fn coordinate(args: CoordinateArgs) -> Result<()> {
    let private_source = resolve_private_source(args.public_pr_number)?;

    let coordinated = match private_source {
        PrivateSource::LinkedPrivatePr { pull } => coordinate_linked_private_pr(&args.public_sha, &pull)?,
        PrivateSource::PrivateMaster { sha } => coordinate_public_only(&args.public_sha, &sha)?,
    };

    println!("View run: {}", coordinated.selected.url);
    write_github_output("run_id", coordinated.selected.id)?;
    write_github_output("run_url", &coordinated.selected.url)?;
    write_github_output("did_start", coordinated.did_start)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pull() -> PullRequest {
        PullRequest {
            number: 42,
            state: "open".to_owned(),
            head: PullRequestRef {
                sha: "private-sha".to_owned(),
                repo: Some(Repository {
                    full_name: PRIVATE_REPO.to_owned(),
                }),
            },
        }
    }

    fn run(id: u64, title: &str, created_at: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            display_title: title.to_owned(),
            status: "completed".to_owned(),
            conclusion: Some("success".to_owned()),
            run_attempt: 1,
            html_url: format!("https://example.test/{id}"),
            created_at: created_at.to_owned(),
            pull_requests: Vec::new(),
        }
    }

    #[test]
    fn pull_request_run_requires_the_linked_pull() {
        let mut matching = run(1, "CI", "2026-01-01T00:00:00Z");
        matching.pull_requests.push(WorkflowRunPullRequest { number: 42 });
        let mut unrelated = matching.clone();
        unrelated.id = 2;
        unrelated.pull_requests[0].number = 99;

        assert_eq!(
            matching_pull_request_run(vec![unrelated, matching], &pull())
                .unwrap()
                .id,
            1
        );
    }

    #[test]
    fn dispatch_run_requires_the_public_sha_and_prefers_the_newest() {
        let expected = dispatch_title("public-sha");
        let selected = select_matching_dispatch_run(
            vec![
                run(1, &expected, "2026-01-01T00:00:00Z"),
                run(2, "unrelated", "2026-01-03T00:00:00Z"),
                run(3, &expected, "2026-01-02T00:00:00Z"),
            ],
            "public-sha",
        )
        .unwrap();
        assert_eq!(selected.id, 3);
    }

    #[test]
    fn only_completed_unsuccessful_runs_are_rerun() {
        let selected = |status: &str, conclusion: Option<&str>| SelectedRun {
            id: 1,
            status: status.to_owned(),
            conclusion: conclusion.map(str::to_owned),
            attempt: 1,
            url: "https://example.test/1".to_owned(),
        };

        assert!(should_rerun_failed_jobs(&selected("completed", Some("failure"))));
        assert!(should_rerun_failed_jobs(&selected("completed", Some("cancelled"))));
        assert!(!should_rerun_failed_jobs(&selected("completed", Some("success"))));
        assert!(!should_rerun_failed_jobs(&selected("in_progress", None)));
    }

    #[test]
    fn dispatch_request_uses_private_master_and_the_public_sha() {
        let request = DispatchWorkflow {
            ref_name: PRIVATE_DEFAULT_BRANCH,
            inputs: DispatchInputs {
                public_ref: "public-sha",
            },
            return_run_details: true,
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "ref": "master",
                "inputs": { "public_ref": "public-sha" },
                "return_run_details": true,
            })
        );
    }
}
