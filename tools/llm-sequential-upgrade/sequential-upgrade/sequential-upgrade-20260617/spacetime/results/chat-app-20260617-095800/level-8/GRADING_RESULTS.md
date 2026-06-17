# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-17
**Backend:** spacetime
**Level:** 7
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
**Browser Test Observations:** Status (online/away/dnd/invisible), last-active timestamps,
and auto-away (5-min idle threshold, 60s checker) all work; presence reflects real connection
state — a closed tab flips to offline live (no refresh), the failure mode that cost mongo 3
fixes. last-active VALUE updates live via subscription on activity (the relative "X ago"
display doesn't self-tick between updates, but that's cosmetic — not refresh-gated, so 3/3
per rubric). Features 1–9 regression-checked, no regressions. Passed on first upgrade (no fix).

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1–9 | 3/3 each | |
| 10. Rich User Presence | 3/3 | new at L7 — clean where mongo needed 3 fixes |
| **TOTAL** | **30/30** | |

**Reprompt count:** 0 (passed on first upgrade)
**Cost:** L7 upgrade $2.08 (cumulative $10.37; ~+28% vs published, ~1.37x mongo — gap narrowed
as mongo's L7 presence fixes kicked in vs STDB's clean pass)
