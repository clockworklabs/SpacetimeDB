# Sequential Upgrade Benchmark — Leaderboard

**MongoDB** (this run, `20260616`) vs published **SpacetimeDB** / **PostgreSQL** (`20260406`).
Same model (**claude-sonnet-4-6**), same composed prompts, same feature spec, exhaustive
fix-to-100% at every level. Cost = Claude Code `cost_usd` (OTel), apples-to-apples.

_**RUN COMPLETE** — MongoDB finished all 12 levels (L1–L12). Final tally below._

---

## Cumulative cost ($) — lower is better

| Level (feature added) | MongoDB | SpacetimeDB | PostgreSQL |
|---|---|---|---|
| L1  Basic/Typing/Receipts/Unread | **1.14** | 1.65 | 6.26 |
| L2  Scheduled Messages | **1.92** | 2.83 | 6.92 |
| L3  Ephemeral Messages | **2.69** | 3.82 | 7.98 |
| L4  Message Reactions | **3.27** | 4.41 | 8.83 |
| L5  Message Editing | **3.85** | 5.33 | 9.73 |
| L6  Real-Time Permissions | **4.75** | 6.83 | 11.00 |
| L7  Rich Presence | **7.54** | 8.08 | 12.27 |
| L8  Message Threading | 9.30 | **9.06** | 14.19 |
| L9  Private Rooms & DMs | 10.75 | **10.45** | 15.96 |
| L10 Activity Indicators | 11.90 | **11.27** | 16.53 |
| L11 Draft Sync | 12.94 | **11.67** | 17.47 |
| L12 Anonymous Migration | 13.92 | **12.62** | 19.68 |

> **Final cost:** MongoDB **$13.92**, SpacetimeDB **$12.62**, PostgreSQL **$19.68**.
> Mongo finishes ~10% above SpacetimeDB and ~29% under PostgreSQL.

> **Crossover at L8:** MongoDB led on cost L1–L7; SpacetimeDB overtook at L8 as the
> sync-heavy features (presence, threading) started costing Mongo fix cycles, and held
> the lead through L12.

## Fix iterations (cumulative) — fewer is better

| Through | MongoDB | SpacetimeDB | PostgreSQL |
|---|---|---|---|
| L12 (final) | 4 (L1, L7, L8, L10) | 1 (L1) | 10 |

> SpacetimeDB took a single fix at L1 and was clean thereafter. MongoDB took fixes at
> L1 (presence), L7 (3 presence bugs), L8 (thread-reply leak), and L10 (activity-decay).
> PostgreSQL needed 10 fix sessions — heavily front-loaded.
> **Counting caveats:** (1) PG's fix telemetry is mislabeled — 8 of 10 sessions are tagged
> `fix-level1` but the bug reports span L1–L7 (+2 at L12); cumulative cost/time are correct,
> per-level distribution is not. (2) MongoDB's 4 excludes two failed/no-edit attempts (a
> killed run at L6 and an API-500 at L8 that produced zero changes); PG's 10 are as-published
> and some may be retries. So treat fix *count* as directional — cost and time (which sum
> actual sessions regardless of label) are the rigorous metrics.

## Quality

All three backends at **100%** (full feature score) at every graded level.

## Time to complete (wall-clock) — lower is better

Sum of `totalDurationSec` across every Claude session (generate + each upgrade + each fix).

| | Full run (L1–L12) |
|---|---|
| **MongoDB** | **70.3 min** — 16 runs (12 gen/upgrade + 4 fix) |
| **SpacetimeDB** | **56.7 min** — 13 runs (12 + 1 fix) |
| **PostgreSQL** | **84.4 min** — 22 runs (12 + 10 fix) |

Same ranking as cost and fix-count: SpacetimeDB fastest (56.7), MongoDB middle (70.3),
PostgreSQL slowest (84.4). The spread tracks fix cycles — each fix is an extra session, so
Mongo's 3 extra fixes vs STDB explain most of the ~14-min gap. Mongo finishes ~24% slower
than STDB and ~17% faster than PG.

> ⚠️ **Least-rigorous metric.** Wall-clock folds in API latency / server load *at run time*.
> The published runs are April (`20260406`); this run is June (`20260616`) — any change in
> Sonnet 4.6 serving latency between those dates shows up here but not in tokens or (pricing-
> confirmed) dollars. Treat time as **directional/supporting**; lead with cost + fix-count +
> quality, which are environment-independent.

---

## Final read (run complete)

- **Quality:** all three backends reached **100%** at every graded level (Mongo 45/45 at L12).
- **Cost:** SpacetimeDB **$12.62**, MongoDB **$13.92** (~10% above STDB), PostgreSQL **$19.68**
  (Mongo ~29% under PG). Mongo led L1–L7; STDB overtook at L8 and held to the finish as the
  sync-heavy features started costing Mongo fix cycles.
- **Fix cycles:** SpacetimeDB 1, MongoDB 4, PostgreSQL 10 (directional — see caveats above).
- **Time:** STDB 56.7 min, Mongo 70.3, PG 84.4 (same ranking; least-rigorous metric).
- **Why STDB leads the two DB stacks:** its built-in sync means the real-time features
  (presence, threading, activity, permissions) come "for free," whereas Mongo/PG must wire
  every broadcast and decay timer by hand — which is where Mongo's 4 fix cycles landed
  (L1 presence, L7 presence, L8 threading, L10 activity-decay). Mongo and PG sit in the same
  "manual real-time" camp; Mongo is the cheaper, cleaner of the two by a wide margin.

_Bug-rate detail per level lives in each run's `level-N/BUG_REPORT.md`._
