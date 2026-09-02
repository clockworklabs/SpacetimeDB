# Stack Bench dashboard

The dashboard is an optional local view over Stack Bench results. It does not
schedule attempts, grade applications, operate Docker, or repair source itself.
Campaign plans, durable campaign state, and run artifacts remain the source of
truth; the dashboard only reads them and, in the appliance, asks the controller
to start or resume a run.

## Pages

- Campaigns (`/`) — a lane per running attempt, then one row per campaign with
  its shape, status, and per-stack score. A campaign whose plan or state this
  build cannot read appears with the status `unreadable` and the reason in
  place of its title.
- Campaign (`/c/:key`) — one sheet: the plan's facts across the top, then
  score, unaided, repairs, regressions, time, spend, climb, and attempt phase
  per stack. Dependency campaigns add questline rows, which
  `?questlines=grid|graph|replay` switches between; `&step=N` moves the replay
  cursor. Sequential campaigns show one row pair per level instead.
- Attempt (`/c/:key/a/:attemptId`) — the attempt's figures, its climb, and
  `?tab=checks|screenshots|files|log`. The log tab follows new bytes.
- Plans (`/plans`) — the frozen campaign plans found under the plans directory,
  and the form that starts one.

## Modes

Inside the appliance (`STACK_BENCH_APPLIANCE=1`) the dashboard runs in
controller mode: the Plans form and the resume button post to the controller.
Elsewhere it runs read-only and those controls are unavailable;
`GET /api/health` reports which mode is active.

For UI development, `npm run dashboard` starts a read-only host view over
`tools/stack-bench/results`. Pass `--port` to move it off 7331 and `--results`
to point it at another results directory.

## Appliance

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

Open `http://127.0.0.1:7331`. Docker publishes that port only on the host's
loopback interface. Stop it with:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml --profile dashboard stop dashboard
```

Starting or resuming a run needs the separate dashboard control secret, typed
into the form. The server reads the expected value from the file configured by
`STACK_BENCH_DASHBOARD_CONTROL_SECRET_FILE` and no dashboard API returns it. A
wrong secret is answered with 403 and nothing is started. Starting a campaign
invokes the same `campaign run` command used by the CLI, so the CLI can inspect
or resume the result normally and CLI-started campaigns appear here.

## Routes

| route | returns |
| --- | --- |
| `GET /api/health` | `read-only` or `controller` |
| `GET /api/overview` | one summary per campaign |
| `GET /api/campaigns/:key` | the campaign sheet |
| `GET /api/campaigns/:key/progression` | the dependency graph and its replay |
| `GET /api/campaigns/:key/attempts/:id/checks` | per-check outcome and history |
| `GET /api/campaigns/:key/attempts/:id/package` | the evidence listing |
| `GET /api/campaigns/:key/attempts/:id/log?from=N` | log bytes after `N` |
| `GET /api/campaigns/:key/artifacts/:name` | one allowlisted artifact |
| `GET /api/events` | the change stream |
| `GET /api/plans` | the discovered plans |
| `POST /api/campaigns` | start a run |
| `POST /api/campaigns/:key/resume` | resume an interrupted dependency run |

Each payload covers one question, so opening a campaign or a tab is what pays
for reading it. The overview and the sheet are cached against the size and
modification time of the evidence they read, including while a campaign runs.

## The event stream

`GET /api/events` is a server-sent event stream. A `campaign` event names a
campaign whose plan, state, run output, or progression state changed; a `log`
event names an attempt whose stdout grew. Changes are debounced for 500 ms and
the stream sends a comment every 25 seconds so an idle connection stays open.
The client loads the overview once, then refetches only what an event names.
While the stream is down it falls back to polling the overview every 15
seconds.

The watcher uses a recursive `fs.watch` per campaign directory. Where the
platform or the mount does not support one it polls the same file fingerprints
every 5 seconds instead. The server logs which mode it opened with when the
first client subscribes.

## What it touches

It reads campaign plans from `<results>/plans` and campaigns from
`<results>/campaigns`. It writes nothing under a campaign directory. The only
file it appends to is `<results>/dashboard/operations.jsonl`, the record of
runs submitted through the dashboard, and the controller it starts writes its
own output under `<results>/dashboard/operations`.
