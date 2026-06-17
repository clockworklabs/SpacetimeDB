# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 3
**Grading Method:** Manual browser interaction
**Setup:** fresh 20260617 baseline — cleaned prompts + official skills + SpacetimeDB 2.6.0

---

## Feature 1: Basic Chat (Score: 3 / 3)
## Feature 2: Typing Indicators (Score: 3 / 3)
## Feature 3: Read Receipts (Score: 3 / 3)
## Feature 4: Unread Message Counts (Score: 3 / 3)
## Feature 5: Scheduled Messages (Score: 3 / 3)
## Feature 6: Ephemeral Messages (Score: 3 / 3)
**Browser Test Observations:** Ephemeral messages send with a TTL, display during their
lifetime, and auto-delete live (no refresh) for all participants when the TTL elapses;
gone server-side on reload. Features 1–5 regression-checked, no regressions. Passed on
first upgrade (no fix needed).

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | 3/3 | |
| 2. Typing Indicators | 3/3 | |
| 3. Read Receipts | 3/3 | |
| 4. Unread Counts | 3/3 | |
| 5. Scheduled Messages | 3/3 | |
| 6. Ephemeral Messages | 3/3 | new at L3 |
| **TOTAL** | **18/18** | |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L3 upgrade $1.24 (cumulative $4.66; ~+22% vs published, L1 generate the main contributor)
