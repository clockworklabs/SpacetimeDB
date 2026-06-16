# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 10
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
**Browser Test Observations:** Activity badge rises live (⚡ Active after ≥1 msg in 2 min,
🔥 Hot after ≥5 msgs in 5 min) on the room list with no refresh, AND decays live when the
room goes quiet — the badge steps down / clears on its own as the 2-min / 5-min windows
expire. One fix iteration: initial generate raised the badge but never lowered it without a
manual refresh (no server-side periodic re-evaluation). Fixed by adding a 15-second timer
that re-evaluates each tracked room's activity level and broadcasts `room-activity` on decay.
Features 1–12 regression-checked, no regressions.

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
| 13. Room Activity Indicators | 3/3 | new at L10; activity-decay bug fixed in iteration 4 |
| **TOTAL** | **39/39** | |

**Reprompt count:** 1 (activity-decay fix)
**Cost:** L10 upgrade $0.75 + fix $0.40 = $1.15
