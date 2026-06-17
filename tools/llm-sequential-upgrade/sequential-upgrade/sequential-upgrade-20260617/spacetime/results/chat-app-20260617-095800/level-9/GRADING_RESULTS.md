# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 8
**Grading Method:** Manual browser interaction
**Setup:** fresh 20260617 baseline — cleaned prompts + official skills + SpacetimeDB 2.6.0

---

## Feature 1: Basic Chat (Score: 3 / 3)
## Feature 2: Typing Indicators (Score: 3 / 3)
## Feature 3: Read Receipts (Score: 3 / 3)
## Feature 4: Unread Message Counts (Score: 3 / 3)
## Feature 5: Scheduled Messages (Score: 3 / 3)
## Feature 6: Ephemeral Messages (Score: 3 / 3)
## Feature 7: Message Reactions (Score: 3 / 3)
## Feature 8: Message Editing with History (Score: 3 / 3)
## Feature 9: Real-Time Permissions (Score: 3 / 3)
## Feature 10: Rich User Presence (Score: 3 / 3)
## Feature 11: Message Threading (Score: 3 / 3)
**Browser Test Observations:** Replies start threads, thread replies stay in the thread and
do NOT leak into the main room flow when a new main-room message is sent (the exact regression
that cost mongo a fix), parent shows a reply count, thread replies update live. Features 1–10
regression-checked, no regressions. Passed on first upgrade (no fix).

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1–10 | 3/3 each | |
| 11. Message Threading | 3/3 | new at L8 — clean where mongo leaked (1 fix) |
| **TOTAL** | **33/33** | |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L8 upgrade $1.61 (cumulative $11.98; ~+32% vs published, ~1.29x mongo — gap narrowing
as mongo's presence+threading fixes accumulate vs STDB's clean 8-for-8)
