# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 7
**Grading Method:** Manual browser interaction

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
**Browser Test Observations:** Initially 2/3 — three presence bugs (offline users not
shown; "last active" frozen at "just now"; auto-away never triggered). Fixed in iteration 2;
re-graded clean: offline users now listed with last-active, the timestamp ages, and auto-away
fires after inactivity. Features 1–9 regression-checked, no regressions.

---

## Summary

| Feature | Score | Notes |
|---------|-------|-------|
| 1. Basic Chat | 3/3 | |
| 2. Typing Indicators | 3/3 | |
| 3. Read Receipts | 3/3 | |
| 4. Unread Counts | 3/3 | |
| 5. Scheduled Messages | 3/3 | |
| 6. Ephemeral Messages | 3/3 | |
| 7. Message Reactions | 3/3 | |
| 8. Message Editing | 3/3 | |
| 9. Real-Time Permissions | 3/3 | |
| 10. Rich User Presence | 3/3 | new at L7; 3 bugs fixed (iteration 2) |
| **TOTAL** | **30/30** | |

**Reprompt count:** 1 (presence: offline-list, last-active aging, auto-away)
**Cost:** L7 upgrade $1.20 + fix $1.59 = $2.79
