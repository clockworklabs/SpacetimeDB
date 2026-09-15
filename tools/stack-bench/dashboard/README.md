# Stack Bench dashboard

The dashboard is an optional local view over Stack Bench results. It does not
schedule attempts, grade applications, or repair source itself.
Campaign plans, durable campaign state, and run artifacts remain the source of
truth. Appliance controls call the shared job and campaign operations. The
dashboard does not have a separate execution engine.

## Pages

- Campaigns (`/`) — a lane per running attempt, then one row per campaign with
  its shape, status, and per-stack score. A campaign whose plan or state this
  build cannot read appears with the status `unreadable` and the reason in
  place of its title.
- Campaign (`/c/:key`) — plan facts, completion, scores, repairs, time, spend,
  and attempts. The chart switches between completion, cost, and distribution
  with `?chart=completion|cost|distribution`. Features are the default unit;
  `&unit=features|checks` switches completion and distribution. Cost keeps these
  controls visible but disabled. Toggle a stack or a repetition to show or hide it.
  Dependency campaigns add questline rows, which
  `?questlines=grid|graph|replay` switches between; `&step=N` moves the replay
  cursor. Sequential campaigns show one row pair per level instead.
- Attempt (`/c/:key/a/:attemptId`) — attempt figures, the dependency graph, and
  `?tab=checks|transcript|screenshots|files|log`. The transcript shows build and
  repair sessions, including tool calls. It follows live work at the newest
  page and pauses updates while you read earlier messages. The log tab shows
  controller output separately.
  In Checks, expand a requirement to see the recorded status, summary, expected
  value and observation for each grade. Missing details remain explicit. These
  are raw grading observations, including unsuccessful repairs; the accepted
  score remains in the run summary. Blocked, inconclusive and harness failures
  retain their recorded status. Credentials and marked sensitive details are omitted.
- New run (`/new`) — select workload, level, stacks, models, guidance, repetitions,
  repairs, and limits. Review the attempt count and cost cap, then start.
- Saved plans (`/plans`) — inspect the exact configuration behind each run.

## Modes

"Before repairs" uses first-build evidence before repairs at each level.
Later levels retain earlier fixes and feedback; this is not a feedback-free run.

Inside the appliance (`STACK_BENCH_APPLIANCE=1`) the dashboard runs in
controller mode: Start and Resume launch the CLI in an owned controller
container. Stop sends a durable request to that controller instance.
Stop interrupts the active attempt; it does not pause it. Resume starts
scheduled dependency work and does not restart a stopped sequential attempt.
It cannot restore a lost database or agent session. A planned depth pause uses
the CLI's `pause-status` and `continue-depth` commands and requires the original
controller to stay running. See [pause behavior](../README.md#pause-before-a-later-depth).
Elsewhere it runs read-only and those controls are unavailable;
`GET /api/health` reports which mode is active.

From `tools/stack-bench`, `npm run dashboard` starts a read-only host view over
`tools/stack-bench/results`. Pass `--port` to move it off 7331 and `--results`
to point it at another results directory.

## Appliance

Run these commands from `tools/stack-bench` after the
[appliance setup](../appliance/README.md).

```sh
docker compose --env-file operator.env \
  -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

Open `http://127.0.0.1:7331`. Docker publishes that port only on the host's
loopback interface. Stop it with:

```sh
docker compose --env-file operator.env \
  -f appliance/docker-compose.yaml --profile dashboard stop dashboard
```

Run controls need no separate password. This is a local, single-user dashboard:
the port binds to loopback, requests must use a loopback Host, and writes require
the exact browser origin and a per-server CSRF token. Other websites cannot read
that token. Local processes that can access the dashboard are trusted. Do not
publish this service through a proxy or on a shared network without authentication.
Model credentials remain private. Starting a run submits
an idempotent execution job and starts its worker in an
owned controller. Retrying Start with the same reviewed settings returns the same
job. A queued job has a status page before its campaign artifacts exist.

The campaign page lists every attempt with its variant and repetition. Check
completion uses all selected checks, including checks not reached. Feature
completion requires all selected checks of a feature to pass, including its
production guarantees. Weighted score remains a separate measure. Spend includes
all executions and shows upper bounds and unknown values. Different comparison
conditions do not share one score average. Files links to the report and its
public export manifest; the manifest lists evidence and any reconstruction gaps.
Charts connect saved observations; intermediate values are not measured.
Comparison summaries and the distribution use eligible completed attempts.
Excluded attempts remain labelled in chart controls and the Runs table. Cost and
progress-over-time charts retain their observations, with the same exclusion label.
Live progress and total spend include unfinished work; total spend also includes
excluded attempts. These operational values are separate from comparison metrics.

## Routes

| route | returns |
| --- | --- |
| `GET /api/health` | `read-only` or `controller` |
| `GET /api/overview` | one summary per campaign |
| `GET /api/campaigns/:key` | the campaign sheet |
| `GET /api/campaigns/:key/live` | live spend, cost observations, activity, and phase |
| `GET /api/campaigns/:key/progression` | the dependency graph and its replay |
| `GET /api/campaigns/:key/attempts/:id/checks` | per-check outcome and history |
| `GET /api/campaigns/:key/attempts/:id/package` | the evidence listing |
| `GET /api/campaigns/:key/attempts/:id/log?from=N` | log bytes after `N` |
| `GET /api/campaigns/:key/attempts/:id/transcript` | selected session and paged transcript messages |
| `GET /api/campaigns/:key/attempts/:id/time` | time allowance, grants, and continuation eligibility |
| `POST /api/campaigns/:key/attempts/:id/time` | request additional time |
| `GET /api/campaigns/:key/artifacts/:name` | one allowlisted artifact |
| `GET /api/events` | the change stream |
| `GET /api/plans` | the discovered plans |
| `POST /api/campaigns` | start a run |
| `POST /api/campaigns/:key/resume` | run eligible scheduled dependency work |
| `POST /api/campaigns/:key/stop` | stop the exact controller shown by the page |

The [job API](../docs/execution-jobs.md#api-and-service-integration) adds durable
submission, listing, status, and cancellation at `/api/jobs`. Submission queues
work; an enabled worker must claim it before execution starts.

Each payload covers one question, so opening a campaign or a tab is what pays
for reading it. The overview and the sheet are cached against the size and
modification time of the evidence they read, including while a campaign runs.

## The event stream

`GET /api/events` is a server-sent event stream. A `campaign` event names a
campaign whose plan, state, run output, or progression state changed; a `log`
event names an attempt whose stdout grew. Changes are debounced for 500 ms and
the stream sends a comment every 25 seconds so an idle connection stays open.
Campaign events refresh the affected evidence. Log events fetch only live fields
and the open log or transcript. The client also refreshes live fields every five
seconds while runs are active; Claude Code and Codex usage can advance without a
controller log write. Live-cost reads share a server cache and concurrent reads.
Docker transcript reads time out after five seconds; a failed read keeps saved
receipts visible. Logs do not invalidate the evidence sheet or graph replay.
While the stream is down, a full refresh every 15 seconds recovers missed evidence
changes. Hidden tabs stop both the event stream and refresh work.

The watcher uses a recursive `fs.watch` per campaign directory. Where the
platform or the mount does not support one it polls the same file fingerprints
every 5 seconds instead. The server logs which mode it opened with when the
first client subscribes.

## What it touches

It reads plans from `<results>/plans`, campaigns from `<results>/campaigns`, and
jobs from `<results>/jobs`. Authorized controls use the shared APIs to write job,
cancellation, and time-grant records. The dashboard records direct controller
operations in `<results>/dashboard/operations.jsonl` and retains their output
under `<results>/dashboard/operations`. Live transcript reads inspect the exact
owned coding container; saved transcripts use the attempt's transcript files.
It does not edit grades or source.

## Workload setup and AI access

Appliance setup installs workload presets under `results/run-presets/`. Each preset
uses the existing campaign manifest format. It supplies the supported levels,
stacks, priced models, guidance conditions, and pinned runtime. Operators can add
approved models and conditions there. Setup does not replace existing presets;
update their runtime pins when deploying a new release. Invalid presets report their
errors. Model prices are recorded values; the dashboard does not guess prices.

Both interfaces use `src/campaigns/run-setup.ts` and the existing execution jobs:

```sh
node dist/commands/job-cli.js options --results /path/to/results
node dist/commands/job-cli.js prepare selections.json --results /path/to/results > review.json
node dist/commands/job-cli.js start review.json --results /path/to/results --host local
```

`options` returns each workload's choices and defaults. `prepare` takes `key`,
`workload`, `workloadSha256` (from options), `level`, `stacks`, `agents` (`index` and `effort`), `conditions`,
`repetitions`, `parallelism`, `repairs`, `timeoutMinutes`, `maxCostUsd`,
`pauseAfterDepth` (null for none), and `credentials` (empty for appliance defaults).
The response records the review identity, immutable plan, cost cap, account mode,
and grading qualification. `start` accepts that response. Any change requires a
new review. It starts the same worker as the dashboard and returns the job ID before
waiting for completion. No model or reasoning level is substituted.

HTTP clients use `GET /api/run-setup`, `POST /api/runs/prepare`, and `POST /api/runs`.
Writes require the same origin and browser token as other controls.
Named credential profiles expose only their labels, provider, version, and account
mode. Secret paths and values remain on the server.
