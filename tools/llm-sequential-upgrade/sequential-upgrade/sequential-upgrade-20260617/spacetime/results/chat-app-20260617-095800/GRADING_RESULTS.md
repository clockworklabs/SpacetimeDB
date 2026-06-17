# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 10
**Grading Method:** Manual browser interaction
**Setup:** fresh 20260617 baseline — cleaned prompts + official skills + SpacetimeDB 2.6.0

---

## Features 1–12 (Score: 3 / 3 each)
## Feature 13: Room Activity Indicators (Score: 3 / 3)
**Browser Test Observations:** Badge rises (🔥 Hot = ≥5 msgs/2min, ⚡ Active = ≥1 msg/5min) and
DECAYS live when the room goes quiet (Hot→Active ~2min, Active→none ~5min) without a refresh —
the exact decay mongo failed (mongo's badge stuck until refresh → 1/3 there). Features 1–12
regression-checked, no regressions. Passed on first upgrade (no fix).

**Implementation note (for the writeup — be accurate):** STDB computed activity CLIENT-side —
it derives Hot/Active from the auto-synced `message` table (reactive `useTable`) and a 30s
`setActivityTick` re-render drives the decay. It did NOT use a server-side scheduled reducer
for this feature. The "win" vs mongo is real but the mechanism is client-side derivation over
STDB's live local replica, not a server-feature showcase. (STDB's server scheduled tables ARE
used elsewhere: scheduled messages L2, ephemeral expiry L3, auto-away checker L7.)

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1–12 | 3/3 each | |
| 13. Room Activity Indicators | 3/3 | new at L10 — decays live (client-side); mongo needed a fix here |
| **TOTAL** | **39/39** | |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L10 upgrade $0.82 (cumulative $14.59; ~+29% vs published, ~1.23x mongo). 10-for-10, 0 fixes.
