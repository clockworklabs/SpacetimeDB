# StackBench appliance

StackBench runs as one Docker appliance. Docker runs the controller, the coding
sessions, the databases, the grading browser, and a package cache. The host needs
Git and Docker and nothing else.

Host requirements and the demo are in [Getting started](../GETTING-STARTED.md).
The controller manages containers through the Docker socket and keeps all state
in the `stack-bench-state` volume; its isolation is described in
[system design](../docs/system-design.md#appliance-isolation).

## Set up for model runs

Every command from here on runs from `tools/stack-bench` and needs
`operator.env`, the Compose environment file that `setup` writes.

If you ran the demo, copy its saved setup:

```sh
docker run --rm --mount type=volume,source=stack-bench-state,target=/state,readonly --entrypoint cat stack-bench-controller:local /state/controller-home/demo.env > operator.env
```

For a new installation without the demo, build the images and run setup from
the repository root:

```sh
docker build --platform linux/amd64 -t stack-bench-build:local tools/stack-bench/container
docker build --platform linux/amd64 --no-cache-filter=source -f tools/stack-bench/appliance/Controller.Dockerfile -t stack-bench-controller:local .
docker run --rm --mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock stack-bench-controller:local setup > tools/stack-bench/operator.env
```

In Windows PowerShell 5, replace `> file` with `| Out-File -Encoding utf8 file`,
or the file is written as UTF-16.

`setup` creates the volume, pins the two local images by content ID, pulls the
database images, installs the prepared plans, and keeps any existing plans and
credentials. `operator.env` is ignored by Git.

`controller` below means this Compose prefix:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller
```

### Provider credentials

Secrets are read from the terminal and stored in the volume. They never appear
in a command line, the environment file, or a plan:

```sh
docker run --rm -it --mount type=volume,source=stack-bench-state,target=/state stack-bench-controller:local set-secret <secret-name>
```

Then set `STACK_BENCH_AGENT_AUTH` in `operator.env` to the billing mode.
Each adapter has an API-key mode and an account (subscription) mode. There is
no fallback between them.

| Adapter | Mode | `STACK_BENCH_AGENT_AUTH` | Secret name and source |
| --- | --- | --- | --- |
| `claude-code` | API key | `api-key` | `anthropic_api_key` |
| `claude-code` | Claude plan | `subscription-token` (default) | `claude_subscription_token` |
| `codex` | API key | `openai-api-key` | `openai_api_key` |
| `codex` | ChatGPT plan | `openai-account` | `codex_auth`: the `auth.json` written by `codex login` |
| `grok-build` | API key | `xai-api-key` | `xai_api_key` |
| `grok-build` | Grok plan | `grok-account` | `grok_auth`: `~/.grok/auth.json` written by `grok login --device-auth` |
| `antigravity` | API key | `gemini-api-key` | `gemini_api_key` |
| `antigravity` | Google plan | `agy-account` | `agy_auth`: see below |
| `openrouter` | API key | `openrouter-api-key` | `openrouter_api_key`; also set `STACK_BENCH_OPENROUTER_API_KEY_FILE` |

Pipe a file into `set-secret` with `docker run -i` instead of `-it`. For a
Google plan, sign in to agy in its own volume, then store the token:

```sh
docker run --rm -it -v stack-bench-agy-login:/root --entrypoint agy stack-bench-build:local
docker run --rm -v stack-bench-agy-login:/login:ro alpine cat /login/.gemini/antigravity-cli/antigravity-oauth-token | docker run --rm -i --mount type=volume,source=stack-bench-state,target=/state stack-bench-controller:local set-secret agy_auth
```

Account tokens: the controller renews Grok and Google sign-ins before and
during a session. It does not renew ChatGPT tokens; if one expires, run
`codex login` again and store the new file. Campaigns record the signed-in
account, not the token.

OpenRouter needs a model with the Responses API and local tools. Each campaign
agent entry must fix `model`, `providerRoute`, and `maxOutputTokens`; the broker
uses that one route and refuses fallbacks.

Cost is model usage priced at the plan's recorded rates from the token counts
the provider reports. It is a comparison cost, not an invoice. Provider
features whose charges the broker cannot price, such as hosted tools or image
input, are forwarded and marked unknown, and the run's cost becomes a lower
bound. The spending cap applies to priced spend only.

### Named credential profiles

Jobs can pick an account per adapter or per attempt instead of using the
environment. Set `STACK_BENCH_CREDENTIAL_PROFILES_FILE` to a JSON file in
controller storage:

```json
{
  "claude-work": { "provider": "anthropic", "mode": "subscription-token", "secretFile": "/state/secrets/claude-work", "version": "v1" },
  "openai-api":  { "provider": "openai",    "mode": "api-key",            "secretFile": "/state/secrets/openai-api",  "version": "v1" }
}
```

`mode` is `api-key` or `subscription-token`, the same two modes as the table
above. A job references profiles by ID: a `default`, an `adapters` map, and an
`attempts` map keyed by attempt ID, with the most specific winning. Evidence
records the profile ID, version, provider, and mode, never the secret. Bump
`version` when you replace a secret, and never overwrite one that an active
attempt is using.

## Validate the appliance

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml config --quiet
controller preflight --backend spacetime,postgres,mongodb --track ecommerce --levels 1 --run-index 0 --agent-adapter reference-fixture --guidance neutral
controller campaign trial plans/reference-check.json --out campaigns/reference-check
```

Preflight checks images, dependencies, ports, and storage without starting
anything. The trial grades the shipped reference app in real containers with no
model calls. Run both before collecting model results.

## Plans

A plan fixes everything a campaign needs: model, stacks, features, checks,
budgets, repetitions, parallelism, pricing, and the exact image IDs. Plans live
under `plans/` in the state volume. Setup installs `plans/paid-l1.json` from
[`campaign.paid-l1.json`](campaign.paid-l1.json); inspect it before launch:

```sh
controller campaign show plans/paid-l1.json
```

Use a new plan ID and output directory for each comparison, and never edit a
running campaign's plan. A plan frozen on another machine binds that machine's
image IDs and will not run here.

Plan fields that shape a dependency campaign:

- `mode.workSelection`: how much ready work each session gets. `feature` (one),
  `progressive` (every ready feature, the default), or `all-at-once`.
- `repair.selection` (`feature` or `batch`) and `repair.budget`: `total`,
  `perFeature`, or `perDepth: { count, carry }`. `total: 0` means no repairs.
- `order`: `declared` (the catalog's order) or `shuffled` (a per-depth
  permutation drawn from `ordering.seed` when the plan compiles).
- `mode.unchangedFailureLimit` (default 3): a feature whose repairs keep
  producing the same failures stops after this many observations.
- `budgets.attemptTimeoutMinutes`: working time per attempt, excluding planned
  depth pauses and waits for a grading slot.

## Run a campaign

```sh
controller campaign run plans/paid-l1.json --out campaigns/campaign-001
controller campaign status campaigns/campaign-001
controller campaign inspect campaigns/campaign-001
controller campaign stop campaigns/campaign-001
controller campaign resume plans/paid-l1.json --out campaigns/campaign-001
controller campaign report campaigns/campaign-001
```

`run` exits when every attempt is finished. `stop` ends active work and keeps
its evidence. `resume` continues pending attempts. `report` rebuilds
`report/report.html` from the evidence. The step-by-step version is the
[run-campaign skill](../skills/run-campaign/SKILL.md).

### Capacity

`parallelism` in the plan is the number of attempts that run at once. Each
attempt gets a coding container (2 CPUs, 4 GiB), a database container (1 CPU,
2 GiB), a browser container (1 CPU, 4 GiB), and a broker (256 MiB). The shared
package cache has 1 CPU and 2 GiB. These are caps, not reservations.

With `STACK_BENCH_RUNNER_CAPACITY=dynamic`, the default, admission checks free
memory and CPU before each launch and reserves 2.5 GiB for the new attempt's
first minute. Dashboard runs and jobs wait when the host is busy. CLI `campaign
run`, `trial`, `resume`, and `extend` do not: they stop launching, let active
attempts finish, and exit 2. Set the variable to a number for a fixed quota.

### Continue a campaign

- A session waiting on a rate-limited provider: inspect it with
  `campaign continuation-status <dir> --attempt <id> --json`, fix the provider
  account, then authorize one continuation with
  `campaign continue-provider <dir> --attempt <id> --request-id <unique-id>`.
  The wait uses the attempt's time allowance.
- More time: `campaign grant-time <dir> --attempt <id> --grant-id <unique-id> --minutes 120`.
  It does not change the plan, the cost cap, or the repair allowance. A stopped
  attempt can take a grant only at a completed-depth checkpoint; then
  `campaign resume`.
- More repairs for a dependency campaign:
  `campaign grant-repairs <dir> --attempt <id> --grant-id <unique-id> --level <N> --feature <id> --repairs <N>`,
  then `campaign resume`.
- A pause between depths in one live execution: set `mode.pauseAfterDepth` in
  the plan. Attempts wait after that depth; `campaign pause-status <dir>`
  shows them and `campaign continue-depth <dir>` releases the whole cohort. The
  controller must stay running; a controller loss ends the pause and needs
  recovery.
- A new campaign seeded from a finished one's source at a given depth:
  `campaign extend <plan.json> --from <parent-dir> --depth 2 --out <dir> --prepare-only`,
  review, then `campaign run`. Stack, model, repetition, guidance, and repair
  settings must match the parent. The result is labelled a seeded continuation
  and its cost excludes the parent's.

### Regrade a saved attempt

After a grader fix, grade a saved execution again without running the agent:

```sh
controller run --grade-from campaigns/<campaign>/attempts/<attempt>/execution-1 --out regrades/<attempt>
```

Add `--check <stable-check-id>` for each check to regrade, and `--grade-level
<N>` for a dependency candidate. The regrade is separate evidence; it does not
replace the original run or count as a new build.

## Jobs

A job runs one whole campaign on one host with named credentials. Submit a file
like this:

```json
{ "key": "release-42-l2", "planFile": "l2.json",
  "credentials": { "adapters": { "claude-code": "claude-work", "codex": "openai-api" } },
  "hostId": "worker-east", "capacityPolicy": "wait" }
```

```sh
controller job submit submission.json
controller job status <id>
controller job list --limit 50
controller job work <id> --host worker-east
controller job resume <id>
controller job cancel <id>
controller job worker --host worker-east --concurrency 2
```

The same `key` with the same request returns the same job; with a different
request it fails. `work` claims and runs one job. `worker` keeps claiming queued
jobs for its host, up to `concurrency` campaigns at once, and drains on SIGTERM.
The `worker` Compose profile runs it with `STACK_BENCH_HOST_ID` and
`STACK_BENCH_JOB_CONCURRENCY`. `capacityPolicy` is `wait` or `fail`.

Claims do not expire, because a worker that lost contact may still have paid
requests in flight. A dead worker leaves a claim to inspect; `job resume` can
continue it once campaign reconciliation proves cleanup.

The dashboard exposes the same operations at `/api/jobs` and the run setup at
`/api/run-setup`, `/api/runs/prepare`, and `/api/runs`. The CLI equivalents are
`job options`, `job prepare <selections.json>`, and `job start <review.json>`.
This is a local, single-host queue. It does not distribute one campaign's
attempts across hosts or share provider quotas.

## Upload to StackBench

Report a finished, frozen dependency campaign, then publish it to a file or to
the site:

```sh
controller campaign report campaigns/campaign-001
controller campaign publish campaigns/campaign-001 --suite ecommerce-1 --out upload.json
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm -e STACKBENCH_UPLOAD_KEY controller campaign publish campaigns/campaign-001 --suite ecommerce-1 --to https://stackbench.org
```

`--exclude <attempt-id>=<reason>` marks an attempt excluded; it is still
uploaded with the reason. The site needs a suite whose feature catalog matches
the campaign. Uploads are private until an admin publishes them. Active or
unfinished attempts block publication, and changed evidence needs a new report.

## Results and cleanup

Results stay in the `stack-bench-state` volume. Copy a campaign out and verify
it with the [archive-evidence skill](../skills/archive-evidence/SKILL.md) before
removing anything. `campaign export <dir> --out exports/<name>` writes a smaller
research pack (report, CSV tables, hashed public artifacts) without source,
transcripts, or media.

Cleanup removes only resources whose ownership it can prove. If it cannot, it
keeps the evidence and marks the run quarantined. Follow the
[recover-run skill](../skills/recover-run/SKILL.md); do not delete containers or
clear the state root by hand.

Qualification, which proves the checks before results are published, is the
[qualify-checks skill](../skills/qualify-checks/SKILL.md). Building a signed
release is the [release skill](../skills/release/SKILL.md).

## Dashboard

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

It serves `localhost:7331` on the loopback interface only and reads the same
state as the CLI. See [dashboard/README.md](../dashboard/README.md).
