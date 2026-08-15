# What this benchmark still has to prove, and in what order

The thesis under test: as a task approaches real production conditions,
SpacetimeDB pulls ahead structurally — the database does the hard parts, so
correctness arrives by construction and the model spends less getting there.
The harness's job is to make that measurable in a way a hostile reviewer
cannot dismantle. Fairness here means one thing: **every criterion is winnable
by an expert human on every stack, using that stack's own real tools.** The
benchmark's product is the bill.

The versioned-definition, action/adapter, campaign, and OCI-appliance work is
mapped separately in [EXTENSIBILITY-ROADMAP.md](EXTENSIBILITY-ROADMAP.md). Its
ticket-sized execution order is in
[IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md). Its
compatibility foundation is now implemented: all current track/scenario files
compile deterministically into definition schema v1, the 47-action legacy
language has an explicit schema-v1 plugin contract and startup-validated
registry with deadlines, capabilities, evidence, redaction, and renderer
metadata; malformed content fails before browser startup, and public result
artifacts use a strict schema-v2 identity envelope and parent-attempt chain.
Pre-v1 results are checksummed in an inert archive; active readers accept only
the current schema and never fabricate missing provenance.
The target authoring model treats L1/L2 as aliases to scored recipes composed
from reusable behavior packs and separately versioned fixtures; current
level/suite files remain compatibility inputs during that migration. Pack-owned
requirements and hook fragments now compose the exact builder task alongside
the checks, while check-only filters leave that task fixed. The first
composition source slice now validates eight ecommerce packs, two
fixture sets, L1/L2 parity recipes, a smaller smoke recipe, and promoted L1 and
L2 aliases. Framework-neutral L1 1.1 and L2 1.2 have completed repeated
reference, mutation, and null-control qualification on the exact appliance
engine and now own the promoted aliases; the earlier releases are retired.
runtime still executes the compatibility scenarios, but every grade and bundle
now binds the exact compiled recipe, pack, fixture, applicable calibration, and
selected stable-check scope. Pack/check selection drives that executor, and the
comparison tool refuses mismatched scope identities. All 47 actions now have
independent capability-scoped executors, and the compatibility action dispatcher
has been deleted.

## Production gate

No new comparative result is publishable until phases 0-2 are complete. Existing
L1-L2 runs remain useful diagnostic evidence, but the hardened harness must rerun
them before they become the baseline for L3-L5.

Hardening completed on 2026-08-10: destructive Spacetime resets no
longer read targets from generated config; unowned and non-loopback resets are
refused; stale phase outputs are removed before execution; fix-round comparison
rejects lost conclusive evidence; setup-level inconclusive points are removed
from the denominator; validated track levels are explicit; and the locked
Playwright install plus a dedicated loop fixture now pass end to end. Every run
now carries an authenticated backend lease with its destructive targets and
immutable process/container identities, plus atomic exclusion for its run slot
and dedicated listener. Coding sessions are container-only; the host execution
path and its duplicate lifecycle logic have been removed. A
model-free Docker smoke builds and publishes a real TypeScript module, verifies
it through SQL, proves its integrated log stream is authorized, and proves
cleanup. The clean-config identity handoff that previously broke log streaming
was fixed in the CLI on 2026-08-11.

### Phase 0 - stop unsafe or unauditable runs

- [x] Give every run an explicit backend lease: run id, server URI, database or
  module name, process/container id, data directory and ownership marker.
- [x] Reset, restart and teardown only through that lease for publishable runs. Generated application
  configuration must never choose a destructive target.
- [x] Run a dedicated SpacetimeDB host per run. A pre-existing host is never
  reused for a publishable run and is never killed.
- [x] Make the isolated build container mandatory. Model sessions have no host
  execution path.
- [x] Replace reusable result filenames with run-stamped artifacts written
  atomically and validated against the current run id.
- [x] Represent harness failure, app failure, inconclusive and ungraded as
  different structured outcomes. A harness failure never changes a score.

Exit criteria: malicious config cannot redirect a reset; a pre-existing listener
survives; stale reports cannot be consumed; an answer-key probe cannot reach the
harness; interrupted runs clean up only resources they created.

Acceptance status: **signed off 2026-08-10.** The automated fault test stops the
leased host through the real restart script, fails before the replacement host
starts, and then proves the owned host, build container, locks and private lease
are removed while a separately owned listener/container survive. It also proves
an unleased same-name container is refused rather than adopted or deleted.

### Phase 1 - make the harness reproducible

- [x] Add one Stack Bench package manifest and lockfile, pin Playwright, and make
  browser installation part of bootstrap.
- [x] Pin container images by digest and record Node, CLI, SDK, model, prompt,
  skill, rubric and scenario hashes in every run.
- [x] Put scoring, fix-round comparison and diagnostic sanitisation behind pure
  modules with unit tests.
- [ ] Run the offline loop, scenario validation, mutation suite and safety tests
  from a clean checkout in CI on Linux; add a Windows orchestration smoke test.

  Partial 2026-08-11: `.github/workflows/stack-bench.yml` now performs a locked
  install, scenario validation, focused tests, the real null-app grading gate
  and three offline loops on Linux, plus the focused tests and one orchestration
  loop on Windows. It also validates the checked-in canonical fixture registry,
  exact hashes, hygiene and candidate mutation anchors from a clean checkout;
  ignored historical origin archives are verified when present but are not a
  prerequisite for validating the canonical bytes. The null artifact is
  retained by CI. This item stays open: the workflow has not received its first hosted
  CI result. The mutation runner itself now fails closed on an imperfect
  baseline, setup/inconclusive/wrong-criterion/collateral failures, dead or
  ambiguous edits, and reset/redeploy/readiness faults; every manifest binds its
  backend, track, scenario and exact target criteria. That is prerequisite
  hardening, not a substitute for executing the mutants against reference apps.
  Historical chat manifests remain `legacy-unreproducible`; exact candidate
  manifests now exist for all three ecommerce L1 canonical fixtures, but their
  live mutation gates have not all passed.

  Partial 2026-08-11: a fail-closed reference registry now binds preserved and
  imported source hashes, evidence, fixture hygiene and manifest ownership.
  Sanitized ecommerce L1 candidates for all three backends pass a combined
  Docker compile gate in the pinned benchmark image. That gate uses locked
  installs, TypeScript checks, and—for SpacetimeDB—the repository-built module,
  standalone schema extractor, generated bindings and client build. The
  retained passing artifact is
  `archive/pre-v1/results/reference-builds/reference-build-20260811152751-1399692.json`.
  They deliberately remain `candidate`: live baseline repetition and regenerated
  mutation kills are the next gate, so the CI/mutation checkbox stays open.

  Partial 2026-08-11: all three ecommerce L1 candidates have now cleared the
  repeated live gate. For each backend, two fresh Docker runs scored 51/51 and passed all 48
  criteria, including nine zero-point controls, with identical evidence
  fingerprints and verified owned-host/container/lock teardown. Every candidate
  still needs a regenerated manifest and clean exact mutation kills; therefore
  no fixture and not this phase is promoted yet.

  Partial 2026-08-11: MongoDB has also cleared the repeated live mutation gate.
  Two fresh Docker runs each scored 51/51 and cleanly caught both exact mutants
  (oversell at 201a/201b and duplicate checkout at 203b), with stable
  fingerprints/image and complete teardown. PostgreSQL and SpacetimeDB mutation
  gates plus a tracked clean-checkout promotion attestation remain before any
  fixture is marked `active`.

  Partial 2026-08-11: PostgreSQL's corrected deterministic manifest caught both
  exact mutants in both repetitions, but the fixture has **not** cleared the
  combined gate. Its second pristine run lost one actor's catalogue and Orders
  controls during contention and finished 50/51 (201a failed, 201b
  inconclusive). The mutation baseline immediately afterward passed the same
  feature, so the failure is transient but not assignable from the retained
  evidence. Media-free qualification now captures failure-only actor screenshots
  and the exact leased container's initial server/client logs; PostgreSQL must
  repeat the entire two-run gate under that hardened evidence path.

  Partial 2026-08-11: PostgreSQL has now passed that repeat on the hardened
  harness. Both fresh Docker runs scored 51/51, caught both exact mutants 2/2,
  used the same immutable image/criterion/harness fingerprints, and released
  their exact leases and containers. Live checks also proved restart no longer
  accumulates `tsx watch` process groups. MongoDB's earlier pass predates the
  final lifecycle/readiness harness hash and must be repeated before the three
  fixtures can be promoted together.

  Partial 2026-08-11: SpacetimeDB's first mutation gate rejected an ineffective
  stock mutant. Removing the outer availability check left an independent
  `decrementStock` guard, so observable behavior did not change. The corrected
  candidate omits inventory reservation and must make both the stock and
  successful-order-count assertions fail. Its first retry is excluded: Docker
  pressure made the pristine systems suite inconclusive, and the operator then
  interrupted still-active mutation work. Exact container, listener and locks
  were verified released. Qualification repetitions now have a bounded outer
  supervisor with a private lease handoff and exact lease-aware rescue cleanup;
  teardown also refuses to equate a failed port inspection with an empty port.
  Every subprocess on the L1 qualification path now has a layer-owned deadline
  as well: Docker control operations, suite/mutation graders, general bench
  helpers, and the coding session itself can no longer wait indefinitely.
  SpacetimeDB has now passed the clean two-run gate on the final hardened
  runtime: both runs scored 51/51 and caught both declared mutants with stable
  image/harness evidence and complete lease teardown. MongoDB and PostgreSQL
  still need repeats under this same frozen runtime hash before the three
  references can be promoted together.

  MongoDB's first final-hash repeat is excluded: both pristine runs were 51/51
  and the first repetition caught both mutants, but repetition 2 hit a bounded
  Docker `ETIMEDOUT` after catching the first mutant. It was correctly recorded
  as a mutation-control harness failure and released its exact container/lease;
  MongoDB still needs a fresh two-run gate on a healthy runner.
- [x] Store each fix session separately and aggregate its tokens, cost, turns and
  duration into the level and run totals.
- [x] Grade a null build — an empty app directory, nothing implemented — and
  require every scored criterion to FAIL. Anything that passes is asserting
  nothing and must be fixed or unscored before it counts again. Cheap: no model
  spend, one grading pass per track. Terminal-Bench's harness ships this as a
  `NopAgent` for the same reason.

  Completed 2026-08-11 against the real L1-L2 grader for both validated tracks.
  A reachable blank HTTP app scored zero: all 129 point-bearing criteria / 179
  points failed conclusively, with zero passes and zero oracle gaps. The
  criterion-level evidence is stored in
  `archive/pre-v1/results/null-control-20260811.json`; the same hard gate now runs in Linux CI.

  Important limit: all 129 failures occurred during feature setup, before their
  individual assertions ran. This proves an empty app cannot collect points; it
  does **not** prove that each assertion detects its target defect once setup is
  healthy. Null artifacts therefore split setup-stage from assertion-stage
  failures. Phase 2's passing reference apps and caught mutations remain the
  non-vacuity proof and are not replaced by this checkbox.

  The pre-control evidence audit on 2026-08-10 found 59 scored criteria / 113
  points in the ecommerce grading bundles then on disk:

  | | criteria | points | share |
  |---|---|---|---|
  | demonstrated able to fail (mutant kills it, or observed both failing and passing) | 13 | 27 | 24% |
  | always passed, never once observed failing | 36 | 60 | **53%** |
  | never executed at all (all of L3) | 10 | 26 | 23% |

  So more than half the score currently rests on checks never seen to catch
  anything, concentrated in the invariants — `102a` ("an order cannot be placed
  in someone else's name by editing the request"), `108a`, `107a`, `4c`. A
  correct app and a vacuous assertion are indistinguishable from the outside.
  This does not skew backend comparisons — it hits all three equally — but it
  weakens any absolute claim, including SpacetimeDB's 50/50 first-try L1.

Exit criteria: a clean checkout bootstraps with one command; CI is green without
ignored `node_modules`; the same fixtures produce the same criterion outcomes in
three consecutive runs; every artifact identifies the run that produced it; and a
null build scores zero on every criterion that carries points.

### Phase 2 - freeze a fair L1-L2 baseline

Current execution order:

1. **Completed 2026-08-13:** qualify and promote ecommerce L1 on all three
   backends with repeated Docker reference, mutation, and null evidence.
2. **Completed 2026-08-13:** audit ecommerce L2 requirement coverage and remove
   false passes before collecting evidence. The contract linter must visit
   operations and fulfilment, every scored assertion must test what its
   description claims, and ambiguous controls stay at zero points.
3. **Completed 2026-08-13:** freeze separate L2 reference revisions for MongoDB,
   PostgreSQL, and SpacetimeDB. Do not edit the qualified L1 reference bytes in
   place.
4. **Completed 2026-08-13:** measure pristine appliance timing for all three
   stacks, review the generated limits, and bind the L2 pack budgets.
5. **Completed 2026-08-13:** run the exact L2 reference and mutation gates twice
   per backend plus the null gate, bind all 13 evidence slots, and promote the
   qualified recipe, fixture, packs, stacks, references, and alias.
6. **Completed 2026-08-15:** qualify the framework-neutral L1 1.1 and L2 1.2
   candidates before moving the promoted aliases. Each release now has two
   passing Docker reference and mutation repetitions per backend plus a passing
   null control, all 13 evidence slots are hash-bound to its exact identity, and
   machine qualification reports zero blockers. Their recipes, packs,
   calibrations, and stack support are qualified; L1/L2 now resolve to these
   framework-neutral versions, while the earlier releases are retired.
7. **Next:** execute a repeated frozen L1-to-L2 campaign and publish the exact
   scope and dispersion. Reconstruct other tracks only after this first product
   path is reproducible end to end.

- [ ] Require every point-bearing criterion to cite prompt text or declare a
  reviewed invariant rationale. Resolve the current `statedBy` warnings.
- [ ] Require a passing reference app and a caught mutation before promoting a
  criterion from zero points.
- [x] Re-run L1 on all three backends under the frozen harness and archive the
  raw qualification evidence with checksums.
- [x] Re-run L2 on all three backends under the frozen harness and archive the
  raw qualification evidence with checksums.
- [ ] Predeclare the repeated-run design and report dispersion; a single matched
  run remains exploratory rather than a headline comparison.

Exit criteria: every scored point is traceable and mutation-tested, no run is
contaminated or partially graded, and repeated L1-L2 results are stable enough to
serve as the upgrade baseline.

### Phase 3 - complete and promote each track's declared L3

The tracks deliberately reach different next properties because ecommerce L1-L2
already contains contention and warehouse operations:

- **Chat L3 â€” contended state:** reactions, polls, capacity, unique claims,
  pinned/ordering races, and history pagination racing live inserts. No L3
  contract or declared suite exists yet.
- **Ecommerce L3 â€” deferred work:** expiring reservations, scheduled restocks,
  automatic order progression and abandoned carts. Its prompt, contract,
  feature suite and invariant suite exist; the invariants perform two owned
  backend restarts and the level reruns the L1 systems suite. It remains
  experimental because no canonical L3 references or mutation evidence exist.

For both tracks, add exact actions where API-scale contention is required,
database-truth checks, canonical upgraded references, and backend-specific
mutants for each promoted guarantee. Re-run every L1-L2 suite against the L3
source rather than treating a new-level score as sufficient.

Exit criteria: each track's L3 prompt, contract, declared suites and actions are
complete; deferred work survives an owned restart and fires exactly once;
contention produces no denominator drift; all inherited guarantees pass in
repeated Docker runs; exact mutants are caught without collateral.

### Phase 4 - complete and promote each track's declared L4

- **Chat L4 â€” deferred and expiring work:** scheduled sends, reminders,
  expiration, exactly-once execution and restart survival.
- **Ecommerce L4 â€” personalisation at catalogue scale:** per-customer ranking,
  faceted search, stable pagination and database/UI agreement over a larger
  catalogue. No L4 prompt, contract or suite exists yet.
- Add live-data migration from each frozen L3 app; a reset may not erase the
  state whose survival or personalised derivation is under test.

Exit criteria: both L4 definitions have complete machine-verifiable suites;
scheduled work survives an owned restart and executes once; personalised
rankings match predeclared arithmetic for multiple viewers; upgrade data
survives without a harness reset masking the result.

### Phase 5 - complete and promote L5

L5 is production load and topology:

- Multi-server application topology for Postgres and MongoDB, with actors split
  across instances.
- Sustained open-loop storms, reconnect/resync under load, pagination during live
  inserts, propagation latency diagnostics and database/UI truth agreement.
- Record latency and resource metrics as diagnostics until their methodology is
  stable; correctness remains the scored axis.

Exit criteria: every backend uses its normal production mechanism, the control
fixture stays green, no result depends on a single lucky burst, and L1-L4 remain
fully regressed.

## Capability roadmap after the production gate

### 1. Multi-server

Two instances of the app's server against the same database, actors split
between them. The most common way a vibe-coded app dies on contact with users:
socket.io broadcasts do not cross instances without a redis adapter, so a
message sent through server A never reaches clients on server B. Postgres and
mongo apps pass with real tools (redis adapter, LISTEN/NOTIFY, change
streams); SpacetimeDB has no app tier to duplicate. Needs: the prompt to
require the server to honour a PORT override, bench.mjs to launch a second
instance, per-actor URLs in the grader. Design sketch in
`tracks/ecommerce/LEVELS.md`.

### 2. Live-data migration

Every suite currently resets the database, so "upgrade without losing data" is
never tested — and real apps ship v2 onto v1's data. Add upgrade criteria
where seeded-then-mutated state must survive the L1→L2 upgrade.

Stated honestly: this is a test SpacetimeDB might LOSE today. The friction log
already shows builds hitting the manual-migration abort. It goes in anyway — a
benchmark containing only tests we win is worthless at the door, and the
failures feed STDB-FRICTION.md with exactly what the product team needs.
(Note the build-time data policy in `backends/*.md` says seed data is
disposable WHILE BUILDING; migration criteria at upgrade time will explicitly
override it.)

### 3. Pagination racing live inserts

"Scroll back through history while new messages arrive" — the classic
offset-pagination bug class: duplicates and gaps at the fetch boundary. Every
real chat needs history; the race is the big sibling of the
enumeration-during-mutation criterion. Chat L3 material, with the history
feature spec'd first.

### 4. Injection floor

Nothing anywhere checks that a message containing markup renders as text. Will
probably saturate (React escapes by default) — fine; it is a floor, and a
security reviewer will notice its absence before they notice anything else.
Cheap invariant, both tracks.

### 5. Propagation latency as a diagnostic

The contention storms already exist; record convergence-time percentiles
during them and report alongside code size and dependency count. Measured,
never scored.

## Standing obligations before anything scores

- The systems promotion checklist in `tracks/ecommerce/LEVELS.md`: server-down
  verb built, counterfeit levers (no-op / API-calling / memory-patch) caught,
  a real build failed for a real reason.
- Contention denominator (0/0 vs 2/2) settled from preserved grading detail.
- Every withheld criterion follows contention's rule: zero points until it
  fails a real build AND its mutant is caught. No exceptions, including for
  criteria we expect SpacetimeDB to win.
- The same rule now applies to criteria that ALREADY score, not just withheld
  ones. A criterion that has never been observed failing at its own assertion
  has never been shown to assert anything. The null-build control catches
  boundary-level false positives cheaply; a setup-complete reference mutation
  is what proves the individual oracle.

## What is deliberately NOT here

More L1 surface area. Both L1s are the right size; discrimination now comes
from conditions, not features. And nothing whose only defense is "SpacetimeDB
happens to be good at it" — a criterion must be defensible as what a real
application needs to someone who has never heard of SpacetimeDB.

## Storm mode and database-truth grading (next after multi-server)

**Storms at API scale.** The grader already captures the app's own write
requests; `replayConcurrently` scaled to N≥64 copies is a load storm on
existing machinery for postgres/mongo. SpacetimeDB's ws writes are not
capturable that way (the known asymmetry): its storm drives ~16 browser
actors, and the diagnostic reports both concurrency levels honestly. Recipe:
seed tight (1 in stock, 1 seat), attack one state-changing endpoint at a
time, a fresh connection per request so keep-alive serializes nothing, and
sustained open-loop load where a single burst can hide the bug.

**Database-truth grading, schema-blind.** Extend the back-office spec with
READ subcommands (`report-stock <item>`, `count-orders`) emitting JSON — the
app ships its own DB reader, and the grader cross-checks three ways: DB truth
vs seed arithmetic vs what the UI renders. A UI papering over a corrupted
database stops being able to pass. UI grading stays: stale pages are half the
story.

**One-shot as the headline.** The unaided `firstBuild` score is the
correctness-by-default story and is already recorded; report it first, with
fix rounds as the separate correction story. A declared correction budget runs
through a flat round; regressive or evidence-losing changes are rolled back.
Every level records whether correction was unnecessary, succeeded, or exhausted
the budget. Report successful cost to correct separately from money spent on an
attempt that remained unresolved.

**Invariant additions:** blind overwrite (two sessions edit one entity; an
acknowledged write must never silently vanish), replay idempotency on money
paths, double-booking semantics once a booking-shaped surface exists (L3+).

## Concurrency bug taxonomy → criterion backlog

The recurring classes in one-shot AI backends, each mapped to where it lands
in our tracks. Every entry follows the standard rule: withheld until it fails
a real build and kills its mutant.

| class | shape | lands in |
|---|---|---|
| oversell | check-then-insert on a summed ledger; N pass one check | HAVE — 201a |
| double-book | two overlap checks read the same empty set | L3 booking surface |
| position collision | concurrent reorders assign one slot twice | chat L3 pinned/ordering |
| tally drift | read-modify-write counter; stored score ≠ rows | ecommerce reviews avg under storm; HAVE partial (review-average) |
| reversal replay | idempotent-once operation applied N× | refund/cancel path, ecommerce L2 money invariants |
| delete-vs-use race | resource deleted mid-checkout yet billed | ecommerce L2: admin deletes item during buy storm |
| duplicate allocation | waitlist/offer consumed twice | L3 booking |
| blind overwrite | two edits, last commit clobbers, both 200 | NEXT — cart qty from two tabs, admin stock edit vs restock |
| boot race / double seed | init runs concurrently, catalogue duplicated | HAVE partial — reseed-once rule in spec; add restart-storm criterion |
| pool deadlock, healthy /health | txn holds one conn, waits for another | storm mode diagnostic: liveness under N > pool size |
| control | an app with no shared mutable contention must stay clean | keep — proves storms don't fail everyone |

The control row matters most for fairness: a storm harness that fails every
app proves nothing. The card-game-shaped control is ours to add as a fixture.
