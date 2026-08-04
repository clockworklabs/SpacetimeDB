# Stack Bench — Plan

An automatically graded, reproducible, blackbox benchmark that measures how well LLMs build
realtime full-stack apps on SpacetimeDB vs Postgres vs MongoDB — correctness, realtime,
durability under fault injection, performance, and cost-to-build — with zero human grading.

Successor to `tools/llm-sequential-upgrade` (which keeps manual grading as the depth/calibration
tool). Target consumer: partners like Emergent who want to point a model at the harness and get
a signed, reproducible scorecard.

---

## 1. What we're actually claiming, and how the benchmark proves it

The claim is not "SpacetimeDB the database beats Postgres the database." The claim is:
**an LLM building on SpacetimeDB ships a correct, realtime, durable app cheaper and faster than
the same LLM building the same app on Express+Postgres or Express+Mongo.** The benchmark
measures the app the LLM built, blackbox, from outside its sandbox.

The reason this claim is winnable is structural, and each structural advantage maps to an
externally observable test:

| STDB property | What LLM-built alternatives get wrong | Blackbox test |
|---|---|---|
| One reducer = one serializable transaction (`IsolationLevel::Serializable` is the only level) | `findOne → mutate → save` lost updates; missing `FOR UPDATE`; race on unique inserts | Concurrency storm: K clients × M increments → final total must equal K×M exactly |
| Subscription updates are atomic, exactly-once per tx, in commit order (subscription eval holds the tx lock) | `io.emit()` after `await pool.query()` — broadcast and commit aren't atomic; interleaving under load | Broadcast drift: after a write storm, every subscriber's reconstructed state must equal the DB's actual state |
| Confirmed reads: acks held until the commitlog offset is fsynced (`?confirmed=true`, default on ws v2/v3 and HTTP) | Mongo `w:1, j:false` loses acked writes on crash; Express apps ack before commit | `docker kill -9` mid-workload, restart, assert every acked write survives |
| Push-based subscriptions in the protocol itself | Realtime features that only work after refresh — the single most common bug class in graded runs (stale closure breaks every realtime feature at once) | Two live clients; assert propagation to the peer within SLO, with reconnects forbidden |
| Self-reporting server: `/v1/metrics` Prometheus (txn counts, rows scanned, index seeks), `total_host_execution_duration` on every ws message, `spacetime-energy-used` header | No per-transaction accounting exists at all in a hand-rolled Express stack | Cost-to-serve: rows_scanned stays flat as data grows (indexes actually used); host-time vs wall-time separation for free |
| Commitlog is self-contained and replayable (module blob in system tables) | n/a | Delete snapshots, restart, assert identical query results |

The historical grading data confirms the thesis empirically: the rubric is ~40% propagation-
latency criteria in disguise, and "realtime-only-after-refresh" is the highest-frequency failure
in Postgres/Mongo runs while being rare on STDB.

---

## 2. Why grading can be automated now (what the research found)

Analysis of 13 historical `GRADING_RESULTS.md` files across models and backends:

- **~60–65% of all lost points are protocol-level** — wrong data scoping, cross-user sync
  failures, permission bypasses (kicked user still receiving messages), timing features,
  non-public tables — all assertable by headless multi-client probes.
- **~35% need a DOM** — missing UI affordances, hover-only controls, silent failures, styling.
  But: the composed prompts **already contain `**UI contract:**` blocks** (exact selectors,
  button text, `title` attributes, status-dot colors) authored precisely for "deterministic
  automated UI assertions" — and `run.sh:288–294` currently *strips them out* because grading
  is manual. Un-stripping them converts most of this bucket into deterministic Playwright asserts.
- The one thing neither layer catches — "plausible-looking UI wired to nothing" — is covered by
  requiring every contract element to produce an observable state change, plus a thin
  screenshot-judge pass for polish only.
- We have a **calibration set for free** — but only for the functional grader, and only by
  replay: re-deploy the historical app snapshots that humans already graded and run the
  auto-grader on the *same apps, same features, same rubric*. The functional grader ships when
  its scores for those apps agree with the human's within tolerance (target: ±2 points per
  level, and never scoring 3 on a feature the human observed broken). This is a regression test
  for the grader against the only ground-truth labels we own — not a comparison across
  different tests. The new scenario families (durability, nemesis, protocol invariants) have no
  human baseline and need none: they are binary invariants correct by construction, validated
  instead by confirming they fire on historical apps known to contain the defect
  (oracle-strength validation, §3.5).

Prior art this design borrows deliberately: WebGen-Bench (browser-agent executes test cases
against the running app), τ-bench (grade by final DB state, judge-free), Jepsen (recorded
operation histories + invariant checkers + scripted nemesis), Convex Fullstack-Bench (the direct
vendor-comparison precedent — our differentiators are full automation and fault-injection
grading, which nobody has), and the SWE-bench weak-oracle lesson (~31% false passes → we
adversarially validate oracles against known-broken historical apps).

---

## 3. Architecture

```
┌────────────────────────── Stack Bench Runner (one container/CLI) ─────────────────────────┐
│                                                                                           │
│  Build phase (per backend × model × trial)                                                │
│  ├─ agent sandbox (container): Claude Code / other-vendor session builds+deploys the app  │
│  ├─ OTel cost capture (existing pipeline, unchanged)                                      │
│  └─ output: app compose stack + bench.config.json manifest                                │
│                                                                                           │
│  Grade phase (runs OUTSIDE the agent sandbox, fresh state per trial)                      │
│  ├─ L1: Functional grader  — Playwright, N browser contexts, UI-contract assertions       │
│  ├─ L2: Protocol probes    — headless clients via manifest; history recorder              │
│  ├─ L3: Nemesis            — scripted faults (kill -9, toxiproxy, disk) + invariants      │
│  ├─ L4: Perf               — existing perf-benchmark clients, driven via manifest         │
│  └─ L5: UX judge           — screenshot LLM-judge, binary rubric, ensemble (small weight) │
│                                                                                           │
│  Fix loop: structured failure report → fix agent → re-grade (capped; replaces reprompts)  │
│  Output: signed results bundle (scores, histories, screenshots, videos, telemetry, seeds) │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 The interface problem, solved two ways

Every generated app has a different API shape — the perf-benchmark clients already duck-type
endpoint names per run. Two complementary fixes:

1. **The browser is the universal interface.** The UI contracts in the spec make the DOM
   machine-checkable without constraining architecture. Playwright with N isolated contexts
   replaces the human's two Chrome profiles. This is the ground truth grader — fully blackbox,
   identical across backends.
2. **`bench.config.json` manifest.** The spec requires the generated app to emit a small
   machine-readable self-description: how to programmatically authenticate a user, send a
   message, open a live subscription/stream (transport + endpoint/reducer names). The harness
   **verifies manifest honesty** by cross-checking: an action performed via the manifest must
   produce the corresponding UI-contract state change in a browser context. A dishonest or
   broken manifest = scored failure. This kills duck-typing for protocol probes and load
   generation without dictating the stack.

### 3.1b Why this isn't the old Playwright problem

Two failure modes killed browser automation here before: LLM-agent-driven browsing (a model
round-trip per click — the current Chrome-MCP grading stack) is slow, and adaptive element
discovery against arbitrary generated HTML is incomplete. Stack Bench does neither:

- **Tests are static, human-authored, written once against the frozen UI contract.** No test is
  ever generated per app. The app must conform to the contract hooks (`data-testid` / exact
  text anchors); a missing hook is a scored failure with a mechanical error message the fix
  loop resolves cheaply — and "follows an interface spec" becomes benchmark signal in itself.
- **No model in the grade loop.** Deterministic Playwright, fixed selectors, parallel isolated
  contexts; a full L12 pass is minutes. The LLM-judge exists only in the bounded UX-polish pass.
- **Time-based features are made test-friendly at the spec level** (Phase 0): ephemeral expiry
  and scheduling durations must be parameterizable down to seconds, because clock-faking can't
  be done uniformly across three backends and long waits are otherwise unavoidable.
- **Flake control:** realtime assertions are SLO-bounded waits ("peer observes within 2s"), not
  instantaneous checks, and every cell reports N-trial pass rates.
- **Conformance guardrail:** contracts stay minimal (one hook per assertable element), and the
  build agent gets a public contract-lint tool to self-check before declaring `DEPLOY_COMPLETE`
  — so scores measure functionality, not selector nitpicks.

History check — statically-authored tests failed here before because contracts were *passive*:
prose-level hints ("button text 'Kick'") buried in the spec, never verified during the build, so
generated apps drifted and the tests shattered. Three changes address that failure mode head-on:

1. **Conformance is a build gate.** `DEPLOY_COMPLETE` requires the contract linter (loads the
   app, checks every required `data-testid`) to pass. That moves hook conformance into the
   "code compiles" category — mechanical error → model fixes it in the same session. The old
   setup gave the model no feedback loop at all.
2. **Contract format is an enumerated `data-testid` table**, not incidental English text.
   "Add exactly this attribute to this element" is an instruction-following task models do
   reliably; matching paraphrasable prose is not.
3. **One-shot binding resolver as fallback.** If hooks are still missing after the fix cap, an
   LLM/accessibility-tree pass inspects *this app's* DOM **once** and emits a selector map
   (abstract contract element → concrete selector). Each binding is verified deterministically
   (the resolved element must produce the contracted state change when probed); the full static
   suite then runs against the map with no model in the loop. AI adapts to variable output
   exactly once per app, and its output is checkable data — unverifiable bindings are the
   scored failure.

Go/no-go: Phase 0 measures post-lint conformance rate per hook on a handful of real runs before
the full suite is built. Below ~95%, the binding resolver is promoted to primary and the testid
mandate demoted to a hint — a two-week experiment either way, not a project-level bet.

### 3.2 Scenario specs (the missing `test-plans/`, done right)

Each feature gets a versioned YAML scenario: setup (users, rooms), steps (actor, action via
UI-contract selector or manifest, expected observation, SLO), and invariants. Scenarios are the
single source for the Playwright grader, the protocol probes, and the report. This is the
`test-plans/` directory that `GRADING.md` and `run.sh` reference but which never existed —
authored once, and validated against historical known-broken apps (the oracle-strength check).

### 3.3 Nemesis catalog (durability / "fail a disk")

Faults are versioned scripts (fault type + workload offset + seed), not random chaos —
reproducibility comes from fixed schedules and N-trial pass rates, not Antithesis-grade
determinism (evaluated; overkill and commercial).

| Scenario | Mechanism | Invariant |
|---|---|---|
| Process crash | `docker kill -9` DB/host mid-workload, restart | Every write acked to a client before the kill is visible after restart (STDB: assert via `--confirmed true`; run both confirmed modes to chart the knob) |
| Disk failure | `dm-flakey` under the DB volume (Linux runners); ENOSPC via volume quota | No acked-write loss; clean error surfacing to clients, no silent corruption |
| Network partition app↔DB | Toxiproxy | During: writes fail visibly or queue coherently. After heal: all clients converge to identical state |
| Latency injection | Toxiproxy latency/jitter | Realtime SLO degradation is graceful; no reorder/duplication visible to subscribers |
| Client reconnect storm | Drop all client sockets | Clean resubscribe; no missed or duplicated updates; kicked users stay kicked |
| Concurrency storm | K concurrent clients, seeded workload | Counter total exact; unique-constraint races rejected; broadcast drift = 0 |

All clients record invoke/ack/observe histories (Jepsen-style, app-level); checkers run over
histories post-hoc. We check app-level invariants, not general linearizability — no Elle needed.

### 3.4 Scoring

Five category scores, reported separately (composite only for headlines):
**Correctness** (scenario pass rate) · **Realtime** (propagation SLOs, no-refresh rule enforced
mechanically) · **Resilience** (nemesis invariants) · **Performance/cost-to-serve** (existing
perf-benchmark + metrics oracles) · **Build cost** (exact $ via existing OTel pipeline, fix
iterations, wall time). Every criterion is binary; console errors and refresh-dependence keep
their current capping rules, now enforced by code. LLM-judge contributes only a small bounded
UX-polish component (rubric-anchored binary criteria, 2–3 judge ensemble, both pairwise
orderings, generating-model family excluded from judging itself, calibrated on ~50 human labels).

### 3.4a Iteration budget (how far the loop goes before "meets spec or not")

Two layers, both fair by construction:

- **In-session:** the build agent iterates freely against the contract linter before
  `DEPLOY_COMPLETE`; every token is billed to build cost. No separate cap.
- **Cross-session fix rounds:** capped at a fixed, published K per level (start: K=3, matching
  manual-workflow tolerance), identical across backends. Contract nonconformance consumes fix
  rounds exactly like functional bugs. At budget exhaustion the app is graded as-is: the
  binding resolver salvages reachable features (costing the agent nothing — grading-side
  completeness, not another fix chance), the rest score 0 as untestable (the existing manual
  rule).

K is a truncation point, not a tuned threshold: the reported output is the trajectory — score
at first deploy, score and cumulative cost after each round — yielding both headline framings
("score at fixed budget", "cost to reach target score") wherever K sits. Fix rounds consumed
replaces the manual "reprompt efficiency" axis, now measured automatically. Fairness rule: the
fix agent receives only the structured failure report (behavioral, `BUG_REPORT.md`-equivalent),
never scenario definitions or oracles — the loop cannot overfit the harness.

### 3.4b The jank problem (subtle weirdness humans catch)

A deterministic floor risks the wrong compression: a janky-but-technically-passing competitor
app scores level with a polished one — and subtle jank is disproportionately where competitors
lose points under human grading. Three mechanisms preserve that signal:

1. **Jank oracles.** Most historical "feels janky" issues have mechanical signatures, captured
   during every scenario run: web vitals (CLS, INP, long tasks — layout jump and input
   sluggishness are literally these metrics); a MutationObserver trace for flicker
   (remove+re-add within a window), transient duplicates, full-list re-renders per update,
   scroll jumps, and focus theft while typing; console *warnings* (React key warnings track the
   duplicate/flicker class); and propagation latency reported as p50/p95 axes, never collapsed
   to pass/fail — 1.9s-but-under-SLO must score visibly worse than 80ms.
2. **Pairwise video judging for the residue.** Playwright records every scenario; judges see
   the same scenario side-by-side across backends and answer "which completed the task more
   convincingly," both orderings, ensembled — far more reliable than absolute scores, it mirrors
   the human's actual "this feels off" judgment, and the videos double as public evidence
   artifacts. Hollow-UI ("didn't actually do the task") is separately covered by the contract
   rule that every element must produce an observable state change.
3. **Human audit as scenario factory.** Periodic human passes over a sample of runs remain —
   not as a gate, but to convert each newly found subtle issue into a mechanical oracle (most
   of them, empirically) or a judge rubric item. The grey area shrinks monotonically instead of
   being accepted as a constant.

Reporting rule: the deterministic floor and the judged polish score are published as separate
axes, never blended. A skeptic can discard the judged axis and the durability/correctness/
realtime claims still stand alone.

### 3.5 Anti-gaming / validity rules

- Grader runs outside the agent's environment; the agent never sees scenario YAMLs or fault
  scripts (it sees the feature spec + UI contracts only, same as today).
- Fresh containers and volumes per trial (the WebArena lesson); no state leakage.
- Never execute agent-authored strings in the grader; manifest is data, verified by observation.
- N ≥ 3 trials per cell; report pass-rate and variance, not single runs.
- Oracles adversarially validated against historical known-broken apps before any published run.
- Pinned model, pinned prompt pack, pinned docker images, seeded workloads; run-id cache-busting
  as today. Everything needed to re-run a result ships in the bundle.
- Environment friction controlled: equivalent docs/guideline packs per backend (the Convex-evals
  finding — docs quality swings success ~20%, so it must be a controlled variable).

---

## 4. What we reuse (this is not a from-scratch build)

| Existing asset | Role in Stack Bench |
|---|---|
| `llm-sequential-upgrade/run.sh` + OTel cost pipeline | Build phase, unchanged (stop stripping UI contracts; add manifest requirement) |
| 19-level prompt pack + UI-contract blocks | Feature specs and DOM assertions |
| 13 historical `GRADING_RESULTS.md` + broken app snapshots | Auto-grader calibration set + oracle-strength validation corpus |
| `xtask-llm-benchmark` scorer architecture + multi-vendor LLM clients | Scorer patterns; multi-model support beyond Claude |
| `perf-benchmark` per-backend clients | Load generation, now manifest-driven instead of duck-typed |
| `crates/smoketests` (`restart_server`, `sql_confirmed`, subscribe oracles) | The durability harness shape, already written |
| `docker-compose.otel.yaml` | Extended into the full per-backend app stacks |
| Results repo + public viewer | Publication target, extended with new score axes |

---

## 5. Phases

**Phase 0 — Foundations (1–2 wks).** Un-strip UI contracts from prompts; add
`bench.config.json` to the spec; containerize the STDB app path so all three backends run as
uniform compose stacks (STDB currently runs on the host); author scenario YAMLs for L1–L7 from
the existing rubric; stand up the scenario schema + runner skeleton.
*Exit: a scenario file executes against a hand-picked historical app and produces a score.*

**Phase 1 — Functional auto-grader (2–3 wks).** Playwright multi-context grader executing
scenarios L1–L12; console-error capture; refresh-detection (fail any feature whose assertion
only passes after `page.reload()`); manifest verification; structured failure reports feeding
the existing `--fix` loop. **Calibrate against historical manual grades until agreement.**
*Exit: full L1–L12 run on all three backends, zero human input, scores within tolerance of the
human-graded canon.*

**Phase 2 — Nemesis + invariants (2 wks).** History-recording clients; nemesis runner
(docker kill, Toxiproxy, dm-flakey — Linux runners required); the six scenario families in §3.3;
convergence and durability checkers.
*Exit: the durability headline chart — acked-write survival across STDB confirmed/unconfirmed,
Postgres, Mongo `w:1` — generated automatically.*

**Phase 3 — Perf + UX judge (1–2 wks).** Manifest-driven perf runs; metrics oracles
(rows-scanned-flat, txn accounting); screenshot judge with calibration; scoring engine emits the
composite bundle and updated public-viewer JSON.

**Phase 4 — Hosting + blackbox trigger (2 wks).** One-command runner
(`stack-bench run --backend X --model Y --trials 3`); CI on dedicated Linux runners (bare-metal
or fixed instance type for perf stability); a trigger API + results endpoint so a partner can
submit `{model, backend(s)}` and receive a signed results bundle URL; publish harness + fault
scripts + recordings publicly (the credibility move Convex made).

**Phase 5 — Hardening the claim (ongoing).** Second app archetype to counter single-app
overfitting; multi-vendor models via the xtask clients; periodic scheduled runs; contamination
hygiene (novel spec revisions).

### What gets built: the task catalog

**v1 is the existing chat app.** It is the realtime app par excellence (the rubric is ~40%
propagation-latency criteria — the terrain where the comparison is won), the entire prompt
pack / UI contracts / rubric / calibration corpus already exist for three backends, and its
later levels contain genuinely race-prone state for the concurrency storm (poll totals,
reaction counts, unread counters, slowmode, member counts — all bug classes human graders
actually caught). Durability reads clearly on it too: "kill the DB mid-conversation, restart,
every acked message survives."

**Archetype #2 (Phase 5): a live auction / marketplace.** Chat's correctness failures are
cosmetic; the second app makes them catastrophic — concurrent bids on one item (unique-winner
races), balance transfers (the K×M invariant), inventory decrement (oversell), live bid feeds
(realtime), auction close (scheduling). Every failure mode has a dollar sign attached, which is
how the comparison charts land with a partner audience. The `keynote-bench-harness` transfer
template already implements the core invariant workload. Deliberately deferred: v1's job is the
automated pipeline, and launching on chat is a quarter faster than launching on a new app.

Rough sizing: Phases 0–2 are the MVP a partner can run — one engineer ~6–8 weeks. Full system
inside a quarter. Highest-risk item is Phase 1 grader/human agreement; it's front-loaded and
has an objective acceptance test.

---

## 6. Decisions taken (flag if you disagree)

1. **Browser as ground truth, protocol probes as accelerant** — not a mandated wire API, which
   would erase STDB's protocol advantage and over-constrain the LLM.
2. **Scripted fault schedules + N trials over deterministic simulation** — Antithesis-grade
   determinism isn't worth the cost; reproducibility = pinned everything + pass rates.
3. **LLM-judge minimized** — deterministic asserts for correctness/realtime/durability;
   judge only for bounded UX polish. This is what makes results defensible to a skeptic.
4. **No HA/replication claims** — open-source standalone is single-replica by design
  (`num_replicas = 1`); the durability story is single-node confirmed reads, which is strong
  and honest. Cloud HA claims need a separate cloud-hosted bench.
5. **Manual grading is retained** as the calibration and regression instrument, not the
   product — it's how we keep catching the subtle issues and feed them back as new scenarios.
