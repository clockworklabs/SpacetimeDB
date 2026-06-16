# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 2
**Grading Method:** Manual browser interaction

---

## Feature 1: Basic Chat (Score: 3 / 3)
**Browser Test Observations:** All criteria pass; presence regression-checked (multi-tab fix holds).

## Feature 2: Typing Indicators (Score: 3 / 3)
**Browser Test Observations:** Real-time, auto-expires.

## Feature 3: Read Receipts (Score: 3 / 3)
**Browser Test Observations:** Real-time; sender excluded from own seen-by.

## Feature 4: Unread Message Counts (Score: 3 / 3)
**Browser Test Observations:** Accurate, live, clears on open.

## Feature 5: Scheduled Messages (Score: 3 / 3)
- [x] Compose and schedule a message for future delivery
- [x] Pending scheduled messages visible to author with cancel option
- [x] Message appears in the room at the scheduled time
**Browser Test Observations:** Scheduling, pending list with cancel, and timed delivery all work.

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

**Reprompt count:** 0 (passed on first generate)
**Cost:** L2 upgrade $0.78
