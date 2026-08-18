# Reproducible run setup

Stack Bench records the inputs needed to explain and compare a result. A run is
usable only when its requested scope passes preflight and its artifact identities
remain consistent through build, grading, repair, and reporting.

For a distributable deployment, use the dedicated Linux appliance described in
[`APPLIANCE-DESIGN.md`](APPLIANCE-DESIGN.md) and
[`appliance/README.md`](appliance/README.md). The commands below are also useful
for local development.

## Prerequisites

- Node.js 22 or newer
- Docker Engine and Docker Compose v2
- enough CPU, memory, disk, and free ports to pass exact-scope preflight
- Chromium installed through the pinned Playwright dependency
- a credential accepted by the selected coding-agent adapter
- for SpacetimeDB, the repository's Linux CLI and TypeScript bindings

Install the JavaScript dependencies and browser:

```bash
cd tools/stack-bench
npm ci
npm run bootstrap:browsers
```

Build the local coding image:

```bash
docker build -t stack-bench-build:2.1.226 container
```

On a Windows checkout, build the repository's Linux SpacetimeDB CLI before a
SpacetimeDB run:

```bash
bash container/build-linux-cli.sh
```

The supported appliance uses digest-pinned images from its release manifest.
The local image tag above is for development; preflight resolves the image that
will actually execute and records its immutable content ID.

## Coding-agent credentials

The selected agent adapter declares its accepted credential sources. The
current Claude Code adapter supports three mutually exclusive modes:

- a long-lived subscription token through `CLAUDE_CODE_OAUTH_TOKEN_FILE`;
- an API key through `ANTHROPIC_API_KEY` or the appliance secret-file mapping;
- the explicit local recovery mode at `~/.claude/.credentials.json`.

Select one mode. Conflicting sources fail closed. Secret values are not written
to run artifacts or Docker command arguments. The coding container can read the
credential selected for it, so production runs belong on a dedicated runner
without unrelated data or credentials.

## Validate before model spend

Run preflight for the exact stacks, track, levels, recipe, and image you intend
to execute. Include `--smoke` for the model-free container and network checks:

```bash
npm run preflight -- \
  --backend spacetime,postgres,mongodb \
  --track ecommerce \
  --levels 1-2 \
  --smoke
```

Preflight checks the selected recipe scope, Docker and platform support,
resources, ports, credentials, image identity, dependency availability,
outbound access, persistent result writes, and stack-specific runtime paths.
The benchmark command repeats admission checks and refuses before a coding
session when the requested environment is not ready.

Run the model-free container regression separately when changing container,
network, credential, or SpacetimeDB CLI behavior:

```bash
npm run test:container
```

## Configuration recorded with each run

`run.json` binds the result to the configuration that produced it, including:

- track, stack, recipe, selected packs/checks, level, and run index;
- agent adapter, model, effort, prompt treatment, and repair budget;
- engine, prompt, contract, scenario, recipe, and calibration identities;
- coding image reference and immutable image ID;
- stack adapter, database image, platform, and Node versions;
- SpacetimeDB CLI and TypeScript binding identities when selected;
- build and repair sessions, token usage, cost, turns, and model duration;
- grading evidence, media, lifecycle events, cleanup, and contamination status.

Campaign plans additionally freeze the attempt matrix, parallelism, pricing,
runtime images, and per-level gate policy. Dashboard and CLI execution consume
the same compiled plan.

## Isolation rules

Coding sessions run only in the build container. They receive the generated app
workspace, the dependencies declared by the stack adapter, their transcript
directory, and the selected credential. They do not receive the controller,
grader, scenarios, recipes, calibration, results, or source checkout.

The harness authenticates backend and container operations through the exact
run lease. It removes only resources whose recorded identities still match and
quarantines a run when cleanup ownership cannot be proven.

## Run and inspect

The common CLI flows are documented in [`README.md`](README.md). Use package
commands instead of importing internal module paths:

```bash
npm run bench -- --backend spacetime --track ecommerce --levels 1
npm run campaign -- status <campaign-directory>
npm run dashboard
```

The dashboard is an optional view and controller for the same campaign engine;
it does not replace the CLI or define a second run format.

## Comparing results

Compare runs with:

```bash
npm run compare -- <run-directory> <run-directory>
```

Only compare results that identify compatible engine, recipe, selection,
grader, image, model, effort, and prompt treatment. The comparison command
reports the common measured check set separately from checks that were not
measured in every run. Never treat a missing, inconclusive, contaminated, or
harness-failed check as a pass.

Raw artifacts are the source of truth. Generated summaries and dashboard views
must remain reproducible from those retained artifacts.
