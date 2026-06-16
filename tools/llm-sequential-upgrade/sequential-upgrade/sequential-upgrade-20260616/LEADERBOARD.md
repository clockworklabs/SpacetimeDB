# Sequential Upgrade Benchmark — Leaderboard

**MongoDB** (this run, `20260616`) vs published **SpacetimeDB** / **PostgreSQL** (`20260406`).
Same model (**claude-sonnet-4-6**), same composed prompts, same feature spec, exhaustive
fix-to-100% at every level. Cost = Claude Code `cost_usd` (OTel), apples-to-apples.

_Last updated: through **L11** (L12 next — final level)._

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
| L12 Anonymous Migration | — | 12.62 | 19.68 |

_(SpacetimeDB / PostgreSQL L10–L12 are the published finish line; MongoDB fills in as we go.)_

> **Crossover at L8:** MongoDB led on cost L1–L7; SpacetimeDB overtook at L8 as the
> sync-heavy features (presence, threading) started costing Mongo fix cycles.

## Fix iterations (cumulative) — fewer is better

| Through | MongoDB | SpacetimeDB | PostgreSQL |
|---|---|---|---|
| L11 | 4 (L1, L7, L8, L10) | 1 (L1) | 8 |

> SpacetimeDB has been bug-free since L1. MongoDB took fixes at L7 (3 presence bugs),
> L8 (1 threading bug), and L10 (1 activity-decay bug). PostgreSQL's 8 fixes were
> front-loaded (worst at L1).
> **PG per-level caveat:** PG fix telemetry is all mislabeled `fix-level1`; cumulative
> totals are correct but per-level distribution is not (PG bug reports span L1–L7).

## Quality

All three backends at **100%** (full feature score) at every graded level.

## Time to complete (wall-clock) — lower is better

Sum of `totalDurationSec` across every Claude session (generate + each upgrade + each fix).

| | Through L11 (apples-to-apples) | Published full run (L12) |
|---|---|---|
| **MongoDB** | **66.9 min** — 15 runs (11 gen/upgrade + 4 fix) | _L12 pending_ |
| **SpacetimeDB** | **53.5 min** — 12 runs (11 + 1 fix) | 56.7 min — 13 runs |
| **PostgreSQL** | **76.8 min** — 19 runs (11 + 8 fix) | 84.4 min — 22 runs |

Same ranking as cost and fix-count: SpacetimeDB fastest, MongoDB middle, PostgreSQL slowest.
The spread tracks fix cycles — each fix is an extra session, so Mongo's 3 extra fixes vs STDB
explain most of the ~13-min gap through L11.

> ⚠️ **Least-rigorous metric.** Wall-clock folds in API latency / server load *at run time*.
> The published runs are April (`20260406`); this run is June (`20260616`) — any change in
> Sonnet 4.6 serving latency between those dates shows up here but not in tokens or (pricing-
> confirmed) dollars. Treat time as **directional/supporting**; lead with cost + fix-count +
> quality, which are environment-independent.

---

## Read so far

- **Cost:** MongoDB and SpacetimeDB are neck-and-neck (~$9, within ~3% at L8), both
  ~35% under PostgreSQL.
- **Bugs:** SpacetimeDB cleanest (1), MongoDB close (3), PostgreSQL a mess (8).
- **Trajectory:** the back half keeps stressing sync, where SpacetimeDB's built-in model
  holds its small lead. L10 (activity indicators) was a textbook example — Mongo's badge
  rose live but didn't decay without a server-side re-evaluation timer (same class of bug
  as L7 presence), costing a 4th fix cycle. L11 Draft Sync then passed clean (0 fixes),
  but STDB's lead widened on cost — $11.67 vs $12.94 at L11 — because Mongo's per-feature
  upgrade cost stays a bit higher even when bug-free. One level left: L12 Anonymous Migration.

_Bug-rate detail per level lives in each run's `level-N/BUG_REPORT.md`._
