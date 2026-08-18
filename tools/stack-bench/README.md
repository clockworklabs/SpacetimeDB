# Stack Bench

A machine-verified benchmark for LLM agents building real-time backend
applications. The agent builds an app from a fixed prompt; the harness verifies
it by driving real clients, hands back anything that failed, and lets the agent
fix it — recording score, cost, tokens, fix rounds and wall time.

Correction rounds are a declared budget, not a promise that the first retry
will improve the score. The harness keeps trying through a flat or rejected
repair until the budget is used, while rolling back changes that lose evidence
or make the score worse. Results record whether correction was unnecessary,
succeeded, or exhausted its budget. Successful correction cost and unresolved
correction spend are reported separately.

Direct runs default to ten correction rounds; campaign manifests bind their own
explicit budget. A multi-level run advances only after the current level passes.
If L1 still fails after its budget, the run preserves its checkpoint, records L2
as blocked, and stops for operator review. The operator can then grant a finite
additional repair budget, correct a benchmark or environment defect, or start a
fresh run. An unresolved lower level is never silently carried into the next one.

The accepted source at the end of every level is also preserved and hash-bound
to the run. After a level conclusively fails and uses its declared correction
budget, an operator can grant a finite number of additional rounds from that
exact checkpoint. The new work is stored as a linked continuation; the original
result is never rewritten. Campaign `retry` still means a fresh execution.

Backends are interchangeable, so the model can be held fixed while the backend
varies.

The optional local dashboard is another client of the same controller, not a
replacement for the CLI. It reads the same durable campaign artifacts and
submits the same bounded commands, so CLI-started work appears in the browser
and dashboard-started work remains fully operable from the CLI. See
`dashboard/README.md` for the Docker service.

## Documentation

- `SETUP.md` — local prerequisites and first-run setup
- `APPLIANCE-DESIGN.md` — appliance boundaries and execution model
- `CONTAINER-DESIGN.md` — container topology and isolation model
- `appliance/README.md` — Docker appliance operation
- `dashboard/README.md` — optional web control room
- `grader/README.md` — grader architecture and evidence model
- `tracks/ecommerce/composition/README.md` — packs, recipes, and release composition

Working notes, generated reports, presentations, and run artifacts are local
operator material and are intentionally not tracked in the repository.

The optional model-based SpacetimeDB behavioral review is separate from the
measured coding sessions. Run it deliberately with `--behavioral-review`; it is
off by default so campaign cost and token accounting never omit an unrecorded model
call. The model-free friction report remains automatic.

## Run it

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

`check:prompts` is model-free and Docker-free. It renders the actual L1/L2
build, upgrade, and fix prompts for every packaged stack under prescribed and
neutral guidance, then compares their exact bytes with the reviewed appliance
snapshot. If an intentional prompt change occurs, inspect the rendered prompts
with `commands/agent.mjs --print-prompt` before refreshing the snapshot with
`node commands/prompt-snapshot.mjs --write`.

`preflight` says whether the exact requested run can start and gives a concrete
fix for each failure. It checks Docker/Compose, resource floors, image and
database identities, credential presence, selected agent materials, ports,
clock, storage, and inherited run state. `--smoke` uses no model: it starts the real build image, checks declared
outbound destinations, and proves its result-volume write survives on the host.
Every real benchmark runs that full smoke automatically before any model call
and stores the result as `preflight.json`; the explicit command is for fixing
the machine before launching a campaign.

`test:null` drives the complete validated L1-L2 grader against a reachable
blank app and fails if any point-bearing criterion passes or becomes
inconclusive. It takes several minutes because it uses the real browser suites,
not fixture reports. Every public track is included by default and the complete
criterion evidence is written under `results/`.

```bash
npm run bench -- --backend spacetime --levels 1-2
npm run bench -- --backend postgres  --levels 1-2 --run-index 1
npm run bench -- --backend mongodb   --levels 1-2 --run-index 2
npm run bench -- --backend postgres --track ecommerce --levels 1 \
  --pack ecommerce.identity-access --check <stable-check-key>
npm run bench -- --backend postgres --track ecommerce --levels 1 \
  --recipe ecommerce.l1-standard@1.1.0

# Inspect an exhausted level, then grant at most four more correction rounds.
npm run repair -- status <run-directory> --level 1
npm run repair -- grant <run-directory> --level 1 --rounds 4 \
  --max-budget-usd 25 --timeout-minutes 120
```

A repair grant is accepted only when the parent has a complete, conclusive
application failure, an exhausted round budget, an intact level checkpoint,
and the exact current harness and adapter identities. A short setup session
installs dependencies and starts the saved app; it is timed and costed
separately from correction rounds. The setup session may not change source.
The controller then verifies the source bytes and reproduces the prior score,
test selection, denominator, and failed-criterion set before spending a
correction round. If any of those checks differ, the continuation stops.

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
rejected. With neither option, the complete promoted recipe is requested and
graded.

`--recipe <id>@<version>` selects one exact non-retired catalogued release for a
single-level run. It uses the same preflight, agent, grader, artifact, null, and
qualification paths as the promoted default. Omitting it continues to resolve
the promoted L1/L2 aliases; selecting an exact release never moves a public label.

Give concurrent runs distinct `--run-index` values; ports and databases are
allocated from it. Results land under a unique run id inside
`results/<backend>-run<N>/`.

Public result JSON uses artifact schema v2. Each file records what kind of
evidence it contains, the attempt and parent attempt that produced it, start and
completion times, and every applicable engine, recipe, pack, fixture,
calibration, experiment, agent-adapter, and stack-adapter identity. Evidence
payload fields are checked by kind and files are replaced atomically. Active
readers accept only schema v2 and reject unknown fields, kinds, versions,
malformed hashes, and secret-bearing keys. Pre-v1 result bytes are preserved in
a checksummed inert archive, not interpreted as current evidence.

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

Bring up the databases first; the SpacetimeDB backend needs `spacetime start`
instead:

```bash
docker compose -f tools/stack-bench/docker-compose.yaml up -d
```

Requires the Claude Code CLI, Node and Docker. The services use their own ports
(6532 Postgres, 6537 MongoDB), container names and volumes, so a run never shares
state with anything else on the machine.

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
application-specific — level prompts, the UI contract, the scenario suites and
the golden path the linter walks — lives under `tracks/<name>/`, declared by a
`track.json`. Adding an application is a matter of dropping in a directory; the
harness needs no change. Pick one with `--track` (default `chat`).

Composition authoring is read-only and does not require Docker:

```bash
npm run pack -- validate tracks/ecommerce/composition/packs/identity-access-1.0.0.json --track ecommerce
npm run recipe -- validate tracks/ecommerce/composition/recipes/l1-standard-1.0.0.json --track ecommerce
npm run recipe -- show tracks/ecommerce/composition/recipes/smoke-1.0.0.json --track ecommerce --pack ecommerce.identity-access
npm run recipe -- diff <old-recipe.json> <new-recipe.json> --track ecommerce
```

`recipe diff` reports meaning, scoring, fixture, execution, and metadata changes
separately, names requirement/contract fragments added or removed, then names the calibration bindings and evidence repetitions that
must be redone. `recipe show --pack` and `--check` produce a selected scope with
its own deterministic selection hash, bound to the source recipe hash. The same
flags on `npm run bench` run that scope. Packs and individual checks are combined as
a union. Run, bundle, and grade artifacts record the request, the exact checks
it resolved to, which checks were attempted, and any checks not run with their
reason. A subset can be an intentional benchmark run. `npm run compare` refuses
different recipe or selection identities and refuses any run whose scope cannot
be proven. Pre-v1 results are preserved under `archive/pre-v1/` for historical
inspection only; active readers do not infer or migrate their meaning.

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

`tracks/ecommerce/overview.html` is a self-contained explainer of that track —
what gets built, how it is graded, and how the benchmark is kept honest — written
for a reader who does not work on the harness.

## Levels

The five-level ladder below is the production target. Both tracks are currently
validated through **L2**. A run may choose any declared level; its artifact says
exactly which levels ran and separately records the track's current
`validatedThrough` boundary. A level with no declared suite fails instead of
falling back to L1 grading.

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
contract of `data-testid` attributes the prompt requires (`tracks/<t>/contracts/`).
Scenarios (`tracks/<t>/scenarios/`) then drive real browser clients — one isolated
context per actor, so identities are genuinely separate — and assert on what a
user would observe.

Three scored axes, kept apart because a feature score cannot see cross-cutting
properties. An app can implement every listed feature and still let one user take
over another's account:

- **Features** — does each described feature work
- **Invariants** — identity, isolation, durability, write-path integrity
- **Delivery** — no loss, no duplication, consistent ordering, reconnect recovery

Scoring rules are enforced in code: console errors cap a feature at 2, a feature
that only works after a reload caps at 1, and an unreachable feature scores 0.
The grader never reloads except to probe for that, so "real-time" means real-time.

## Files

| Path | Purpose |
|---|---|
| `commands/bench.mjs` | runs everything for one backend, unattended |
| `commands/agent.mjs` | drives one headless coding session (build, upgrade, fix) |
| `commands/run-suite.mjs` | grades one app: reset, lint, then each suite |
| `commands/report-bugs.mjs` | turns failed checks into a repair report for the agent |
| `grader/grade.mjs` | executes scenarios against real clients |
| `grader/mutation-test.mjs` | validates the grader by injecting known defects |
| `linter/lint.mjs` | checks the app exposes the contract's test ids |
| `docker-compose.yaml` | the Postgres and MongoDB services |
| `appliance/` | dedicated Linux runner controller image, Compose bundle, and operator guide |
| `reset-db.sh`, `restart-backend.sh` | environment control used by the suites |
| `src/composition/tracks.mjs` | resolves a track: its paths, suites, ports and names |
| `tracks/<name>/` | one application: prompts, contracts, scenarios, lint walk |
| `backends/` | per-backend setup and deploy instructions given to the agent |
## Validation safeguards

- **Reference qualification.** Every scored recipe must pass against its exact,
  source-bound reference implementation before promotion.
- **Mutation testing.** `grader/mutation-test.mjs` injects declared defects and
  requires the intended criterion to fail conclusively without unrelated
  regressions. Setup and infrastructure failures do not count as detections.
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
| `records/bug-report-l<N>-round<M>.md` | what the agent was told each fix round |
| `.session-<mode>-l<N>.json` | the agent session: cost, tokens, duration, final message |
| `.prompt-<mode>-l<N>.md` | the exact prompt the agent received |
| `run.json` | exact stack, model, recipe, test-pack, prompt, image, repair budget, outcome, usage, and timing for the run |
| `level-l<N>-checkpoint.json` | strict parent-linked identity for the source accepted at the end of a level |
| `level-l<N>-source/` | source-only level checkpoint; dependencies, build output, prompts, sessions, and grading evidence are excluded |
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
