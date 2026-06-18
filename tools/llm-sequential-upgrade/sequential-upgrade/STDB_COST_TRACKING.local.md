# SpacetimeDB Cost Tracking (per level)

Local working doc. Costs are from each run's `cost-summary.json` (`totalCostUsd`,
from Claude Code's `cost_usd` OTel attribute). All runs are **claude-sonnet-4-6**
(published April runs confirmed sonnet via cost/token ratio; current run confirmed
via raw telemetry).

## Per-level totals (generate/upgrade + any fix iterations)

| Level | Pub 20260403 | Pub 20260406 | Current 20260617-2 (post-fix) |
|-------|-------------|-------------|-------------------------------|
| L1    | $2.8412 (gen 1.3705 + 4 fix 1.4706) | $1.6457 (gen 0.8384 + 1 fix 0.8073) | **$1.6680** (0 fix) |
| L2    | $1.1751 | $1.1816 | **$1.0361** |
| L3    | $1.2901 | $0.9898 | **$1.2793** |
| L4    | $0.9492 | $0.5896 | **$0.8456** |
| L5    | $0.0574 ⚠ | $0.9215 | **$1.2242** |
| L6    | $1.1623 | $1.5011 | **$1.1723** |
| L7    | $0.7995 | $1.2543 | **$2.1762** (upg 1.69 + fix 0.48) |
| L8    | $0.8281 | $0.9762 | **$1.6049** |
| L9    | $1.7680 | $1.3869 | **$1.4577** |
| L10   | $0.5679 | $0.8211 | **$0.6776** |
| L11   | $1.0007 | $0.4019 | **$1.0669** |
| L12   | $0.8855 | $0.9498 | **$1.4402** |
| **Total** | **$13.3250** | **$12.6195** | **$15.6490** (L1–L12 COMPLETE) |

## Cumulative cost-to-done (the "as we go" comparison)

Running total through each level (includes fix iterations = true cost-to-done).

| Through | Current 20260617-2 | Pub 20260403 | Pub 20260406 | Current vs cheaper pub |
|---------|--------------------|--------------|--------------|------------------------|
| L1      | $1.67              | $2.84        | $1.65        | +$0.02 |
| L2      | $2.70              | $4.02        | $2.83        | −$0.13 |
| L3      | $3.98              | $5.31        | $3.82        | +$0.16 |
| L4      | $4.83              | $6.26        | $4.41        | +$0.42 |
| L5      | $6.05              | $6.32        | $5.33        | +$0.72 |
| L6      | $7.23              | $7.48        | $6.83        | +$0.40 |
| L7      | $9.40              | $8.28        | $8.08        | +$1.32 |
| L8      | $11.01             | $9.11        | $9.06        | +$1.95 |
| L9      | $12.46             | $10.88       | $10.45       | +$2.02 |
| L10     | $13.14             | $11.45       | $11.27       | +$1.87 |
| L11     | $14.21             | $12.45       | $11.67       | +$2.54 |
| L12     | $15.65             | $13.33       | $12.62       | +$3.03 |

_L7 includes a presence fix (multi-connection offline bug) — the **same** bug mongo was
dinged for. The published April runs did NOT fix it (no fix-level7 in their data; same code
pattern), so our higher L7 reflects holding STDB to the 100% bar they weren't held to._

So far the current (post-fix, 0-fix) run is **tracking at or below both published runs on
cost-to-done**, because the published runs spent on L1 fix cycles that the current run didn't need.

## vs Mongo — the true apples-to-apples (both fixed to 100%)

Mongo (Express + Mongoose + Socket.io, run 20260616) was also graded to 100% with
exhaustive fixes — the fairest comparison to the current STDB run. The published April
STDB runs were NOT held to that bar.

| Lv | feature | STDB current | Mongo |
|----|---------|--------------|-------|
| L1 | basic | $1.67 | $1.14 |
| L2 | scheduled | $1.04 | $0.78 |
| L3 | ephemeral | $1.28 | $0.77 |
| L4 | reactions | $0.85 | $0.58 |
| L5 | editing | $1.22 | $0.57 |
| L6 | permissions | $1.17 | $0.90 |
| L7 | presence | $2.18 | $2.79 |
| L8 | threading | $1.60 | $1.76 |
| L9 | private/DM | $1.46 | $1.45 |
| L10 | activity | $0.68 | $1.15 |
| L11 | drafts | $1.07 | $1.04 |
| L12 | anon | $1.44 | $0.98 |
| **Total** | | **$15.65** (1 fix) | **$13.92** (4 fixes) |

- STDB +$1.73 (+12%) on cost, but **1 fix vs mongo's 4** (mongo: L1, L7, L8, L10).
- On the bug-magnet levels STDB was **cheaper to-done**: L7 ($2.18 vs $2.79; mongo's fix
  alone was $1.59/3 bugs), L8 ($1.60 vs $1.76), L10 ($0.68 vs $1.15).
- STDB's premium is concentrated in heavy-but-clean features (L1, L5, L12) = the SDK-2.6
  output cost, not debugging.

## Setup differences (important for fair comparison)

- **Published April runs (20260403, 20260406):** `rules: standard`, SpacetimeDB ~2.0.x SDK,
  buggy templates (missing typescript devDep + `moduleResolution: node`). May NOT have been
  fixed to the same exhaustive 100% bar (GRADING_RESULTS never published; sample data shows
  some 2/3s) — so their totals are "cost to published scores," not necessarily "cost to 100%."
- **Current run (20260617-2):** `rules: guided` + full official skills, SpacetimeDB 2.6.0,
  **fixed templates**, graded to 100% (3/3 every feature), 0 fix iterations so far.

## Notes

- The "$0.84 published L1" figure cited in earlier notes was a **single session** — the
  20260406 L1 *generate* ($0.8384), NOT the L1 cost-to-done. Both published L1s actually
  cost ~$1.6–2.8 once their fix sessions are included.
- Published 20260403 L5 = $0.0574 / 2 calls — a near-no-op (likely broken/trivial upgrade).
- Published L1 "fixes" were scattered across the run timeline (some after later levels were
  built), so summing them as "L1 cost-to-done" overstates L1; the clean comparison is
  generate-to-generate.
- Clean generate/upgrade comparison at matching levels: current is in line with published
  per-level (e.g. L2 current $1.04 < both published $1.18); the SDK-2.6 premium shows up
  mainly on the L1 generate.
