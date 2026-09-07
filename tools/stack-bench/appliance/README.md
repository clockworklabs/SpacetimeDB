# Stack Bench appliance

The demo uses one Docker appliance. Docker runs the controller, coding sessions,
backends, browser grader, and package cache. The host needs Git and Docker; it
does not need Node, Rust, PostgreSQL, MongoDB, or local SpacetimeDB binaries.

## Run the demo

From the repository root of a clean checkout, with Docker running:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

When startup completes, open [localhost:7331](http://localhost:7331). Docker
cannot open the host browser for you. This command builds the images, prepares
state, starts the dashboard, and runs a model-free example: nine checks across
three reference stacks in parallel. No provider credential or model spend is
needed. Reference results demonstrate the runner; they do not measure a coding
agent's ability. The dashboard stays running after the example finishes. You can
stop its container in Docker Desktop; results remain in the state volume.

The bootstrap stores its resolved environment at
`/state/controller-home/demo.env` in the `stack-bench-state` volume. Campaign
evidence also stays in that volume. The first source build can take substantial
time. Requirements are below; manual setup and paid campaigns follow.

## Requirements

- Docker must run Linux containers for `linux/amd64`, with BuildKit and the
  Compose plugin. The current tool checks used Docker Engine 29.6.2 and Compose
  5.3.1. Other versions are not yet part of the release proof.
- Preflight requires 4 CPUs, 8 GiB total Docker memory allocation, and 10 GiB
  free result storage. This is a conservative startup policy, not a measured
  minimum for every workload. Choose parallel capacity for the available
  resources and the work being run.
- For the first source build, reserve additional disk space and build time.
  16 GiB RAM and 60 GiB free Docker storage are conservative planning allowances,
  not measured minimums. The release rehearsal must record actual build use.
- Use a clean, normal Git clone of the delivered branch. Release builds reject
  changed release inputs. A host Git worktree whose `.git` points outside the
  build context is not a release checkout.
- Docker needs internet access to the pinned base images, build packages,
  public npm registry, and selected model provider. Model work also needs a
  supported provider credential and a declared spend limit.

The controller uses the Docker socket to manage containers. Its state uses the
`stack-bench-state` named volume. The controller mounts that volume at the Docker
daemon's own mountpoint, so child bind mounts use the same path on Docker Desktop
and a Linux host. No host `/var/lib/stack-bench` directory is required.

For each release, verify the clean-branch image build and one-command demo on a
fresh machine. A passing source test alone does not prove the whole appliance.

## Advanced: manual setup and paid campaigns

From the repository root:

```sh
docker build --platform linux/amd64 -f tools/stack-bench/container/Dockerfile -t stack-bench-build:local .
docker build --platform linux/amd64 -f tools/stack-bench/appliance/Controller.Dockerfile -t stack-bench-controller:local .
docker run --rm --mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock stack-bench-controller:local setup > tools/stack-bench/operator.env
```

The controller build exports the clean Git revision, builds the native binaries
and SDK, and records their source and checksums in the image. It does not consume
ignored host binaries. BuildKit caches the Rust target and package downloads.
The first build can take substantial time; complete it before the demo.

Commands below are single lines so they can be pasted into PowerShell or a
POSIX shell. Docker must be running before `setup` or Compose commands.

`setup` creates the state volume and directories, resolves both local images to
immutable content IDs, and writes a UTF-8 Compose environment file. It installs
the pinned PostgreSQL and MongoDB images when they are absent. It also installs
four prepared plans and creates a dashboard control secret. It keeps existing
plans and secrets. Keep `operator.env` locally; it is ignored by Git. In Windows
PowerShell 5, use `| Out-File -Encoding utf8 tools/stack-bench/operator.env` instead
of `>` so the environment file is not UTF-16.

For a model-free check, no provider secret is needed. To configure model work,
write the subscription token through stdin:

```sh
docker run --rm -i --mount type=volume,source=stack-bench-state,target=/state stack-bench-controller:local set-secret claude_subscription_token
```

Supply the token on stdin and close input. For API billing, use secret name
`anthropic_api_key` and set `STACK_BENCH_AGENT_AUTH=api-key` in `operator.env`.
The secret stays in a private volume file; it is not a command argument or
part of the environment file. The Docker socket is not needed by `set-secret`.
After pasting the token and pressing Enter, close stdin with Ctrl+D in a POSIX
terminal, or Ctrl+Z followed by Enter in Windows PowerShell.

Run the remaining commands from `tools/stack-bench`. Compose uses the state
volume's results directory as its working directory, so `plans/...` and
`campaigns/...` refer to durable state inside Docker.

## Validate the appliance

Run commands from `tools/stack-bench` on the runner.

Check the Compose configuration:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml config --quiet
```

Run preflight for the exact planned scope:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller preflight --backend spacetime,postgres,mongodb --track ecommerce --levels 1 --run-index 0 --agent-adapter reference-fixture --guidance neutral
```

This preflight selects the model-free reference adapter and needs no provider
credentials. For model work, use the planned agent adapter and its configured
credential. This preflight verifies the runner, images, dependencies, ports,
and storage without creating an attempt. Each campaign attempt then runs its
own smoke check inside its activated private network, before the agent starts.
Standalone appliance preflight is read-only. The reference trial below exercises
the automatic smoke checks without model calls.

## Check the delivered runtime

This command starts real containers and grades the shipped reference app. It
makes no model calls. Run it before collecting model results:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller campaign trial plans/reference-check.json --out campaigns/reference-check
```

Inspect its status and evidence before using the appliance for model work.
A reference fixture pass checks the runtime path; it is not evidence that an
agent implemented the product.

## Inspect a campaign

To correct grading after a grader fix, use the saved execution in a separate
output directory. This runs no coding agent and has no provider cost:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller run --grade-from campaigns/example/attempts/attempt-id/execution-1 --out regrades/attempt-id
```

This path accepts a completed single-level sequential run. It verifies the
source checkpoint, keeps the original check scope and account aliases, and
rejects changes to the product request or contract. Use the original build
image and dependency bundle. Startup must reproduce the saved source without
changes. Repeat `--check <stable-check-id>` to regrade only affected checks from
the original scope. The separate `regrade.json`, grading bundle, and cleanup evidence do
not replace the original run or create another build sample. For saved dependency
candidates, select `--grade-level` and affected checks as described in the
[dependency replay method](../docs/grading-coverage.md#replay-a-saved-dependency-candidate).

The campaign file is the run authority. Store it below
`plans/` in the state volume.

`setup` installs `plans/paid-l1.json` from the supplied
[`campaign.paid-l1.json`](campaign.paid-l1.json), binds it to the local controller
and build image IDs, and freezes it for execution. It runs one fresh L1 build per stack,
three in parallel, with no repairs or retries and a $10 limit per attempt
($30 maximum across the three attempts). It uses Sonnet 5 and includes the
SpacetimeDB skills. Its results are provisional. Set
`STACK_BENCH_RUNNER_CAPACITY=3` in `operator.env` before running this plan.
Inspect its model, stacks, repetitions, spend limits, and pricing before launch.
For a longer study, [`campaign.paid-l1-l3.json`](campaign.paid-l1-l3.json) is a
draft three-stack progression pilot. It selects L1 through L3, six repairs total
per attempt, no execution retries, a 120-minute attempt limit, and a $30
per-attempt cap ($90 maximum). These are proposed limits, not a cost estimate.
It must be bound to the selected image identities, frozen, and installed under
`plans/` before execution; setup does not install it automatically. Earlier
levels must pass before later levels start. See the
[research roadmap](../docs/research-roadmap.md) for collection and analysis rules.
`appliance/campaign.example.json` is a model-free reference plan; changing its
title does not make it a coding-agent campaign.

Use a new manifest ID and output directory for a new comparison. Do not edit a
running campaign's plan. Setup preserves an existing `paid-l1.json`; it does not
silently replace or rebind it after an image rebuild. A frozen plan copied from
another machine binds that machine's image identities and cannot run unchanged.

Compile and inspect it without model work:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller campaign show plans/paid-l1.json
```

A test plan selects the model, stacks, work, checks, budgets, repetitions,
parallelism, pricing, controller image, and build image. When the run starts,
Stack Bench records these settings with the results. This prevents settings
from changing during a campaign.

The manifest also defines how repair work is selected and limited:

```json
"repair": { "selection": "feature", "budget": { "perFeature": 1 } }
```

Dependency mode supports `feature` or `batch` selection. The budget must name
at least one limit; each is a non-negative integer with no upper cap:

- `total`: repairs across the whole attempt. `0` runs the initial grade and
  advances passed branches without any repair.
- `perFeature`: repairs that may include one feature.
- `perDepth`: `{ "count": N, "carry": true | false }`. Each opened depth adds
  `count` repairs; `carry` keeps unused depth repairs available later.

When several features have failed, the next repair goes to the first of them
by dependency depth, then by `order`:

- `declared` (default): the order the catalog declares its features, which is
  part of the catalog's identity.
- `shuffled`: a permutation within each depth drawn once from the campaign's
  `ordering.seed` when the plan compiles, frozen in the plan as the policy's
  `nodeOrder`, and used by every stack in the campaign. The catalog and its
  qualification are unchanged; the policy identity carries the order.
When limits are combined, the tightest remaining limit wins, and the result
names which one stopped a feature: `feature-repairs-exhausted`,
`depth-repairs-exhausted`, `total-repairs-exhausted`, or `repeated-findings`
when the same failures survived the configured number of repairs. Set
`mode.unchangedFailureLimit` to a positive integer to change that stop (default 3). A completed
repair counts even when its grade did not finish; its source is kept beside
the run and graded on resume before any further coding session. Sequential
mode requires `batch` selection and one `total` limit.

The plan, dashboard, and report show qualification status. Publish scores as
verified comparison data only after every selected level is qualified.

`plans/reference-check.json` is the shipped zero-cost reference check. It uses
the reference adapter included in the controller image. `plans/ecommerce-progression.json`
keeps the full rigorous workload for reference validation; the short check does
not replace that workload. Both use hand-written reference apps and produce no
comparative model data.

## Run a campaign

Start the campaign. This command creates the run state and records the exact
test plan automatically:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller campaign run plans/paid-l1.json --out campaigns/campaign-001
```

The campaign controls attempt counts and concurrency. `repetitions` sets the
default attempt count per stack. A stack can override it. `parallelism` limits
simultaneous attempts. Each live attempt receives isolated ports, database
names, locks, workspaces, and evidence paths.

Coding, backend, browser, and broker containers share only their attempt's
private network namespace. Native firewall rules block host and other-attempt
connections. Concurrent native attempts and exact cleanup have passed the
bounded Docker checks. A clean appliance rehearsal remains a release gate.

Set `STACK_BENCH_RUNNER_CAPACITY` in `operator.env` to the total worker pool
across all campaigns. It defaults to 1 and accepts up to 21. Set each plan's
`parallelism` within that pool. Admission atomically reserves capacity and
ports; it refuses work when there are too few free workers. The startup baseline
remains 4 CPUs and 8 GiB RAM. Preflight also reports the configured pool's
container caps and warns when their sum exceeds Docker's total allocation.
These caps do not reserve CPU or RAM and are not measured hardware minimums.

Each worker has a 2-CPU/4-GiB coding container, a 1-CPU/1-GiB backend, a
1-CPU/1-GiB browser, and a broker capped at 256 MiB when needed. Thus nine
workers have known caps totaling 36 CPUs and 56.25 GiB RAM. Broker CPU,
controller processes, the package cache, and Docker need additional resources.

The shared package cache has caps of 1 CPU, 2 GiB RAM, and 128 processes. Allow
for its use alongside the controller and attempt containers.

Docker's reported memory is total allocation, not free memory. Contention can
increase run time; heavier apps can exhaust memory. Set the pool to the
intended parallelism and validate it with the selected workload. A short fixture
test does not establish capacity for arbitrary model builds or timed grading.

The 10-GiB disk check is a startup free-space check, not a per-worker reservation
or a storage quota. Concurrent installs, app builds, and retained evidence share
that storage. Keep space for their growth and for shared services.

An attempt holds its worker from its first build through its final grade and
cleanup. Results remain provisional until their grading qualification is current.

The remaining `campaign` snippets are controller subcommands. Run them after
the same Docker Compose `run --rm controller` prefix used above.

Use durable state for normal control:

```sh
campaign status <campaign-directory>
campaign stop <campaign-directory>
campaign inspect <campaign-directory>
campaign report <campaign-directory>
```

- `status` is the compact normal view.
- `stop` stops owned active work and retains its state and evidence.
- `inspect` adds score, cost, duration, cleanup, evidence, and feature progress.
- `report` rebuilds `report/report.json` and `report/report.html` from retained
  evidence.

Stop interrupts active attempts. A stopped sequential attempt remains invalid.
Running the same trial again starts only pending attempts; it does not restart
the interrupted attempt. Resume starts scheduled dependency work.

Do not infer state from logs. Use logs only to diagnose a reported phase or
failure. Automatic retries are limited by the manifest
`attemptPolicy`. Additional repair grants and budget extensions require an
explicit operator action.

## Resume and repair

If the controller stopped while an attempt remained live, reconcile ownership
before any resume:

```sh
campaign reconcile <campaign.json> --out <campaign-directory>
```

Reconciliation changes state only when private supervisor evidence proves that
the exact owned resources are clean.

Dependency campaigns can grant more repairs to selected exhausted features:

```sh
campaign grant-repairs <campaign-directory> --attempt <attempt-id> --grant-id <unique-id> --level <N> --feature <feature-id> --repairs <N>
```

The grant creates a linked continuation. It does not rewrite the completed
execution. Use `campaign resume <campaign.json> --out <campaign-directory>` to
run scheduled dependency work.

## Model-free trials and qualification

`campaign trial` accepts only registered non-billable adapters and zero pricing.
It validates orchestration but does not produce comparative model data.

Check qualification requirements without starting work:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller qualification status --track ecommerce --level <N>
```

Run only evidence required by that exact status. Do not repeat reference,
mutation, or null work when its bound inputs have not changed. See the
[reference app guide](../reference-apps/README.md) and
[grader guide](../grader/README.md) for qualification rules.

## Dashboard

Start the optional dashboard:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

Open `http://127.0.0.1:7331`. The dashboard reads the same campaign state as the
CLI. Reading results does not require provider credentials. Run controls launch
the Compose controller and check provider configuration at launch.

To use controls, read the generated control secret locally and enter it in the
dashboard:

```sh
docker run --rm --mount type=volume,source=stack-bench-state,target=/state,readonly --entrypoint cat stack-bench-controller:local /state/secrets/dashboard_control_secret
```

See [dashboard/README.md](../dashboard/README.md).

## Results and cleanup

Results remain in the `stack-bench-state` Docker volume after the controller exits.
Verify and copy the complete campaign package before deleting the runner.

For example, export the completed `campaign-001` to the host. Run from
`tools/stack-bench`; first create an empty local `results` directory if needed.
These commands copy evidence and remove only the temporary transfer container:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller campaign report campaigns/campaign-001
docker create --name stack-bench-result-transfer --mount type=volume,source=stack-bench-state,target=/state,readonly stack-bench-controller:local --help
docker cp stack-bench-result-transfer:/state/results/campaigns/campaign-001 results/campaign-001
docker rm stack-bench-result-transfer
```

Open `results/campaign-001/report/report.html` in a browser, or use the dashboard
to inspect checks, screenshots, logs, and cost evidence. The copied files remain
available when Docker is stopped.

To prepare a smaller research pack, use
`campaign export campaigns/campaign-001 --out exports/campaign-001` in the
controller. The destination parent must already exist; the destination itself
must be new and outside the campaign directory. The export contains the report,
attempt/execution CSV tables, and indexed public artifacts with verified hashes.
It is a partial copy: source, transcripts, media, and external evidence are
omitted, so links to those files do not work offline. Review free text before
sharing. Keep the complete original campaign as the durable internal archive.

A run removes only resources whose private ownership evidence still matches.
If cleanup cannot be proved, it preserves the evidence and quarantines the run.
Follow [RECOVERY.md](RECOVERY.md). Do not delete same-name resources or clear the
shared state root by guesswork.

Workspace cleanup requires the owned build container to remain running until
the controller stops its application processes and restores directory permissions.
Normal run completion then removes the temporary work directory. Early aborts
and interruptions retain that directory for inspection, with controller access
restored when handback succeeds. Preserve needed files before an operator removes
the exact retained directory. The controller does not sweep retained work.
If that container exits, runs out of memory, or is removed before handback, cleanup
retains the private lease and reports the failure. Preserve the result package
and private recovery state. For a stopped container, diagnose the exit and restore
that exact container before retrying authenticated recovery. A container removed
before handback requires manual workspace ownership repair; automatic recovery
continues to refuse because it cannot prove the handback. No background sweep
repairs this condition.
