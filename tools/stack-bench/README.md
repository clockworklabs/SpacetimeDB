# Stack Bench

A machine-verified benchmark for coding agents that build real-time
applications. Each run uses versioned product work, stack guidance, grading,
and budget inputs. The harness drives real clients, returns failed checks for
repair, and records score, cost, tokens, repair rounds, and wall time.

## Progression terms

- A **level** groups features at the same depth in the feature graph.
- A **repair round** is one coding session after a conclusive failed grade.
- A **strike** is spent when a conclusive grade fails a feature included in that
  coding request. Inconclusive grades and harness failures do not spend strikes.

Sequential mode requires the current level to pass before the next level can
start. Dependency mode treats each feature as a branch. A feature opens only
after its parent features pass. An exhausted feature blocks its children, but
other open branches continue. Earlier passed features are graded again so that
regressions remain visible.

Dependency campaigns default to `"repairSelection": "feature"`. Each repair
request then contains one failed feature. Use `"batch"` to repair all currently
failed features together. Initial builds still contain all features open at that
level. Both policies grade passed prerequisites again. Only failed features in
the repair request spend a strike. Sequential mode is unchanged and does not use
this setting.

Direct runs default to ten repair rounds. Campaign manifests set an explicit
budget. After a dependency run stops, the operator can grant additional
strikes to selected exhausted features. Stack Bench resumes from the saved
source checkpoint. It keeps the completed execution unchanged and records the
new work as a linked continuation. Campaign `retry` still means a fresh
execution.

```text
node dist/commands/campaign-cli.js grant-strikes <campaign-output> \
  --attempt <attempt-id> --grant-id <unique-id> --level <number> \
  --feature <feature-id> --strikes <number>
```

Stack adapters are interchangeable. The model and requested work can remain
fixed while the stack changes.

The optional local dashboard is another client of the same controller, not a
replacement for the CLI. It reads the same durable campaign artifacts and
submits the same bounded commands, so CLI-started work appears in the browser
and dashboard-started work remains fully operable from the CLI. See
`dashboard/README.md` for the Docker service.

## Documentation

- `SETUP.md` — local prerequisites and first-run setup
- `APPLIANCE-DESIGN.md` — appliance boundaries and execution model
- `appliance/README.md` — Docker appliance operation
- `dashboard/README.md` — optional web dashboard
- `grader/README.md` — grader architecture and evidence model
- `tracks/ecommerce/composition/README.md` — packs, recipes, and release composition
- `docs/dependency-graph.html` — generated ecommerce feature dependency graph
- `docs/stack-bench.html` - Stack Bench presentation

Working notes, generated reports, and run artifacts are local operator material
and are intentionally not tracked in the repository. Reviewable project
presentations belong in `docs/`.

## Source layout

| Path | Purpose |
|---|---|
| `commands/` | executable operator and harness commands |
| `src/actions/` | scenario action contracts and executors |
| `src/agents/` | coding-agent adapters and credentials |
| `src/campaigns/` | campaign compilation, scheduling, locking, and reports |
| `src/composition/` | tracks, packs, recipes, definitions, and calibration |
| `src/evidence/` | artifacts, scoring, provenance, and evidence states |
| `src/grading/` | grader execution and isolated check workers |
| `src/progression/` | feature graphs, dependency state, strikes, and progression scoring |
| `src/references/` | reference fixture selection and qualification |
| `src/releases/` | release source, bundle, and signature verification |
| `src/runtime/` | leases, containers, snapshots, recovery, and platform control |
| `src/stacks/` | stack adapter contracts and stack-specific operations |

Use the `npm run` commands documented below instead of depending on internal
module locations. `src/package-root.ts` is the single source of truth for the
repository and Stack Bench roots.

During development, run the smallest test or validator that covers the change.
After a shared runtime, composition, grading, campaign, or release change is
stable, run this integrated source gate once:

```bash
npm run lint
npm run typecheck
npm test
```

Documentation-only changes do not require the integrated gate. Run Docker
qualification only when a changed file affects its recorded scope. Run the
complete mutation set only for a release candidate.

The optional model-based SpacetimeDB behavioral review is separate from the
measured coding sessions. Run it deliberately with `--behavioral-review`; it is
off by default so campaign cost and token accounting never omit an unrecorded model
call. The model-free friction report remains automatic.

## Run it

The supported v1 deployment is the Docker appliance on a dedicated Linux
runner. Follow `appliance/README.md` to build or verify the controller image,
configure credentials, run preflight, and start a campaign.

### Local harness development

The commands in this section run from a source checkout. Use them to develop
and qualify the harness. They are not the distributable appliance workflow.

Install the locked harness dependencies and browser once per checkout:

```bash
cd tools/stack-bench
npm ci
npm run bootstrap:browsers
npm test
npm run check:prompts
npm run preflight -- --backend spacetime,postgres,mongodb --track ecommerce --levels 1-2 --smoke
npm run test:null
npm run test:container
```

`check:prompts` is model-free and Docker-free. It renders the actual L1-L3
dependency-mode requests for every packaged stack. It verifies their exact
bytes, confirms that the product request is the same across stacks, and rejects
language that tells the agent about grading or testing.

`preflight` says whether the exact requested run can start and gives a concrete
fix for each failure. It checks Docker/Compose, resource floors, image and
database identities, credential presence, selected agent materials, ports,
clock, storage, and inherited run state. `--smoke` uses no model: it starts the real build image, checks declared
outbound destinations, and proves its result-volume write survives on the host.
Every real benchmark runs that full smoke automatically before any model call
and stores the result as `preflight.json`; the explicit command is for fixing
the machine before launching a campaign.

`test:null` drives the selected grader against a reachable blank app and fails
if any point-bearing criterion passes or becomes inconclusive. It takes several
minutes because it uses the real browser suites, not fixture reports. Every
public track is included by default and the complete criterion evidence is
written under `results/`. A passing null control is only one input to release
qualification. It is not benchmark data.

```bash
npm run bench -- --backend spacetime --track ecommerce --levels 1-2
npm run bench -- --backend postgres  --track ecommerce --levels 1-2 --run-index 1
npm run bench -- --backend mongodb   --track ecommerce --levels 1-2 --run-index 2
npm run bench -- --backend postgres --track ecommerce --levels 1 \
  --pack ecommerce.feature.accounts \
  --check ecommerce.feature.accounts.accounts.1a
npm run bench -- --backend postgres --track ecommerce --levels 1 \
  --recipe ecommerce.sequential-l1@2.5.0

# Inspect an exhausted level, then grant at most four more repair rounds.
npm run repair -- status <run-directory> --level 1
npm run repair -- grant <run-directory> --level 1 --rounds 4 \
  --max-budget-usd 25 --timeout-minutes 120
```

A repair grant is accepted only when the parent has a complete, conclusive
application failure, an exhausted round budget, an intact level checkpoint,
and the exact current harness and adapter identities. A short setup session
installs dependencies and starts the saved app; it is timed and costed
separately from repair rounds. The setup session may not change source.
The controller then verifies the source bytes and reproduces the prior score,
test selection, denominator, and failed-criterion set before spending a
repair round. If any of those checks differ, the continuation stops.

Each grant creates `continuations/grant-<id>/` below its parent result. Its
`run.json` records the original run, immediate parent, grant size, rounds used,
cumulative rounds/cost/time, reproduced baseline, setup session, and new source
checkpoint. `process.json` records the bounded controller process and retained
logs. Repairing an earlier ladder level invalidates the meaning of later-level
results; those levels are listed explicitly as needing a fresh run and are not
charged to that earlier level's cumulative correction path.

`--pack` changes requested scope: the agent receives only global recipe framing
plus the selected packs' requirements and testing contracts. Declared pack
dependencies are included automatically and recorded as resolved task packs.
`--check` only narrows measurement inside that requested task; it never removes
requirements by itself, and a check outside explicitly selected packs is
rejected. With neither option, the current catalog recipe is requested and
graded.

`--recipe <id>@<version>` selects one exact non-retired catalogued release for a
single-level run. It uses the same preflight, agent, grader, artifact, null, and
qualification paths as the current default. Omitting it resolves the current
L1/L2 candidates. Selecting an exact release never changes a public label.

Give concurrent runs distinct `--run-index` values; ports and databases are
allocated from it. Results land under a unique run id inside
`results/<backend>-run<N>/`.

Public result JSON uses artifact schema v2. Each file records what kind of
evidence it contains, the attempt and parent attempt that produced it, start and
completion times, and every applicable engine, recipe, pack, fixture,
calibration, experiment, agent-adapter, and stack-adapter identity. Evidence
payload fields are checked by kind and files are replaced atomically. Active
readers accept only schema v2 and reject unknown fields, kinds, versions,
malformed hashes, and secret-bearing keys. Files outside the current artifact
schema are not interpreted as benchmark evidence. Operators may retain them
separately as local archival material.

### Cost evidence

`costUsd` is the normalized benchmark cost. The credential broker records the
provider usage and applies the pricing rates frozen in the campaign. This value
controls the benchmark budget. `calculatedCostUsd` is the receipt calculation
from the same recorded usage and rates. It must match the broker ledger.

`cliCostUsd` preserves the coding CLI's own cost value for comparison. It can
differ from the normalized benchmark cost and does not control the benchmark
budget. `costComplete: true` means that every recorded billable session has a
complete, reconciled receipt. Do not use a cost result when this field is false.

Every check records exactly one typed state: `passed`, `failed`, `inconclusive`,
or `harness_failure`. One shared status table drives scoring, run outcomes,
mutation/null controls, comparisons, repair eligibility, and console labels.
Diagnostic wording is only rendered or redacted for people; changing that prose
cannot turn missing evidence into a product failure or send a harness defect to
the repair agent.

The scenario action language is also startup-validated. Every registered action
declares a versioned input compiler, required capabilities, hard
deadline, evidence type, redaction tags, renderer metadata, and a narrow
executor boundary. Actions run through independent registered executors with
capability-scoped access; concurrency, browser lifecycle, backend/app control,
and direct database writes use the same typed contract as browser observations.

For local harness development, bring up the database services first. The
SpacetimeDB adapter starts its own dedicated run host:

```bash
docker compose -f tools/stack-bench/docker-compose.yaml up -d
```

Local development requires the Claude Code CLI, Node, and Docker. The services
use their own ports (6532 Postgres, 6537 MongoDB), container names, and volumes.
A run does not share their state with other services on the machine.

A run owns only what it starts, and stops it again when finished or interrupted.
A SpacetimeDB host that was already running belongs to whoever started it, so the
benchmark refuses to reuse or restart it. Use a dedicated `STACK_BENCH_STDB_URI`
whose explicit loopback port is free; the benchmark starts that host and stops it
at the end, or retains it for debugging with `--retain-backend`.

Coding sessions are selected through a statically registered agent adapter.
`claude-code` is the default; deterministic, fault-injection, and model-free
reference adapters exercise the same versioned request/result contract in
harness qualification. Select one with `--agent-adapter <id>`. Arbitrary
executable paths are not accepted as production adapters.

## Tracks

A track is one application the benchmark can build and grade. Everything
application-specific, including level prompts, the UI contract, scenario suites,
and the core flow the linter walks, lives under `tracks/<name>/`, declared by a
`track.json`. Adding an application is a matter of dropping in a directory; the
harness needs no change. Pick one with `--track` (default `chat`).

Tracks whose fixture must be recreated after a database reset set
`reseedOnReset`. Stack Bench restarts the application and verifies its public URL
before grading. The scored feature checks then verify the required data and behavior.

Composition authoring is read-only and does not require Docker:

```bash
npm run pack -- validate tracks/ecommerce/composition/packs/feature-accounts-1.1.0.json --track ecommerce
npm run recipe -- validate tracks/ecommerce/composition/recipes/sequential-l1-2.5.0.json --track ecommerce
npm run recipe -- show tracks/ecommerce/composition/recipes/sequential-l1-2.5.0.json --track ecommerce --pack ecommerce.feature.accounts
npm run recipe -- diff <old-recipe.json> <new-recipe.json> --track ecommerce
npm run graph
```

`npm run graph` rebuilds `docs/dependency-graph.html` from the versioned
ecommerce progression definition.

`recipe diff` reports meaning, scoring, fixture, execution, and metadata changes
separately, names requirement/contract fragments added or removed, then names the calibration bindings and evidence repetitions that
must be redone. `recipe show --pack` and `--check` produce a selected scope with
its own deterministic selection hash, bound to the source recipe hash. The same
flags on `npm run bench` run that scope. Packs and individual checks are combined as
a union. Run, bundle, and grade artifacts record the request, the exact checks
it resolved to, which checks were attempted, and any checks not run with their
reason. A subset can be an intentional benchmark run. Compare only results with
the same recipe and selection identities. Working notes and superseded local
artifacts are not part of the public source tree and must not be presented as
benchmark results.

Packs own ordered public requirement fragments, testing-hook fragments, and
their checks; recipes retain global framing and choose exact pack versions.
Removing a pack therefore removes its unique instructions and checks together.
Explicitly shared fragments are deduplicated only when their source slice,
order, modes, and bytes match exactly. A `--check` filter intentionally narrows
measurement without changing the task the app was built to satisfy. `recipe
show` displays that exact composed task and its independent hash.

| Track | Application | Why it exists |
|---|---|---|
| `chat` | rooms, messages, presence | baseline real-time collaboration workload |
| `ecommerce` | storefront, cart, warehouses | numeric shared state and contention workload |

Tracks are isolated by a port offset and a name slug, so two can run at the same
`--run-index` without colliding on ports, databases or result directories.

## Levels

The current ecommerce sequential definitions cover L1 through L3. All three are
candidates. No current qualification result is accepted. L1 contains 46 scored
checks. L2 contains 74 scored checks. Each scored L1 and L2 check has an exact
known-defect definition for all three supported stacks. Chat definitions remain
available through L2.

Every artifact records the exact levels and recipe that ran. A level without a
launchable catalog release fails instead of falling back to another level.

Ordered by the property each makes verifiable, not by feature novelty — see each
track's `LEVELS.md`.

| Level | Chat adds | Ecommerce adds | Makes verifiable |
|---|---|---|---|
| 1 | accounts + basic chat | storefront, cart, warehouses | identity, durable state, real-time |
| 2 | private rooms, membership | fulfilment, transfers, returns, pricing | authorization / multi-view consistency |
| 3 | reactions, polls, capacity | reservations and scheduled work | atomicity, isolation, deferred-work durability |
| 4 | scheduling, expiry | per-customer ranking and catalogue search | per-viewer derivation at catalogue scale |
| 5 | volume | volume | throughput, latency, efficiency |

Levels are cumulative: an app at L3 is still checked against L1 and L2, so a
regression is caught rather than scored around.

## How verification works

Apps differ in structure, so the harness locates elements only through a
contract of stable element IDs that the prompt requires (`tracks/<t>/contracts/`).
Scenarios (`tracks/<t>/scenarios/`) then drive real browser clients — one isolated
context per actor, so identities are genuinely separate — and assert on what a
user would observe.

Scoring groups are track-defined because the applications expose different
failure modes. Ecommerce uses features, invariants, contention, and systems
coverage; chat uses features, invariants, delivery, and systems coverage. The
compiled recipe—not a hard-coded universal axis list—is the source of truth for
which checks and points apply to a run.

The separation matters: a feature score cannot detect cross-cutting failures
such as broken ownership, durability, transaction boundaries, or reconnect
recovery.

Scoring is the exact sum of passed checks. Console errors remain visible diagnostics,
but do not silently change unrelated check scores. Failed or unavailable checks earn
zero without changing the declared denominator. The grader does not reload a failed
live-update assertion and retry it, so "real-time" means real-time.

## Files

| Path | Purpose |
|---|---|
| `commands/bench.ts` | runs one benchmark attempt |
| `commands/agent.ts` | drives one coding session (build, upgrade, or repair) |
| `commands/run-suite.ts` | resets and grades one prepared app |
| `commands/report-bugs.ts` | turns failed checks into a repair report |
| `grader/grade.ts` | executes scenarios against real clients |
| `grader/mutation-test.ts` | validates checks with known defects |
| `linter/lint.ts` | checks the app exposes the required test ids |
| `docker-compose.yaml` | the Postgres and MongoDB services |
| `appliance/` | dedicated Linux runner controller image, Compose bundle, and operator guide |
| `reset-db.sh`, `restart-backend.sh` | environment control used by the suites |
| `src/composition/tracks.ts` | resolves a track: its paths, suites, ports and names |
| `tracks/<name>/` | one application: prompts, contracts, scenarios, lint walk |
| `backends/` | per-backend setup and deploy instructions given to the agent |
## Validation safeguards

- **Reference qualification.** Every scored recipe must pass against its exact,
  source-bound reference implementation before promotion.
- **Mutation testing.** `dist/grader/mutation-test.js` injects declared defects and
  requires the intended criterion to fail conclusively without unrelated
  regressions. Setup and infrastructure failures do not count as detections.
  `npm run check:mutations -- --app <reference-app> --mutations <manifest>`
  verifies every source edit is present exactly once before a Docker run.
  During development, validate only the mutation definitions affected by the
  change. The complete mutation set is a release-candidate gate. Live full-set
  qualification requires the explicit `--release-candidate` option. A targeted
  live check uses `--mutation-id <id>` and cannot become promotion evidence.
- **Null controls.** A blank application must not earn points or produce
  inconclusive scored evidence.
- **State isolation.** The database is reset before each suite, and each run
  receives distinct ports, database names, leases, and result paths.
- **Fail-closed evidence.** Missing, malformed, mismatched, or inconclusive
  evidence cannot become a passing score.

## What a run records

Evidence emitted into the result directory and `<app>/stack-bench/` includes:

| Artifact | What it is |
|---|---|
| `preflight.json` | exact environment admission checks completed before model spend |
| `bundle.json` | scores per suite, code metrics, environment checks |
| `grading-<suite>.json` | every criterion's typed verdict and structured evidence |
| `contract-lint.json` | which test ids resolved |
| `media/*.webm` | one video per actor per feature — what each user saw |
| `media/*.png` | full-page screenshot at the exact moment an assertion failed |
| `media/*.trace.zip` | Playwright trace: steppable, with DOM snapshots and network |
| `records/bug-report-l<N>-round<M>.md` | what the agent was told each repair round |
| `run.json` | exact stack, model, recipe, test pack, prompt identity, image, repair budget, outcome, usage, and timing for the run |
| `level-l<N>-checkpoint.json` | strict parent-linked identity for the source accepted at the end of a level |
| `level-l<N>-source/` | source-only level checkpoint; dependencies, build output, and grading evidence are excluded |
| `continuations/grant-<id>/run.json` | immutable child result for one finite post-run correction grant, including reproduced baseline and cumulative effort |
| `continuations/grant-<id>/process.json` | bounded continuation-process outcome plus retained stdout/stderr identities |

Recording is on by default; `--no-media` turns it off for a quick check. Watching
the failing actor's video is the fastest way to confirm a verdict is real before
reporting it, and the recordings are the evidence published alongside results.

## Watching a run

Recorded videos carry an annotation banner showing which actor is being driven,
the feature and criterion under test, and the step in progress — so a recording
explains itself rather than needing the log beside it. The banner turns green on
a passing criterion and red on a failure, with the failure message, immediately
before the screenshot is captured.

It is injected outside the app's root and carries no test id, so scoped
assertions cannot see it and it cannot affect a score.
