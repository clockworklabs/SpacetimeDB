# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 2
**Grading Method:** Manual browser interaction
**Setup:** fresh 20260617 baseline — cleaned prompts + official skills + SpacetimeDB 2.6.0

---

## Feature 1: Basic Chat (Score: 3 / 3)
## Feature 2: Typing Indicators (Score: 3 / 3)
## Feature 3: Read Receipts (Score: 3 / 3)
## Feature 4: Unread Message Counts (Score: 3 / 3)
## Feature 5: Scheduled Messages (Score: 3 / 3)
**Browser Test Observations:** Schedule a message for a future time → hidden until then,
delivered automatically live at the scheduled time; pending scheduled messages visible to
the sender and cancellable. Exercises STDB scheduled tables + ScheduleAt correctly.
Features 1–4 regression-checked, no regressions. Passed on first upgrade (no fix needed).

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | 3/3 | |
| 2. Typing Indicators | 3/3 | |
| 3. Read Receipts | 3/3 | |
| 4. Unread Counts | 3/3 | |
| 5. Scheduled Messages | 3/3 | new at L2 |
| **TOTAL** | **15/15** | |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L2 upgrade $1.26 (in-line with published ~$1.18; the L1 generate spike looks like
from-scratch variance, not a systematic skill cost)
