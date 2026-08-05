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

## Levels

Ordered by the property each makes verifiable, not by feature novelty —
see `levels/LEVELS.md`.

| Level | Adds | Makes verifiable |
|---|---|---|
| 1 | accounts, rooms, messages | identity, durability, real-time |
| 2 | private rooms, membership | authorization, revocation, isolation |
| 3 | reactions, polls, capacity | atomicity and isolation under contention |
| 4 | scheduling, expiry | durability of deferred work |
| 5 | volume | throughput, latency, efficiency |

Levels are cumulative: an app at L3 is still checked against L1 and L2, so a
regression is caught rather than scored around.

## How verification works

Apps differ in structure, so the harness locates elements only through a
contract of `data-testid` attributes the prompt requires (`levels/contracts/`).
Scenarios (`levels/scenarios/`) then drive real browser clients — one isolated
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
| `levels/` | prompts, contracts and scenarios per level |
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
