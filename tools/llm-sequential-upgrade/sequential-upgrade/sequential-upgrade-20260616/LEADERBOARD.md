# Sequential Upgrade Benchmark — Leaderboard

**MongoDB** (this run, `20260616`) vs published **SpacetimeDB** / **PostgreSQL** (`20260406`).
Same model (**claude-sonnet-4-6**), same composed prompts, same feature spec, exhaustive
fix-to-100% at every level. Cost = Claude Code `cost_usd` (OTel), apples-to-apples.

_Last updated: through **L9** (L10 next)._

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
| L10 Activity Indicators | — | 11.27 | 16.53 |
| L11 Draft Sync | — | 11.67 | 17.47 |
| L12 Anonymous Migration | — | 12.62 | 19.68 |

_(SpacetimeDB / PostgreSQL L10–L12 are the published finish line; MongoDB fills in as we go.)_

> **Crossover at L8:** MongoDB led on cost L1–L7; SpacetimeDB overtook at L8 as the
> sync-heavy features (presence, threading) started costing Mongo fix cycles.

## Fix iterations (cumulative) — fewer is better

| Through | MongoDB | SpacetimeDB | PostgreSQL |
|---|---|---|---|
| L9 | 3 (L1, L7, L8) | 1 (L1) | 8 |

> SpacetimeDB has been bug-free since L1. MongoDB took fixes at L7 (3 presence bugs) and
> L8 (1 threading bug). PostgreSQL's 8 fixes were front-loaded (worst at L1).
> **PG per-level caveat:** PG fix telemetry is all mislabeled `fix-level1`; cumulative
> totals are correct but per-level distribution is not (PG bug reports span L1–L7).

## Quality

All three backends at **100%** (full feature score) at every graded level.

---

## Read so far

- **Cost:** MongoDB and SpacetimeDB are neck-and-neck (~$9, within ~3% at L8), both
  ~35% under PostgreSQL.
- **Bugs:** SpacetimeDB cleanest (1), MongoDB close (3), PostgreSQL a mess (8).
- **Trajectory:** the back half (private rooms, drafts, anon-migration) keeps stressing
  sync, where SpacetimeDB's built-in model is expected to hold its small lead.

_Bug-rate detail per level lives in each run's `level-N/BUG_REPORT.md`._
