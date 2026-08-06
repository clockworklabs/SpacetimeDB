# Stack Bench

A machine-verified benchmark for LLM agents building real-time backend
applications. The agent builds an app from a fixed prompt; the harness verifies
it by driving real clients, hands back anything that failed, and lets the agent
fix it — recording score, cost, tokens, fix rounds and wall time.

Backends are interchangeable, so the model can be held fixed while the backend
varies.

## Run it

```bash
node bench.mjs --backend spacetime --levels 1-5
node bench.mjs --backend postgres  --levels 1-5 --run-index 1
node bench.mjs --backend mongodb   --levels 1-5 --run-index 2
```

Give concurrent runs distinct `--run-index` values; ports and databases are
allocated from it. Results land in `results/<backend>-run<N>/run.json`.

Bring up the databases first; the SpacetimeDB backend needs `spacetime start`
instead:

```bash
docker compose -f tools/stack-bench/docker-compose.yaml up -d
```

Requires the Claude Code CLI, Node and Docker. The services use their own ports
(6532 Postgres, 6537 MongoDB), container names and volumes, so a run never shares
state with anything else on the machine.

A run owns only what it starts, and stops it again when finished or interrupted.
A SpacetimeDB host that was already running belongs to whoever started it — other
databases live there — so it is used as-is and left alone. If none is running, one
is started and stopped again at the end, or kept with `--keep-spacetime`.

## Tracks

A track is one application the benchmark can build and grade. Everything
application-specific — level prompts, the UI contract, the scenario suites and
the golden path the linter walks — lives under `tracks/<name>/`, declared by a
`track.json`. Adding an application is a matter of dropping in a directory; the
harness needs no change. Pick one with `--track` (default `chat`).

| Track | Application | Why it exists |
|---|---|---|
| `chat` | rooms, messages, presence | the original; history and human grades exist for it |
| `ecommerce` | storefront, cart, warehouses | live derived numbers and contention, which chat saturated on |

Tracks are isolated by a port offset and a name slug, so two can run at the same
`--run-index` without colliding on ports, databases or result directories.

`tracks/ecommerce/overview.html` is a self-contained explainer of that track —
what gets built, how it is graded, and how the benchmark is kept honest — written
for a reader who does not work on the harness.

## Levels

Ordered by the property each makes verifiable, not by feature novelty — see each
track's `LEVELS.md`.

| Level | Chat adds | Ecommerce adds | Makes verifiable |
|---|---|---|---|
| 1 | accounts + basic chat | storefront, cart, warehouses | identity, durable state, real-time |
| 2 | private rooms, membership | personalisation, catalogue queries | authorization / per-user derivation |
| 3 | reactions, polls, capacity | warehouse transfers | atomicity and isolation |
| 4 | scheduling, expiry | order lifecycle, expiry | durability of deferred work |
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
| `bench.mjs` | runs everything for one backend, unattended |
| `agent.mjs` | drives one headless coding session (build, upgrade, fix) |
| `run-suite.mjs` | grades one app: reset, lint, then each suite |
| `report-bugs.mjs` | turns findings into a behavioural bug report for the agent |
| `grader/grade.mjs` | executes scenarios against real clients |
| `grader/mutation-test.mjs` | validates the grader by injecting known defects |
| `linter/lint.mjs` | checks the app exposes the contract's test ids |
| `docker-compose.yaml` | the Postgres and MongoDB services |
| `reset-db.sh`, `restart-backend.sh` | environment control used by the suites |
| `tracks.mjs` | resolves a track: its paths, suites, ports and names |
| `tracks/<name>/` | one application: prompts, contracts, scenarios, lint walk |
| `backends/` | per-backend setup and deploy instructions given to the agent |
| `FINDINGS.md` | product issues the benchmark has surfaced |

## Keeping the harness honest

A benchmark that only produces flattering numbers is worthless. Practices that
earned their place, mostly by catching this harness being wrong:

- **Identical failures across backends mean the grader is wrong.** Architectures
  this different do not break identically. This tell has caught five false
  positives; none were real defects.
- **Mutation testing.** `grader/mutation-test.mjs` injects known defects into a
  working app and requires the grader to catch each one, in the right feature.
  A defect it misses is a hole in the oracle.
- **Reset before every suite.** Dirty state lowers scores silently.
- **Verify the app is using the benchmark's own database.** A generated app once
  connected to an unrelated instance and graded normally.
- **Never patch prompts, guidance or tests to make a failing check pass.** Fix
  the product, or report the failure.
- **Negative results are results.** Several predicted advantages did not survive
  contact with a stronger model. Those are recorded, not buried.

## What a run records

Everything needed to audit a verdict afterwards, under `<app>/stack-bench/`:

| Artifact | What it is |
|---|---|
| `bundle.json` | scores per suite, code metrics, environment checks |
| `grading-<suite>.json` | every criterion, pass or fail, with the observed detail |
| `contract-lint.json` | which test ids resolved |
| `media/*.webm` | one video per actor per feature — what each user saw |
| `media/*.png` | full-page screenshot at the exact moment an assertion failed |
| `media/*.trace.zip` | Playwright trace: steppable, with DOM snapshots and network |
| `records/bug-report-l<N>-round<M>.md` | what the agent was told each fix round |
| `.session-<mode>-l<N>.json` | the agent session: cost, tokens, duration, final message |
| `.prompt-<mode>-l<N>.md` | the exact prompt the agent received |

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
