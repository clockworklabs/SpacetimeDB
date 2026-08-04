# stack-bench

A cross-backend, **agentic** benchmark: rank backends (SpacetimeDB vs Convex vs
Supabase vs …) by how successfully and efficiently an AI coding agent builds
real-time apps on each. Built in [Harbor](https://www.harborframework.com/)
format (the harness behind Terminal-Bench 2.0), so every task is a standard,
publishable Harbor task with fixed prompts and machine verification.

This is **separate from** the one-shot leaderboard in `tools/xtask-llm-benchmark/`
(which powers spacetimedb.com/llms-benchmark and ranks *models* on SpacetimeDB
only). stack-bench fixes the agent+model and varies the **backend**.

## Tasks

| Task | What it tests | Status |
|---|---|---|
| `team-chat` | **The main benchmark.** Full team-chat backend: rooms/owners/membership, presence, server-maintained unread counters, message edit + tombstone delete, idempotent sends, per-room **gapless seq under concurrency**, atomic credit tips, **restart durability**, real-time push (fan-out, late-joiner, edits/deletes/presence/membership/balances), latency + throughput. 46 checks, 4 weighted metric groups. | **oracle 1.0 on spacetimedb + convex, through Harbor** |
| `realtime-chat` | The original spike: minimal single-room chat, 4 checks. Kept as a smoke test. | oracle 1.0 on both, via Harbor |

Each `tasks/<task>/<backend>/` is a complete Harbor task (instruction.md +
task.toml + environment/ + solution/ + tests/).

## Design (what makes team-chat hard to saturate)

The verifier (`_shared/harness/`) is one backend-neutral TypeScript scenario
written against an `AppClient` contract (`src/appClient.ts`); each backend ships
a thin adapter implementing that contract with its **real client SDK**. Grading
is purely behavioral — multiple concurrent SDK clients drive the deployed app.

Four weighted metric groups (also emitted individually in `reward.json`):

- **correctness 0.40** — functional rules + transactional behavior under
  concurrency: gapless per-room seq with 3 concurrent writers, exact unread
  counters (multi-row atomicity with the message insert), tip conservation
  under concurrent transfers, idempotent resends, monotone mark-read,
  tombstone privacy, permission edges (kick/leave/edit/delete), and a final
  τ-bench-style **full goal-state comparison** against the harness's model.
- **realtime 0.30** — push, not polling: 3-subscriber fan-out exactly-once in
  seq order, cross-room isolation (server-side subscription filtering),
  late-joiner history-then-live with no dupes/gaps at the boundary, live
  propagation of edits, deletes, presence, membership, and balances.
- **durability 0.20** — Jepsen-style process kill + restart mid-scenario via
  each environment's `/opt/stack-bench/backendctl restart`; verifies every
  piece of state survives (messages incl. edits/tombstones, memberships,
  read state, balances), that the per-room **seq counter continues gapless**
  (durable counter — in-memory counters that reset fail), that the
  `client_msg_id` dedupe record survives, and that real-time works again.
- **perf 0.10** — delivery latency p95 under a generous threshold (1.5s) and a
  120-message concurrent burst delivered within budget; raw p50/p95 and
  throughput are reported as metrics either way.

Why an agent can't trivially score 1.0: the concurrency checks require real
transactional design (read-increment-write counters, multi-row atomic
updates), the durability checks kill lazy in-memory state, the late-joiner
boundary and exactly-once fan-out catch sloppy subscription logic, and the
scenario's ~46 checks are graded independently — partial credit makes the
leaderboard discriminating rather than binary.

The harness writes (Harbor contract, all under `/logs/verifier/`):

- `reward.txt` — the scalar weighted reward
- `reward.json` — named metrics: group subscores + latency/throughput numbers
- `result.json` — every check with pass/fail + failure detail: the
  machine-readable findings payload for a multi-step agent feedback loop

## Layout

```
stack-bench/
├── _shared/
│   ├── team-chat.base.md        backend-agnostic team-chat spec (source of truth)
│   ├── instruction.base.md      realtime-chat spec (spike)
│   └── harness/                 the shared grader (TS via tsx; injected into tests/)
│       └── src/
│           ├── appClient.ts     team-chat cross-backend contract
│           ├── teamChat/        model.ts (goal state) + scenario.ts (~46 checks)
│           ├── runTeamChat.ts   entry point: reward.txt/reward.json/result.json
│           └── …                chatClient.ts/scenario.ts/runScenario.ts (spike)
├── tasks/
│   ├── team-chat/
│   │   ├── spacetimedb/         env: rust+node+spacetime, backendctl; oracle Rust module
│   │   └── convex/              env: convex-backend image + node, backendctl; oracle app
│   └── realtime-chat/           the original spike tasks
├── xtask/                       cargo runner — `cargo stack-bench …`
└── README.md
```

Instruction files are assembled as: shared base spec + backend-specific
**Contract** section pinning the exact identifiers the grader connects to
(SpacetimeDB: table schemas + reducer signatures; Convex: function names +
arg/return shapes). Fixed prompts, machine-checkable surface.

## Environments & the restart hook

Every backend runs **in the agent's container** (single-container model):

- `spacetimedb`: `spacetime start` backgrounded at boot.
- `convex`: the official `ghcr.io/get-convex/convex-backend` image as the base
  (Ubuntu 24.04 + its `run_backend.sh`), with Node 22 installed on top;
  deterministic admin key from a fixed instance name/secret.

Both ship `/opt/stack-bench/backendctl` (`start|stop|restart|wait-ready`).
Boot and the verifier's durability restart use the same script, so restart
behaves exactly like a fresh boot against the same data dir. Single-container
also means the backend shares the agent's cpu/memory budget (`task.toml
[environment]`), keeping the perf metrics a fair cross-backend comparison.

## How to run

```bash
# Inject the shared grader into each task's tests/ (auto-run by the commands below).
cargo stack-bench build

# List task variants.
cargo stack-bench list

# Oracle sanity check (expect reward 1.0). --task defaults to team-chat.
cargo stack-bench oracle spacetimedb
cargo stack-bench oracle convex
cargo stack-bench oracle spacetimedb --task realtime-chat

# Run a real agent (needs the provider API key for the agent).
cargo stack-bench agent spacetimedb --model anthropic/claude-opus-4-6
cargo stack-bench agent convex      --model anthropic/claude-opus-4-6

# Run EVERY task/backend with the same agent+model and print a comparison table.
cargo stack-bench all --agent claude-code --model anthropic/claude-opus-4-6

# Anything after `--` is forwarded verbatim to `harbor run` (e.g. -k 5 for pass^k trials):
cargo stack-bench agent spacetimedb --model … -- -k 5
```

The tasks are plain Harbor tasks, so they also run without cargo:
`harbor run -p tasks/team-chat/spacetimedb -a oracle -y`.

Local tooling used to validate: `harbor 0.7.1`, `spacetime 2.5.0`, `node v22`,
`docker`, `cargo`.

## Agents & models

Harbor's `-a` flag picks who attempts the task: `oracle` runs the committed
reference solution (harness self-test, no API key), `nop` is the empty
baseline, and real agents (`claude-code`, `codex`, `terminus`, …) attempt the
task from `instruction.md` with `-m provider/model`. See Harbor's docs for
keys; OpenRouter works for the model-routed agents.

## Multi-step feedback loop

`result.json` lists every failed check with a concrete detail string
(machine-readable findings). A driver can hand these back to the agent and
re-run the verifier for a fix-it loop; per-run effort (tokens, cost,
wall-clock) comes from Harbor's job output. Wiring a standard loop driver is
future work (Harbor `[[steps]]` semantics are still being validated).

## Validation status

- **team-chat via Harbor** (Dockerized env + SHARED verifier + backendctl
  restart): oracle reward **1.0000 on both backends** — 46/46 checks, 0
  exceptions. `harbor run -p tasks/team-chat/{spacetimedb,convex} -a oracle`.
- Observed oracle metrics (same host, same container limits):
  SpacetimeDB p50 4ms / p95 7ms / 952 msg/s burst;
  Convex p50 18ms / p95 27ms / ~90 msg/s burst.
- **realtime-chat via Harbor**: oracle 1.0 on both (spike result, June 2026;
  pinned to the older CLI/SDK it was built against).

## Anti-gaming notes (from the design research)

- Grading is behavioral through real SDKs against the running app; the agent
  never sees `tests/` (Harbor copies it in at verification time).
- Durability checks make hardcoded/stubbed backends fail: state must survive
  a process kill, and counters must continue exactly.
- For real leaderboard runs, pre-bake dependencies and cut agent internet
  access after setup (SWE-bench-style retrieval gaming), and pin image
  digests + tool versions for reproducibility.
- Publish tasks, graders, and oracle solutions; report results honestly
  including where SpacetimeDB loses.

## Remaining work

- Harbor-mode validation of team-chat (in progress) and real-agent runs.
- Effort metrics (tokens/cost/turns) surfaced in the comparison table
  (`in_tok`/`out_tok` come from Harbor job output today).
- Feedback-loop driver (Harbor `[[steps]]` or a thin outer loop).
- More backends (Postgres, Mongo, Supabase, Firebase) — each needs an
  environment + backendctl, an adapter (~200 lines), an instruction Contract
  section, and an oracle solution.
- Pre-baked deps in the environment images for speed + hermeticity.
