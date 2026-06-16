# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 11
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
## Feature 12: Private Rooms & Direct Messages (Score: 3 / 3)
## Feature 13: Room Activity Indicators (Score: 3 / 3)
## Feature 14: Draft Sync (Score: 3 / 3)
**Browser Test Observations:** Unsent drafts persist per-room across navigation, sync live
across the same user's open tabs (type in one tab → appears in the other with no refresh),
survive a page reload (server-backed via `GET /api/drafts`), and clear everywhere on send.
Features 1–13 regression-checked, no regressions. Passed on first generate (no fix needed).

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
| 11. Message Threading | 3/3 | |
| 12. Private Rooms & DMs | 3/3 | |
| 13. Room Activity Indicators | 3/3 | |
| 14. Draft Sync | 3/3 | new at L11 |
| **TOTAL** | **42/42** | |

**Reprompt count:** 0 (passed on first generate)
**Cost:** L11 upgrade $1.04
