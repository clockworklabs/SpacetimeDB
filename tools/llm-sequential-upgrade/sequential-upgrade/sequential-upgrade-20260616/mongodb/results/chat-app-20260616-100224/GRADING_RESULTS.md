# Chat App Grading Results

**Model:** claude-sonnet-4-6
**Date:** 2026-06-16
**Backend:** mongodb
**Level:** 12 (final)
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
## Feature 15: Anonymous Migration (Score: 3 / 3)
**Browser Test Observations:** An anonymous user (auto-assigned `Anon_XXXX`, no signup) can
use the app fully — create/join rooms, send messages, react — then register a permanent name,
at which point all of their history migrates to the new identity: messages, room membership,
admin rights, reactions, read receipts, scheduled messages, drafts, invitations, and DMs. No
orphaned `Anon_` ghost remains, and a fresh anonymous user still gets a distinct name.
Features 1–14 regression-checked, no regressions. Passed on first generate (no fix needed).

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
| 14. Draft Sync | 3/3 | |
| 15. Anonymous Migration | 3/3 | new at L12 |
| **TOTAL** | **45/45** | |

**Reprompt count:** 0 (passed on first generate)
**Cost:** L12 upgrade $0.98

---

## Run complete — final tally (MongoDB, all 12 levels)

- **Quality:** 45/45 at L12; every graded level reached 100% (full feature score).
- **Cumulative cost:** $13.92 (Claude Code `cost_usd`, OTel).
- **Fix iterations:** 4 total — L1 (presence), L7 (3 presence bugs), L8 (thread-reply leak),
  L10 (activity-decay). L9, L11, L12 passed clean on first generate.
- **Wall-clock:** 70.3 min across 16 sessions (12 generate/upgrade + 4 fix).
