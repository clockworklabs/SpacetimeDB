# StackBench dashboard

The dashboard is a local web view of campaign results and a way to start runs.
It reads the plans, campaign state, and evidence that the CLI writes, and starts
runs through the same job and campaign commands. It does not grade anything
itself.

## Pages

- **Campaigns** (`/`): a lane per running attempt, then one row per campaign
  with its status and per-stack score.
- **Campaign** (`/c/:key`): plan facts, completion, scores, repairs, time,
  spend, and attempts. `?chart=completion|cost|distribution` picks the chart and
  `&unit=features|checks` the counting unit. Dependency campaigns add the
  feature graph (`?questlines=grid|graph|replay`).
- **Attempt** (`/c/:key/a/:attemptId`): the attempt's figures and
  `?tab=checks|transcript|screenshots|files|log`. Checks shows each check's
  recorded status, expected value, and observation. Transcript shows the
  agent's sessions, including tool calls. Log shows the controller's output.
- **New run** (`/new`): choose workload, level, stacks, models, guidance,
  repetitions, repairs, and limits; review the attempt count and cost cap; start.
  Defaults come from the installed presets under `results/run-presets/`.
- **Saved plans** (`/plans`): the exact configuration behind each run.
- **Checks** (`/checks`): search the installed check definitions and inspect
  their steps, waits, and pass rules.

"Before repairs" shows each level's first-build evidence. Later levels still
carry earlier fixes, so it is not a repair-free run.

## Start and stop

From `tools/stack-bench`, after [appliance setup](../appliance/README.md):

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
docker compose --env-file operator.env -f appliance/docker-compose.yaml --profile dashboard stop dashboard
```

Open `localhost:7331`. Docker publishes the port on the loopback interface only,
and writes need the browser's origin and a per-server token, so there is no
password. Do not expose it on a shared network. Outside the appliance,
`npm run dashboard` serves a read-only view of `tools/stack-bench/results`;
`--port` and `--results` change the defaults.

Start, Resume, and Stop run the CLI in a controller container. Stop interrupts
the attempt; it does not pause it. Resume continues scheduled dependency work.
Starting a run submits a job; starting again with the same reviewed settings
returns the same job.

Cost shown as `~$` is a live estimate from completed responses at the plan's
rates. The final receipt replaces it, and estimates never enter scores or budgets.

## Routes

| route | returns |
| --- | --- |
| `GET /api/health` | `read-only` or `controller` |
| `GET /api/overview?page=N&filter=F` | campaign summaries, 20 per page |
| `GET /api/campaigns/:key` | the campaign sheet |
| `GET /api/campaigns/:key/live` | live spend, activity, and phase |
| `GET /api/campaigns/:key/progression` | the dependency graph and its replay |
| `GET /api/campaigns/:key/attempts/:id/checks` | per-check outcome and history |
| `GET /api/campaigns/:key/attempts/:id/package` | the evidence listing |
| `GET /api/campaigns/:key/attempts/:id/log?from=N` | log bytes after `N` |
| `GET /api/campaigns/:key/attempts/:id/transcript` | paged transcript messages |
| `GET /api/campaigns/:key/attempts/:id/time` | time allowance and grants |
| `POST /api/campaigns/:key/attempts/:id/time` | request more time |
| `GET /api/campaigns/:key/artifacts/:id` | one allowlisted artifact |
| `GET /api/events` | server-sent change events |
| `GET /api/plans` | the discovered plans |
| `GET /api/session` | whether controls are available, and the browser token |
| `GET /api/run-setup` | workload choices and defaults |
| `POST /api/runs/prepare` | the review of a run setup |
| `POST /api/runs` | start a reviewed run |
| `GET /api/reference-runs` | reference qualification runs |
| `POST /api/jobs`, `GET /api/jobs`, `GET /api/jobs/:id`, `POST /api/jobs/:id/start`, `POST /api/jobs/:id/cancel` | the job queue |
| `POST /api/campaigns/:key/resume` | run scheduled dependency work |
| `POST /api/campaigns/:key/stop` | stop the controller shown by the page |

The dashboard records the controller operations it starts in
`<results>/dashboard/operations.jsonl`. It never edits grades or source.
