# Chat App Grading Results

**Date:** 2026-01-28
**Platform:** PostgreSQL (Express + Drizzle ORM + Socket.io + React)
**AI Model:** Grok Code

---

## Overall Metrics

| Metric                  | Value                      |
| ----------------------- | -------------------------- |
| **Prompt Level Used**   | 5 (05_edit_history.md)     |
| **Features Evaluated**  | 1-8 (max 15)               |
| **Total Feature Score** | 11.25 / 24                 |
| **Percentage**          | 46.9%                      |

- [x] Compiles without errors
- [x] Runs without crashing
- [ ] First-try success

| Metric                   | Value                                                                   |
| ------------------------ | ----------------------------------------------------------------------- |
| Lines of code (backend)  | 731                                                                     |
| Lines of code (frontend) | 1,252                                                                   |
| Number of files created  | 27                                                                      |
| External dependencies    | drizzle-orm, postgres, express, socket.io, react, vite, socket.io-client |

---

## Feature 1: Basic Chat Features (Score: 0.75 / 3)

- [x] Users can set a display name (0.5)
- [ ] Users can create chat rooms (0) — _rooms exist but don't isolate messages, making them non-functional_
- [ ] Users can join/leave rooms (0) — _room visibility required 2 reprompts; rooms still don't work properly_
- [ ] Users can send messages to joined rooms (0) — _messages appear in all rooms bug_
- [x] Online users are displayed (0.25) — _list shows but count always wrong_
- [ ] Basic validation exists (0) — _member count always shows 1_

**Note:** Rooms that don't isolate messages defeat the entire purpose of the feature. Creating a "room" that behaves identically to no room is not functional.

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 0 / 3)

- [ ] System tracks which users have seen which messages (0) — _backend exists but not user-facing_
- [ ] "Seen by X, Y, Z" indicator displays under messages (0) — _not displayed in UI_
- [ ] Read status updates in real-time (0) — _cannot verify without UI_

**Note:** Backend-only implementation without UI has zero user-facing value. Users cannot see read receipts, so this feature is effectively non-existent.

---

## Feature 4: Unread Message Counts (Score: 0 / 3)

- [ ] Unread count badge shows on room list (0)
- [ ] Count tracks last-read position per user per room (0)
- [ ] Counts update in real-time (0)

---

## Feature 5: Scheduled Messages (Score: 0 / 3)

- [ ] Users can compose and schedule messages for future delivery (0)
- [ ] Pending scheduled messages visible to author with cancel option (0)
- [ ] Message appears in room at scheduled time (0)

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 2.5 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1)
- [1] Message is permanently deleted when timer expires (0.5) — _UI sync required 1 reprompt_

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [x] Hover/click shows who reacted (0.75)

---

## Feature 8: Message Editing with History (Score: 2 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [ ] Edit history is viewable by other users (0) — _not accessible in UI_
- [x] Edits sync in real-time to all viewers (0.5)

---

## Features 9-15: Not Evaluated

_Not included in prompt level 5_

- Feature 9: Real-Time Permissions — N/A
- Feature 10: Rich User Presence — N/A
- Feature 11: Message Threading — N/A
- Feature 12: Private Rooms & Direct Messages — N/A
- Feature 13: Room Activity Indicators — N/A
- Feature 14: Draft Sync — N/A
- Feature 15: Anonymous to Registered Migration — N/A

---

## Summary Score Sheet

| Feature                  | Max    | Score   | Reprompts |
| ------------------------ | ------ | ------- | --------- |
| 1. Basic Chat            | 3      | 0.75    | 2         |
| 2. Typing Indicators     | 3      | 3       | 0         |
| 3. Read Receipts         | 3      | 0       | 0         |
| 4. Unread Counts         | 3      | 0       | 0         |
| 5. Scheduled Messages    | 3      | 0       | 1         |
| 6. Ephemeral Messages    | 3      | 2.5     | 1         |
| 7. Message Reactions     | 3      | 3       | 0         |
| 8. Message Editing       | 3      | 2       | 0         |
| **TOTAL**                | **24** | **11.25** | **4**   |

---

## Known Issues (Critical)

1. **Messages appear in all rooms** — New messages show up in every room instead of just the target room
2. **Member count always 1** — Room member count doesn't update when users join
3. **Unread badges not showing** — Badge counts not displaying on room list
4. **Scheduled messages broken** — Cannot schedule messages for future delivery
5. **No "Seen by" UI** — Read receipts tracked in database but not displayed to users
6. **No edit history view** — Edit history stored in database but not accessible in UI

## Infrastructure/Config Issues (Required Reprompts)

1. **Drizzle command syntax** — `drizzle-kit push` → `drizzle-kit push:pg`
2. **Missing postgres package** — `postgres` (postgres-js) not in dependencies
3. **Drizzle config invalid** — Wrong driver/dialect configuration
4. **Index syntax error** — `.desc()` not supported in Drizzle Kit index definitions
5. **Environment variables not loaded** — Missing `dotenv.config()` before DB connection
6. **Missing unique constraints** — `onConflictDoUpdate` required `uniqueIndex` instead of `index`
7. **Room visibility broken** — Rooms only visible to creator; required broadcast fix
8. **Ephemeral UI sync missing** — Server deleted messages but client UI not updated

---

## Technical Notes

- **Backend:** Express.js with Socket.io for real-time, Drizzle ORM for PostgreSQL
- **Frontend:** React with Vite, Socket.io-client
- **Auth:** Simple username-based (no passwords)
- **Database:** PostgreSQL 15 (Docker)
- **Real-time:** Socket.io for all bidirectional communication
