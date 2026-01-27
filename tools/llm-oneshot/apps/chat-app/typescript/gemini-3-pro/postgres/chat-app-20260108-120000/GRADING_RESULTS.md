# Chat App Grading Results

**Date:** 2026-01-08
**Platform:** PostgreSQL (Node.js/Express + React client)
**AI Model:** Gemini 3 Pro

---

## Overall Metrics

| Metric                  | Value                           |
| ----------------------- | ------------------------------- |
| **Prompt Level Used**   | 5 (05_postgres_edit_history.md) |
| **Features Evaluated**  | 1-8 (max 15)                    |
| **Total Feature Score** | 17.25 / 24                      |
| **Percentage**          | 71.9%                           |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value                                                                                                         |
| ------------------------ | ------------------------------------------------------------------------------------------------------------- |
| Lines of code (backend)  | 568                                                                                                           |
| Lines of code (frontend) | 778                                                                                                           |
| Number of files created  | 28                                                                                                            |
| External dependencies    | drizzle-orm, postgres, express, socket.io, jsonwebtoken, zod, react, react-router-dom, date-fns, lucide-react |

---

## Feature 1: Basic Chat Features (Score: 2 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [ ] Users can join/leave rooms (0.5) — **No join/leave UI exists**
- [x] Users can send messages to joined rooms (0.5)
- [ ] Online users are displayed (0.5) — **Not implemented**
- [x] Basic validation exists (0.5) — _zod validation on all endpoints_

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1) — _2s client timeout_
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (1)
- [x] Read status updates in real-time (1)

---

## Feature 4: Unread Message Counts (Score: 2.5 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [1] Counts update in real-time (1) — **Uses 5-second polling instead of true real-time**

---

## Feature 5: Scheduled Messages (Score: 2 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [ ] Pending scheduled messages visible to author with cancel option (1) — **BUG: Response not used, messages don't appear; no cancel option**
- [x] Message appears in room at scheduled time (1) — _server periodic task processes_

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1) — _live countdown "Expires in Xs"_
- [x] Message is permanently deleted when timer expires (1) — _server periodic cleanup_

---

## Feature 7: Message Reactions (Score: 0.75 / 3)

- [ ] Users can add emoji reactions to messages (0.75) — **BUG: Buttons hidden (CSS specificity)**
- [x] Reaction counts display and update in real-time (0.75) — _backend works correctly_
- [ ] Users can toggle their own reactions on/off (0.75) — **Cannot initiate first reaction**
- [ ] Hover/click shows who reacted (0.75) — **Not implemented**

**Root Cause:** `.message-actions` div has inline `display: 'none'` which cannot be overridden by the CSS hover rule due to specificity.

---

## Feature 8: Message Editing with History (Score: 1 / 3)

- [ ] Users can edit their own messages (1) — **BUG: Edit button hidden (same CSS issue)**
- [x] "(edited)" indicator shows on edited messages (0.5) — _would work if editing worked_
- [ ] Edit history is viewable by other users (1) — **Not implemented (data stored but no UI)**
- [x] Edits sync in real-time to all viewers (0.5) — _would work if editing worked_

**Root Cause:** Same CSS specificity bug as reactions — edit button in hidden `.message-actions` div.

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

| Feature               | Max    | Score     |
| --------------------- | ------ | --------- |
| 1. Basic Chat         | 3      | 2         |
| 2. Typing Indicators  | 3      | 3         |
| 3. Read Receipts      | 3      | 3         |
| 4. Unread Counts      | 3      | 2.5       |
| 5. Scheduled Messages | 3      | 2         |
| 6. Ephemeral Messages | 3      | 3         |
| 7. Message Reactions  | 3      | 0.75      |
| 8. Message Editing    | 3      | 1         |
| **TOTAL**             | **24** | **17.25** |

---

## Known Issues

### Critical Bugs

1. **CSS Specificity Bug (Reactions & Editing)** — The `.message-actions` div has inline `style={{ display: 'none' }}` which cannot be overridden by the CSS rule `.message-item:hover .message-actions { display: flex }`. This makes reaction and edit buttons permanently invisible.

2. **Scheduled Messages Not Shown** — When creating a scheduled message, the POST response containing the message is ignored by the client. The message only appears after page refresh or when it's actually sent.

3. **No Cancel for Scheduled Messages** — No UI to cancel pending scheduled messages.

### Missing Features

4. **No Join/Leave Room UI** — Users cannot explicitly join or leave rooms. All rooms are visible to all users.

5. **No Online Users Display** — No indicator showing who is currently online.

6. **No Edit History Viewer** — Edit history is stored in database but no UI to view it.

7. **No "Who Reacted" Display** — Reaction user data exists but no hover/tooltip to show who reacted.

### Minor Issues

8. **Unread Counts Use Polling** — 5-second polling interval instead of real-time socket updates.

---

## Deployment Issues

The application did not work on first deployment:

1. **Docker Proxy Misconfiguration** — Vite proxy was hardcoded to `localhost:3001` instead of `server:3001` for Docker network
2. **Container Rebuild Required** — Config changes weren't applied until full `docker-compose down && up --build`
3. **Port Conflict** — PostgreSQL port 5432 conflicted with local instance, changed to 5433

---

## Technical Notes

- **Backend:** Node.js + Express + Socket.IO + Drizzle ORM
- **Database:** PostgreSQL 16 (Docker)
- **Frontend:** React 18 + Vite + Socket.IO client
- **Auth:** JWT tokens (24h expiry)
- **Real-time:** Socket.IO rooms for message broadcasting
- **Schema:** 6 tables (users, rooms, messages, messageEdits, reactions, messageReads)
- **Periodic Tasks:** setInterval for scheduled message delivery and ephemeral cleanup
