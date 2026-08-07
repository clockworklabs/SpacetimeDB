# What this benchmark still has to prove, and in what order

The thesis under test: as a task approaches real production conditions,
SpacetimeDB pulls ahead structurally — the database does the hard parts, so
correctness arrives by construction and the model spends less getting there.
The harness's job is to make that measurable in a way a hostile reviewer
cannot dismantle. Fairness here means one thing: **every criterion is winnable
by an expert human on every stack, using that stack's own real tools.** The
benchmark's product is the bill.

## Next, in order

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
fix rounds as the separate cost-to-correct story.

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
