# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 12 (final)
**Grading Method:** Manual browser interaction
**Setup:** 20260617 — cleaned prompts + **full official skills** (typescript-server + typescript-client) + SpacetimeDB 2.6.0

---

## Features 1–14 (Score: 3 / 3 each)
## Feature 15: Anonymous Migration (Score: 3 / 3)
**Browser Test Observations:** An anonymous user can use the app fully, then register a
permanent name with all history migrating to the registered identity; no orphaned anonymous
record; a fresh anonymous user gets a distinct identity. Features 1–14 regression-checked,
no regressions. Passed on first upgrade (no fix).

---

## Summary

| Feature | Score |
|---------|-------|
| 1–14 | 3/3 each |
| 15. Anonymous Migration | 3/3 |
| **TOTAL** | **45/45** |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L12 upgrade $1.96

---

## Run complete — final tally (SpacetimeDB, 20260617, full-skill variant)

- **Quality:** 45/45 at L12; every level reached 100% on first pass.
- **Fix iterations:** 0 (clean every level — incl. mongo's bug levels L7 presence, L8 threading, L10 activity).
- **Cumulative cost:** $17.78.
- **Ruleset:** full official skills (typescript-server 257 + typescript-client 108 lines) + 2.6.0 SDK.
- **Note:** treated as the full-skill reference run; a focused-ruleset variant (parity in scope
  with the mongo/pg backend files) is the intended fair-comparison baseline.
