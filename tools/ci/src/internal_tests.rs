use anyhow::{bail, Context, Result};
use clap::Args;
use github_api::{prs_mentioning, GithubApi, PullRequest, WorkflowRun};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

const PUBLIC_REPO: &str = "clockworklabs/SpacetimeDB";
const PRIVATE_REPO: &str = "clockworklabs/SpacetimeDBPrivate";
const PRIVATE_WORKFLOW: &str = "ci.yml";
const PRIVATE_DEFAULT_BRANCH: &str = "master";
const PRIVATE_PROTOCOL_FILE: &str = ".github/internal-tests-protocol";
const PINNED_COORDINATION_PROTOCOL: &str = "2";

#[derive(Args)]
/// Selects or starts the private workflow for a public Internal Tests run.
pub(crate) struct CoordinateArgs {
    #[arg(long)]
    public_run_attempt: u64,

    #[arg(long)]
    public_sha: String,

    #[arg(long)]
    public_pr_number: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivateProtocol {
    Legacy,
    Pinned,
}

#[derive(Debug)]
struct PrivateSource {
    sha: String,
    workflow_ref: String,
    pull: Option<PullRequest>,
}

/// The subset of a GitHub workflow run needed by the coordinator.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedRun {
    id: u64,
    event: String,
    status: String,
    conclusion: Option<String>,
    attempt: u64,
    url: String,
}

impl From<&WorkflowRun> for SelectedRun {
    fn from(run: &WorkflowRun) -> Self {
        Self {
            id: run.id,
            event: run.event.clone(),
            status: run.status.clone(),
            conclusion: run.conclusion.clone(),
            attempt: run.run_attempt,
            url: run.html_url.clone(),
        }
    }
}

#[derive(Serialize)]
struct PinnedPrivateCiInputs<'a> {
    private_ref: &'a str,
    public_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_pr_number: Option<String>,
}

#[derive(Serialize)]
struct LegacyPrivateCiInputs<'a> {
    public_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_pr_number: Option<String>,
}

/// Coordinates the public Internal Tests run without checking out or executing
/// code from the private repository.
pub(crate) fn coordinate(args: CoordinateArgs) -> Result<()> {
    let github = GithubApi::from_environment();
    let public_pr_created_at = validate_public_pr_head(&github, &args)?;
    let private_source = resolve_private_source(&github, args.public_pr_number)?;

    let selected = match private_protocol(&github, &private_source.sha)? {
        PrivateProtocol::Legacy => {
            println!(
                "Private source {} does not advertise pinned coordination; using the legacy dispatch protocol.",
                private_source.sha
            );
            dispatch_legacy_private_ci(&github, &args)?
        }
        PrivateProtocol::Pinned => coordinate_pinned(&github, &args, &private_source, public_pr_created_at.as_deref())?,
    };

    write_github_output("run_id", selected.id)?;
    write_github_output("run_url", &selected.url)?;
    write_github_output("cancel_on_cancel", selected.event == "workflow_dispatch")?;
    Ok(())
}

/// The marker is a capability probe, so any read failure uses the legacy protocol.
/// A successfully-read but unknown version remains an error.
fn private_protocol(github: &GithubApi, private_sha: &str) -> Result<PrivateProtocol> {
    protocol_from_marker_result(
        private_sha,
        github.repository_file(PRIVATE_REPO, PRIVATE_PROTOCOL_FILE, private_sha),
    )
}

fn protocol_from_marker_result(private_sha: &str, marker: Result<String>) -> Result<PrivateProtocol> {
    match marker {
        Ok(protocol) => parse_private_protocol(private_sha, &protocol),
        Err(error) => {
            eprintln!(
                "warning: failed to read the Internal Tests protocol marker at private source {private_sha}: {error:#}; falling back to legacy coordination"
            );
            Ok(PrivateProtocol::Legacy)
        }
    }
}

fn parse_private_protocol(private_sha: &str, protocol: &str) -> Result<PrivateProtocol> {
    match protocol.trim() {
        PINNED_COORDINATION_PROTOCOL => Ok(PrivateProtocol::Pinned),
        version => bail!("private source {private_sha} advertises unsupported Internal Tests protocol {version:?}"),
    }
}

fn coordinate_pinned(
    github: &GithubApi,
    args: &CoordinateArgs,
    private_source: &PrivateSource,
    public_pr_created_at: Option<&str>,
) -> Result<SelectedRun> {
    let (equivalent_private_run, workflow_dispatch_runs) =
        fetch_existing_runs(github, args, private_source, public_pr_created_at)?;
    let mut selected = select_existing_run(
        args,
        &private_source.sha,
        equivalent_private_run,
        workflow_dispatch_runs,
    );

    // A pull-request run is authoritative and is never retried here. A workflow-dispatch run
    // is the coordinator's fallback and may rerun only its failed jobs.
    rerun_failed_dispatch_jobs(github, &mut selected, args.public_run_attempt)?;

    if should_dispatch(&selected) {
        selected = Some(dispatch_pinned_private_ci(github, args, private_source)?);
    }

    selected.context(
        "failed to select or dispatch the private CI workflow; inspect the private workflow, then rerun the public Internal Tests workflow",
    )
}

/// Verifies that a public PR still points at the commit that triggered this run.
fn validate_public_pr_head(github: &GithubApi, args: &CoordinateArgs) -> Result<Option<String>> {
    let Some(public_pr_number) = args.public_pr_number else {
        return Ok(None);
    };

    let current = github.pull_request(PUBLIC_REPO, public_pr_number)?;
    if current.head.sha != args.public_sha {
        bail!(
            "public PR #{public_pr_number} advanced from {} to {}; rerun the public Internal Tests workflow for the newer public SHA",
            args.public_sha,
            current.head.sha
        );
    }
    Ok(Some(current.created_at))
}

/// Resolves the single open private PR linked through GitHub's cross-reference
/// timeline. Draft PRs are included and duplicate timeline events are collapsed.
fn find_related_private_pr(github: &GithubApi, public_pr_number: Option<u64>) -> Result<Option<PullRequest>> {
    let Some(public_pr_number) = public_pr_number else {
        return Ok(None);
    };

    let related = prs_mentioning(github, PUBLIC_REPO, PRIVATE_REPO, public_pr_number, true)?;
    if related.len() > 1 {
        bail!(
            "found multiple open private PRs related to public PR #{public_pr_number}: {}",
            related
                .iter()
                .map(|pull| pull.number.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    related
        .first()
        .map(|pull| github.pull_request(PRIVATE_REPO, pull.number))
        .transpose()
}

/// Selects an immutable private commit, preferring a linked PR over private master.
fn resolve_private_source(github: &GithubApi, public_pr_number: Option<u64>) -> Result<PrivateSource> {
    if let Some(pull) = find_related_private_pr(github, public_pr_number)? {
        println!("Using related private PR #{} at {}.", pull.number, pull.head.sha);
        return Ok(PrivateSource {
            sha: pull.head.sha.clone(),
            workflow_ref: pull.head.ref_name.clone(),
            pull: Some(pull),
        });
    }

    let sha = github.branch(PRIVATE_REPO, PRIVATE_DEFAULT_BRANCH)?.commit.sha;
    println!("No related private PR; using private master at {sha}.");
    Ok(PrivateSource {
        sha,
        workflow_ref: PRIVATE_DEFAULT_BRANCH.to_owned(),
        pull: None,
    })
}

/// Builds the display title that identifies a public/private source pair.
fn run_name(public_sha: &str, private_sha: &str) -> String {
    format!("CI [public={public_sha}; private={private_sha}]")
}

/// Returns the newest workflow run by update time, using creation time as a tie-breaker.
fn newest_run(runs: impl IntoIterator<Item = WorkflowRun>) -> Option<WorkflowRun> {
    runs.into_iter().max_by(|left, right| {
        left.updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.created_at.cmp(&right.created_at))
    })
}

fn matching_private_pr_run(runs: Vec<WorkflowRun>, pull: &PullRequest) -> Option<WorkflowRun> {
    newest_run(runs.into_iter().filter(|run| run.head_sha == pull.head.sha))
}

fn public_submodule_sha(github: &GithubApi, private_tree_sha: &str) -> Result<Option<String>> {
    let tree = github.git_tree(PRIVATE_REPO, private_tree_sha)?;
    Ok(tree
        .tree
        .into_iter()
        .find(|entry| entry.path == "public" && entry.kind == "commit")
        .map(|entry| entry.sha))
}

fn equivalent_private_pr_run(
    github: &GithubApi,
    pull: &PullRequest,
    public_sha: &str,
    created_at: Option<&str>,
) -> Result<Option<WorkflowRun>> {
    if public_submodule_sha(github, &pull.head.sha)?.as_deref() != Some(public_sha) {
        return Ok(None);
    }

    let mut filters = vec![
        ("event", "pull_request".to_owned()),
        ("head_sha", pull.head.sha.clone()),
    ];
    if let Some(created_at) = created_at {
        filters.push(("created", format!(">={created_at}")));
    }
    let runs = github.latest_workflow_runs(PRIVATE_REPO, PRIVATE_WORKFLOW, &filters)?;
    Ok(matching_private_pr_run(runs, pull))
}

fn fetch_existing_runs(
    github: &GithubApi,
    args: &CoordinateArgs,
    private_source: &PrivateSource,
    public_pr_created_at: Option<&str>,
) -> Result<(Option<WorkflowRun>, Vec<WorkflowRun>)> {
    let created_at = created_lower_bound(public_pr_created_at, private_source.pull.as_ref());
    let equivalent_private_run = if let Some(pull) = private_source.pull.as_ref() {
        equivalent_private_pr_run(github, pull, &args.public_sha, created_at)?
    } else {
        None
    };

    let should_search_dispatch_history = private_source.pull.is_some() || args.public_run_attempt > 1;
    let workflow_dispatch_runs = if equivalent_private_run.is_none() && should_search_dispatch_history {
        let mut filters = vec![
            ("event", "workflow_dispatch".to_owned()),
            ("branch", private_source.workflow_ref.clone()),
        ];
        if let Some(created_at) = created_at {
            filters.push(("created", format!(">={created_at}")));
        }
        github.latest_workflow_runs(PRIVATE_REPO, PRIVATE_WORKFLOW, &filters)?
    } else {
        if private_source.pull.is_none() && args.public_run_attempt == 1 {
            println!("No linked private PR on the initial attempt; dispatching without scanning private CI history.");
        }
        Vec::new()
    };

    Ok((equivalent_private_run, workflow_dispatch_runs))
}

/// Excludes runs from before either linked PR existed.
fn created_lower_bound<'a>(
    public_pr_created_at: Option<&'a str>,
    private_pull: Option<&'a PullRequest>,
) -> Option<&'a str> {
    [public_pr_created_at, private_pull.map(|pull| pull.created_at.as_str())]
        .into_iter()
        .flatten()
        .max()
}

fn select_existing_run(
    args: &CoordinateArgs,
    private_sha: &str,
    equivalent_private_run: Option<WorkflowRun>,
    workflow_dispatch_runs: Vec<WorkflowRun>,
) -> Option<SelectedRun> {
    if let Some(run) = equivalent_private_run {
        println!("The private PR CI run tested the same public SHA: {}.", run.html_url);
        return Some(SelectedRun::from(&run));
    }

    let expected_name = run_name(&args.public_sha, private_sha);
    let selected = newest_run(
        workflow_dispatch_runs
            .into_iter()
            .filter(|run| run.display_title == expected_name),
    );
    if let Some(run) = selected {
        println!(
            "Found the private run for this exact public/private pair: {}.",
            run.html_url
        );
        return Some(SelectedRun::from(&run));
    }

    None
}

fn wait_for_rerun(github: &GithubApi, run: &SelectedRun) -> Result<SelectedRun> {
    for _ in 0..30 {
        let current = github.workflow_run(PRIVATE_REPO, run.id)?;
        if current.run_attempt > run.attempt {
            return Ok(SelectedRun {
                id: current.id,
                event: run.event.clone(),
                status: current.status,
                conclusion: current.conclusion,
                attempt: current.run_attempt,
                url: current.html_url,
            });
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    bail!(
        "timed out waiting for private CI run {} to start its failed-job rerun; rerun private CI manually, then rerun public Internal Tests",
        run.id
    )
}

fn rerun_failed_dispatch_jobs(
    github: &GithubApi,
    selected: &mut Option<SelectedRun>,
    public_run_attempt: u64,
) -> Result<()> {
    let Some(run) = selected else {
        return Ok(());
    };

    if should_rerun_failed_dispatch_jobs(run, public_run_attempt) {
        println!("Re-running only failed jobs in the private CI workflow at {}.", run.url);
        github.rerun_failed_jobs(PRIVATE_REPO, run.id)?;
        *selected = Some(wait_for_rerun(github, run)?);
    } else {
        println!("Reusing {} without starting private CI again.", run.url);
    }
    Ok(())
}

fn should_rerun_failed_dispatch_jobs(run: &SelectedRun, public_run_attempt: u64) -> bool {
    run.event == "workflow_dispatch"
        && public_run_attempt > 1
        && run.status == "completed"
        && matches!(run.conclusion.as_deref(), Some("failure" | "timed_out"))
}

fn should_dispatch(selected: &Option<SelectedRun>) -> bool {
    selected.as_ref().is_none_or(|run| {
        run.event != "pull_request"
            && run.status == "completed"
            && !matches!(run.conclusion.as_deref(), Some("success" | "failure" | "timed_out"))
    })
}

fn dispatch_pinned_private_ci(
    github: &GithubApi,
    args: &CoordinateArgs,
    private_source: &PrivateSource,
) -> Result<SelectedRun> {
    let response = github.dispatch_workflow(
        PRIVATE_REPO,
        PRIVATE_WORKFLOW,
        &private_source.workflow_ref,
        PinnedPrivateCiInputs {
            private_ref: &private_source.sha,
            public_ref: &args.public_sha,
            public_pr_number: args.public_pr_number.map(|number| number.to_string()),
        },
    )?;
    println!(
        "Dispatched full private CI for the current public/private pair: {}.",
        response.html_url
    );
    Ok(dispatched_run(response.workflow_run_id, response.html_url))
}

fn dispatch_legacy_private_ci(github: &GithubApi, args: &CoordinateArgs) -> Result<SelectedRun> {
    let response = github.dispatch_workflow(
        PRIVATE_REPO,
        PRIVATE_WORKFLOW,
        PRIVATE_DEFAULT_BRANCH,
        LegacyPrivateCiInputs {
            public_ref: &args.public_sha,
            public_pr_number: args.public_pr_number.map(|number| number.to_string()),
        },
    )?;
    println!("Dispatched legacy private CI: {}.", response.html_url);
    Ok(dispatched_run(response.workflow_run_id, response.html_url))
}

fn dispatched_run(id: u64, url: String) -> SelectedRun {
    SelectedRun {
        id,
        event: "workflow_dispatch".to_owned(),
        status: "queued".to_owned(),
        conclusion: None,
        attempt: 1,
        url,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pull(created_at: &str) -> PullRequest {
        PullRequest {
            number: 42,
            state: "open".to_owned(),
            head: github_api::PullRequestRef {
                ref_name: "feature".to_owned(),
                sha: "private-sha".to_owned(),
            },
            created_at: created_at.to_owned(),
        }
    }

    fn workflow_run(id: u64, updated_at: &str, conclusion: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            name: "CI".to_owned(),
            path: ".github/workflows/ci.yml".to_owned(),
            display_title: "CI".to_owned(),
            event: "workflow_dispatch".to_owned(),
            head_sha: "private-sha".to_owned(),
            status: "completed".to_owned(),
            conclusion: Some(conclusion.to_owned()),
            run_attempt: 1,
            html_url: format!("https://example.test/{id}"),
            created_at: updated_at.to_owned(),
            updated_at: updated_at.to_owned(),
            pull_requests: Vec::new(),
        }
    }

    fn selected_run(event: &str, status: &str, conclusion: Option<&str>) -> SelectedRun {
        SelectedRun {
            id: 1,
            event: event.to_owned(),
            status: status.to_owned(),
            conclusion: conclusion.map(str::to_owned),
            attempt: 1,
            url: "https://example.test/1".to_owned(),
        }
    }

    fn coordinate_args() -> CoordinateArgs {
        CoordinateArgs {
            public_run_attempt: 1,
            public_sha: "public-sha".to_owned(),
            public_pr_number: None,
        }
    }

    fn workflow_dispatch_run(id: u64, updated_at: &str, display_title: &str) -> WorkflowRun {
        let mut run = workflow_run(id, updated_at, "failure");
        run.display_title = display_title.to_owned();
        run
    }

    #[test]
    fn protocol_negotiation_accepts_pinned_and_rejects_unknown_versions() {
        assert_eq!(
            protocol_from_marker_result("private-sha", Err(anyhow::anyhow!("probe failed"))).unwrap(),
            PrivateProtocol::Legacy
        );
        assert_eq!(
            parse_private_protocol("private-sha", "2\n").unwrap(),
            PrivateProtocol::Pinned
        );
        assert!(parse_private_protocol("private-sha", "3\n").is_err());
    }

    #[test]
    fn run_name_contains_both_immutable_source_refs() {
        assert_eq!(
            run_name("public-sha", "private-sha"),
            "CI [public=public-sha; private=private-sha]"
        );
    }

    #[test]
    fn created_lower_bound_uses_the_later_pr_creation_time() {
        let private_pull = pull("2026-01-02T00:00:00Z");
        assert_eq!(
            created_lower_bound(Some("2026-01-01T00:00:00Z"), Some(&private_pull)),
            Some("2026-01-02T00:00:00Z")
        );
        assert_eq!(
            created_lower_bound(Some("2026-01-03T00:00:00Z"), Some(&private_pull)),
            Some("2026-01-03T00:00:00Z")
        );
        assert_eq!(created_lower_bound(None, None), None);
    }

    #[test]
    fn private_pr_run_takes_precedence_over_dispatch_candidate() {
        let args = coordinate_args();
        let expected = run_name(&args.public_sha, "private-sha");
        let selected = select_existing_run(
            &args,
            "private-sha",
            Some(workflow_dispatch_run(3, "2026-01-03T00:00:00Z", "private-pr-run")),
            vec![workflow_dispatch_run(4, "2026-01-04T00:00:00Z", &expected)],
        );
        assert_eq!(selected.unwrap().id, 3);
    }

    #[test]
    fn dispatch_selection_requires_exact_source_pair_identity() {
        let args = coordinate_args();
        let expected = run_name(&args.public_sha, "private-sha");
        let selected = select_existing_run(
            &args,
            "private-sha",
            None,
            vec![
                workflow_dispatch_run(1, "2026-01-01T00:00:00Z", "unrelated"),
                workflow_dispatch_run(2, "2026-01-02T00:00:00Z", &expected),
            ],
        );
        assert_eq!(selected.unwrap().id, 2);
    }

    #[test]
    fn matching_private_pr_run_uses_only_the_private_head_sha() {
        let mut run = workflow_run(1, "2026-01-01T00:00:00Z", "success");
        run.event = "pull_request".to_owned();
        assert_eq!(
            matching_private_pr_run(vec![run], &pull("2026-01-01T00:00:00Z"))
                .unwrap()
                .id,
            1
        );

        let mut different_head = workflow_run(2, "2026-01-02T00:00:00Z", "success");
        different_head.event = "pull_request".to_owned();
        different_head.head_sha = "different-private-sha".to_owned();
        assert!(matching_private_pr_run(vec![different_head], &pull("2026-01-01T00:00:00Z")).is_none());
    }

    #[test]
    fn failed_job_rerun_policy_is_limited_to_failed_dispatch_runs_on_public_reruns() {
        struct Case {
            name: &'static str,
            event: &'static str,
            status: &'static str,
            conclusion: Option<&'static str>,
            public_run_attempt: u64,
            expected: bool,
        }

        let cases = [
            Case {
                name: "unlinked failed run on a public rerun",
                event: "workflow_dispatch",
                status: "completed",
                conclusion: Some("failure"),
                public_run_attempt: 2,
                expected: true,
            },
            Case {
                name: "unlinked timed out run on a public rerun",
                event: "workflow_dispatch",
                status: "completed",
                conclusion: Some("timed_out"),
                public_run_attempt: 2,
                expected: true,
            },
            Case {
                name: "linked private PR pull-request run on a public rerun",
                event: "pull_request",
                status: "completed",
                conclusion: Some("failure"),
                public_run_attempt: 2,
                expected: false,
            },
            Case {
                name: "unlinked failed run on the initial public attempt",
                event: "workflow_dispatch",
                status: "completed",
                conclusion: Some("failure"),
                public_run_attempt: 1,
                expected: false,
            },
            Case {
                name: "successful run",
                event: "workflow_dispatch",
                status: "completed",
                conclusion: Some("success"),
                public_run_attempt: 2,
                expected: false,
            },
            Case {
                name: "private run still in progress",
                event: "workflow_dispatch",
                status: "in_progress",
                conclusion: None,
                public_run_attempt: 2,
                expected: false,
            },
        ];

        for case in cases {
            let run = selected_run(case.event, case.status, case.conclusion);
            assert_eq!(
                should_rerun_failed_dispatch_jobs(&run, case.public_run_attempt),
                case.expected,
                "unexpected rerun policy for {}",
                case.name
            );
        }
    }

    #[test]
    fn dispatch_policy_reuses_known_runs_and_replaces_cancelled_dispatches() {
        struct Case {
            name: &'static str,
            event: &'static str,
            run: Option<(&'static str, Option<&'static str>)>,
            expected: bool,
        }

        let cases = [
            Case {
                name: "no matching run",
                event: "workflow_dispatch",
                run: None,
                expected: true,
            },
            Case {
                name: "queued run",
                event: "workflow_dispatch",
                run: Some(("queued", None)),
                expected: false,
            },
            Case {
                name: "in-progress run",
                event: "workflow_dispatch",
                run: Some(("in_progress", None)),
                expected: false,
            },
            Case {
                name: "successful run",
                event: "workflow_dispatch",
                run: Some(("completed", Some("success"))),
                expected: false,
            },
            Case {
                name: "failed run",
                event: "workflow_dispatch",
                run: Some(("completed", Some("failure"))),
                expected: false,
            },
            Case {
                name: "timed out run",
                event: "workflow_dispatch",
                run: Some(("completed", Some("timed_out"))),
                expected: false,
            },
            Case {
                name: "cancelled run",
                event: "workflow_dispatch",
                run: Some(("completed", Some("cancelled"))),
                expected: true,
            },
            Case {
                name: "skipped run",
                event: "workflow_dispatch",
                run: Some(("completed", Some("skipped"))),
                expected: true,
            },
            Case {
                name: "cancelled pull-request run remains authoritative",
                event: "pull_request",
                run: Some(("completed", Some("cancelled"))),
                expected: false,
            },
            Case {
                name: "skipped pull-request run remains authoritative",
                event: "pull_request",
                run: Some(("completed", Some("skipped"))),
                expected: false,
            },
        ];

        for case in cases {
            let selected = case
                .run
                .map(|(status, conclusion)| selected_run(case.event, status, conclusion));
            assert_eq!(
                should_dispatch(&selected),
                case.expected,
                "unexpected dispatch policy for {}",
                case.name
            );
        }
    }
}
