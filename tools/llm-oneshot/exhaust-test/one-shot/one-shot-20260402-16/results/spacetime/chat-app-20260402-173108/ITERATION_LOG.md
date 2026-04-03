# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 7
- **Module:** chat-app-20260402-173108
- **Started:** 2026-04-02T17:31:08

---

## Iteration 0 — Initial Deploy (no browser testing yet)

**Status:** Build successful, dev server running on http://localhost:5173

**Build:** Compiled without errors (tsc + vite build both passed)
**Reprompts so far:** 0

### Features implemented:
1. Basic Chat — rooms, messages, online users, register/join/leave
2. Typing Indicators — setTyping/clearTyping reducers, server-side cleanup every 5s
3. Read Receipts — lastRead table, "Seen by X" under messages
4. Unread Message Counts — unread badge on room list
5. Scheduled Messages — scheduled_message_delivery table, cancel support
6. Ephemeral Messages — ephemeral_delete_job scheduled table, client-side countdown
7. Message Reactions — toggleReaction, emoji picker, grouped display with counts
8. Message Editing with History — editMessage reducer, message_edit_history table
9. Real-Time Permissions — kickUser/banUser/promoteUser, Admin badge
10. Rich User Presence — status selector (Online/Away/DND/Invisible), lastActive display, auto-away via awayChecker scheduled job

**Console errors:** Not yet tested (requires grading session)
**Browser testing:** Deferred to separate grading session

---

*Browser testing and fix iterations will be recorded here during the grading session.*
