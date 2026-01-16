# Chat App Grading Results

**Date:** 2026-01-08
**Platform:** PostgreSQL (Drizzle ORM + Express + Socket.io + React)
**AI Model:** GPT 5.2

---

## Overall Metrics

| Metric | Value |
|--------|-------|
| **Prompt Level Used** | 5 (05_postgres_edit_history.md) |
| **Features Evaluated** | 1-8 (max 15) |
| **Total Feature Score** | 10 / 24 |
| **Percentage** | 42% |

- [x] Compiles without errors
- [x] Runs without crashing
- [ ] First-try success (required Docker CMD fix for tsx module resolution)

| Metric | Value |
|--------|-------|
| Lines of code (backend) | ~500 |
| Lines of code (frontend) | ~640 |
| Number of files created | 18 |
| External dependencies | drizzle-orm, postgres, express, socket.io, jsonwebtoken, react, socket.io-client |

---

## Feature 1: Basic Chat Features (Score: 1.5 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [ ] Users can join/leave rooms (0.5) — **No room discovery; can only see own rooms**
- [x] Users can send messages to joined rooms (0.5) — *works but no error feedback*
- [ ] Online users are displayed (0.5) — **Broken (stale closure in socket handler)**
- [ ] Basic validation exists (0.5) — *Server-side exists but errors not shown to user*

---

## Feature 2: Typing Indicators (Score: 0 / 3)

- [ ] Typing state is broadcast to other room members (1) — **Stale closure breaks handler**
- [ ] Typing indicator auto-expires after inactivity (1) — *Server-side timeout exists*
- [ ] UI shows "User is typing..." or "Multiple users are typing..." (1) — **Never triggers due to stale closure**

---

## Feature 3: Read Receipts (Score: 1 / 3)

- [x] System tracks which users have seen which messages (1) — *Backend works correctly*
- [ ] "Seen by X, Y, Z" indicator displays under messages (1) — **Doesn't update in real-time**
- [ ] Read status updates in real-time (1) — **Stale closure breaks it**

---

## Feature 4: Unread Message Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [x] Counts update in real-time (1) — *Only working real-time feature (uses function updater)*

---

## Feature 5: Scheduled Messages (Score: 2 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [x] Pending scheduled messages visible to author with cancel option (1)
- [ ] Message appears in room at scheduled time (1) — **Only appears after refresh (stale closure)**

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 1.5 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1) — *Partial: countdown exists*
- [ ] Message is permanently deleted when timer expires (1) — **Server job works but real-time update broken**

---

## Feature 7: Message Reactions (Score: 1 / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [ ] Reaction counts display and update in real-time (0.75) — **Stale closure breaks it**
- [x] Users can toggle their own reactions on/off (0.75) — *Works locally*
- [ ] Hover/click shows who reacted (0.75) — **Untested**

---

## Feature 8: Message Editing with History (Score: 0 / 3)

- [ ] Users can edit their own messages (1) — **Untested**
- [ ] "(edited)" indicator shows on edited messages (0.5) — **Untested**
- [ ] Edit history is viewable by other users (1) — **Untested**
- [ ] Edits sync in real-time to all viewers (0.5) — **Would be broken by stale closure**

---

## Features 9-15: Not Evaluated

*Not included in prompt level 5*

- Feature 9: Real-Time Permissions — N/A
- Feature 10: Rich User Presence — N/A
- Feature 11: Message Threading — N/A
- Feature 12: Private Rooms & Direct Messages — N/A
- Feature 13: Room Activity Indicators — N/A
- Feature 14: Draft Sync — N/A
- Feature 15: Anonymous to Registered Migration — N/A

---

## Summary Score Sheet

| Feature | Max | Score |
|---------|-----|-------|
| 1. Basic Chat | 3 | 1.5 |
| 2. Typing Indicators | 3 | 0 |
| 3. Read Receipts | 3 | 1 |
| 4. Unread Counts | 3 | 3 |
| 5. Scheduled Messages | 3 | 2 |
| 6. Ephemeral Messages | 3 | 1.5 |
| 7. Message Reactions | 3 | 1 |
| 8. Message Editing | 3 | 0 |
| **TOTAL** | **24** | **10** |

---

## Known Issues

### Critical Bugs

1. **Stale Closure in Socket Handlers** — All socket event handlers in `client/src/App.tsx` (lines 140-177) are registered in a `useEffect` with empty deps `[]`. The `activeRoomId` variable is captured at mount time (when it's `null`) and never updates. This breaks ALL real-time features except unread counts.

2. **No Error Handling in handleSend** — `client/src/App.tsx` lines 227-240. API calls to `sendMessage()` and `sendEphemeral()` have no try/catch. Errors silently fail with no user feedback.

3. **Room Discovery Missing** — `server/src/index.ts` lines 70-104. `GET /rooms` only returns rooms user is already a member of. No endpoint to discover other users' rooms.

### Missing Features

4. **No Online Users Display** — Would work if stale closure was fixed, but currently broken.

5. **Edit History Untested** — Backend stores history, UI exists, but not verified due to other bugs.

### Minor Issues

6. **Unread counts only real-time feature** — Works because it uses `setUnreadByRoomId((prev) => ...)` function updater pattern which doesn't rely on stale closure.

---

## Deployment Issues

The application did not work on first deployment:

1. **Docker CMD Module Resolution** — Server container crashed with `ERR_MODULE_NOT_FOUND`. Had to change Dockerfile CMD from `npm start` to use `tsx` via a new `serve` script.

2. **TypeScript Config** — `rootDir` in tsconfig.json caused compilation issues, had to remove it.

3. **Vite Types Missing** — Client needed `vite-env.d.ts` for `import.meta.env` types.

---

## Technical Notes

- **Backend:** Node.js + Express + Socket.IO + Drizzle ORM
- **Database:** PostgreSQL 16 (Docker)
- **Frontend:** React 18 + Vite + Socket.IO client
- **Auth:** JWT tokens
- **Real-time:** Socket.IO rooms for message broadcasting — *server broadcasts correctly*
- **Schema:** 8 tables (users, rooms, roomMembers, messages, messageEdits, reactions, scheduledMessages, roomReadPositions)
- **Periodic Tasks:** setInterval for scheduled message delivery and ephemeral cleanup

**The server-side is well-implemented. The critical failure is in client-side socket handling.**
