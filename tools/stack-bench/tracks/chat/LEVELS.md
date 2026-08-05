# Stack Bench level sequence

Levels are ordered by **the property each one makes verifiable**, not by feature novelty.
Every level adds features, but its reason for existing is the class of machine checks it
unlocks, and each depends on the primitives the level below establishes.

This replaces the inherited 19-level chat ladder, which was built for a cost-to-build
benchmark and ordered by feature novelty. That ordering put authentication at no level at
all and contended state at level 19, so the first two levels could not discriminate between
backends — all three scored identically because nothing they exercised could fail.

The domain stays a chat/collaboration app. Rooms and messages are a good shared resource.
Only the progression changed.

---

## L1 — Basic chat, with accounts

**Unlocks:** identity, durable shared state, ephemeral state, per-user state.

Sequential's basic chat, with accounts added in place of its display-name stub.
Rooms, messages, presence, typing indicators, read receipts and unread counts.

Each feature contributes a different class of state for the harness to verify.
Accounts give stable identity, without which ownership and attribution mean
nothing. Rooms and messages give durable shared state. Typing indicators give
ephemeral, room-scoped state that has to expire on its own. Read receipts give
per-user-per-message state driven by other people's actions. Unread counts give
per-user derived counters, which is where the historical grading data shows the
most bugs.

The chat app is sequential's, unchanged: it is the app the team has history and
human grades for, and its feature set was well chosen. Accounts are the only
addition — "set a display name" is not authentication, and standing it in for one
made identity untestable.


## L2 — Authorization

**Unlocks:** access control, revocation, tenant isolation.

Resource ownership, private rooms, membership, invitations, kick/ban. Only members read a
private room. Revocation takes effect immediately, without a reload.

This is where hand-rolled backends genuinely struggle: every check is code someone has to
remember to write, on every path.

## L3 — Contended state

**Unlocks:** atomicity and isolation — the ACID properties nothing else here reaches.

Reaction and vote counts, a limited-capacity resource (seats, inventory, claims), a
per-user balance that transfers. All mutated by many clients at once.

Verified arithmetically: K clients × M operations must leave exactly K×M; a capacity of N
must never be exceeded; no balance may be double-spent; two clients claiming the same unique
thing must produce exactly one winner. These are unarguable checks with no judgment in them,
and they are impossible to perform by hand.

## L4 — Deferred and expiring work

**Unlocks:** durability of background work.

Scheduled sends, expiring/ephemeral content, reminders. Verified across a backend restart:
pending work survives and still fires, and expired content is actually deleted rather than
hidden.

## L5 — Volume

**Unlocks:** throughput, latency, and efficiency under load.

The app must remain correct and responsive with many rooms, a large message history, and
many concurrent clients. Sustained writes per second, propagation latency percentiles, and
whether query cost grows with data size.

---

## Compliance with the benchmark brief

| Requirement | Where it is satisfied | Status |
|---|---|---|
| Machine verifiable; deterministic inputs and expected end state | Scenario specs executed by `grader/grade.mjs`; no human judgment in scoring | done |
| Restart the application, verify durability | `restartBackend` step + `restart-backend.sh`; L1 session durability, L4 deferred work | done |
| Real-time properties with multiple clients, non-trivial effects | One isolated browser context per actor; delivery integrity, ordering, exactly-once, convergence | done |
| ACID properties | Durability at L1/L4; **atomicity and isolation at L3** | L3 outstanding |
| Scaling: throughput and latency | **L5**; `perf-benchmark/` exists but is not yet wired in | outstanding |
| Backend hint; fix the model, vary the backend | `--backend` selects the guidance pack; spacetime, postgres, mongodb implemented | done (Convex absent) |
| Fixed prompts, easy to run | Level prompts in `spec/`; `run-suite.mjs` grades an app in one command | partial — build and grade are still two commands |
| Agent hears verifier findings and gets to fix them | `run.sh --fix` consumes a bug report; the grader does not yet emit one automatically | partial — loop not closed |
| Track time, errors, tokens and cost | OpenTelemetry cost capture, per-run timing, structured findings in every bundle | done |

## Notes on running this sequence

Levels are cumulative: an app at L3 still has to pass L1 and L2 checks. Grading a level runs
that level's scenarios plus every earlier level's, so a regression introduced at L4 is
caught rather than scored around.

Each level keeps its own contract (`contracts/`), feature scenarios, and invariant scenarios.
Invariants are scored on a separate axis from features, because a feature-only score cannot
see cross-cutting properties — an app can implement every feature and still let one user
take over another's account.
