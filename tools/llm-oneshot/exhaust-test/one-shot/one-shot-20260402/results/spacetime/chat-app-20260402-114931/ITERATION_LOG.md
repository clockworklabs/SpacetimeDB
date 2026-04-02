# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 7
- **Started:** 2026-04-02T11:49:31

---

## Iteration 0 — Initial Deploy (11:49)

**Status:** Deploy complete, no browser testing yet (separate grading session)

**Build summary:**
- Backend: Published module `chat-app-20260402-114931` to SpacetimeDB local
- Bindings: Generated TypeScript bindings (10 tables, 17 reducers)
- Client: TypeScript check passed, production build succeeded
- Dev server: Running at http://localhost:5173

**Features implemented:**
1. Basic Chat — users, rooms (create/join/leave), messages, online status display
2. Typing Indicators — updateTyping reducer + 5s auto-expire via cleanup timer
3. Read Receipts — markRead reducer, seenBy display under messages
4. Unread Message Counts — userRoomRead table + badges on room list
5. Scheduled Messages — scheduledMessage table with SpacetimeDB scheduler, cancel support
6. Ephemeral Messages — isEphemeral + expiresAt, countdown timer, deleted by cleanup
7. Message Reactions — toggleReaction (add/remove), reaction counts with hover names
8. Message Editing with History — editMessage reducer, messageEdit history table, (edited) indicator
9. Real-Time Permissions — kickUser, banUser, promoteToAdmin reducers, admin panel UI
10. Rich User Presence — status (online/away/dnd/invisible), lastActive, auto-away after 5m

**Reprompts during build:** 0 (one-shot success — force-republish due to pre-existing module)
**Build errors fixed:** 2 TypeScript compilation errors fixed (unused Map cast, unused variable)

---
