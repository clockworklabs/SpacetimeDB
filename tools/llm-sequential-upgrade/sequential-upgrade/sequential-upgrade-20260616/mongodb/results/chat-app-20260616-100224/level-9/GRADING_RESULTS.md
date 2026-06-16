# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 8
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
## Feature 11: Message Threading (Score: 3 / 3)
**Browser Test Observations:** Initially 2/3 — thread replies leaked into the main room chat
when a main-room message was sent (read endpoint didn't filter out replies). Fixed in iteration 3
(added `parentId: null` filter); re-graded clean: replies stay in the thread, reply count/preview
and real-time sync work. Features 1–10 regression-checked, no regressions.

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
| 10. Rich User Presence | 3/3 | |
| 11. Message Threading | 3/3 | new at L8; 1 bug fixed (iteration 3) |
| **TOTAL** | **33/33** | |

**Reprompt count:** 1 (thread replies leaking into main chat)
**Cost:** L8 upgrade $1.38 + fix $0.38 = $1.76
