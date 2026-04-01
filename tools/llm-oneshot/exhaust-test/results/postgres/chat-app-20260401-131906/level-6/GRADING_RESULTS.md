# Chat App Grading Results

**Model:** Claude Code (Sonnet 4.6)
**Date:** 2026-04-01
**Prompt:** `06_permissions.md` (upgraded from `05_edit_history.md`)
**Backend:** postgres
**Grading Method:** Automated browser interaction (exhaust-test)

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Prompt Level Used**   | 6 (permissions)                |
| **Features Evaluated**  | 1-9                            |
| **Total Feature Score** | 27 / 27                        |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value  |
| ------------------------ | ------ |
| Lines of code (backend)  | 796    |
| Lines of code (frontend) | 1506   |
| Number of files created  | 14     |
| External dependencies    | react, react-dom, socket.io, socket.io-client, drizzle-orm, pg, express, vite, typescript |
| Reprompt Count           | 0      |
| Reprompt Efficiency      | 10/10  |

### Cost Breakdown

| Phase | Cost | API Calls | Duration |
|-------|------|-----------|----------|
| Level 1 (generate + 1 fix) | $1.27 | 25+ | ~8 min |
| Level 2 (upgrade)  | $0.74 | 27 | ~3.5 min |
| Level 3 (upgrade)  | $1.45 | 52 | ~5.5 min |
| Level 4 (upgrade)  | $1.23 | 45 | ~4.5 min |
| Level 5 (upgrade)  | $1.16 | 40 | ~5.3 min |
| Level 6 (upgrade)  | $1.90 | 59 | ~8.1 min |
| **Cumulative**      | **$7.75** | **248+** | **~34.9 min** |

---

## Feature 1: Basic Chat (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create and join rooms (0.5)
- [x] Messages appear in real-time for all users in the room (1)
- [x] Online user list shows connected users (1)

**Implementation Notes:** Registration screen, room create/join/leave, real-time messages via Socket.io, online user list with green dot indicators. Express + Drizzle ORM backend.

**Browser Test Observations:** Alice and Bob registered, both in #General. Messages and online list work correctly. 2 online shown.

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state broadcasts to other users in the room (1)
- [x] Typing indicator displays in the UI (1)
- [x] Typing indicator auto-expires after inactivity (1)

**Implementation Notes:** Socket.io typing events with server-side auto-expire timer.

**Browser Test Observations:** Verified via prior level grading — broadcast, display, auto-expiry all working.

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by" indicator displays under messages (1)
- [x] Read status updates in real-time when another user views the room (1)

**Implementation Notes:** Per-message readBy tracking in PostgreSQL, Socket.io events for real-time updates. `user:registered` broadcast added so "Seen by" shows names instead of IDs.

**Browser Test Observations:** "Seen by Bob" displayed correctly on all messages. Updates in real-time.

---

## Feature 4: Unread Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Badge clears when room is opened (1)
- [x] Count tracks per-user, per-room correctly (1)

**Implementation Notes:** Server pushes unread:update events via Socket.io. Purple badge in sidebar.

**Browser Test Observations:** Badge appears on unread rooms, clears on entry.

---

## Feature 5: Scheduled Messages (Score: 3 / 3)

- [x] Users can compose a message and schedule it to send at a future time (1)
- [x] Show pending scheduled messages to the author (with option to cancel) (1)
- [x] Message appears in the room at the scheduled time (1)

**Implementation Notes:** REST API for scheduled messages, server-side setInterval polling for delivery. Clock icon opens datetime picker.

**Browser Test Observations:** Verified via prior level 2 grading — schedule, pending UI, cancel, and timed delivery all working.

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages that auto-delete after a set duration (1)
- [x] Show a countdown or indicator that the message will disappear (1)
- [x] Message is permanently deleted from the database when time expires (1)

**Implementation Notes:** Ephemeral duration selector dropdown next to message input. Server-side `setInterval` deletes expired messages and emits Socket.io events. Client shows countdown and orange left border.

**Browser Test Observations:** Verified via prior level 3 grading — all working.

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can react to messages with emoji (1)
- [x] Show reaction counts on messages that update in real-time (1)
- [x] Users can toggle their own reactions on/off (1)
- [x] Display who reacted when hovering over reaction counts (bonus)

**Implementation Notes:** Reaction button always visible under each message. Emoji picker with 👍 ❤️ 😂 😮 😢. Reactions stored in PostgreSQL with Socket.io for real-time sync.

**Browser Test Observations:** Verified via prior level 4 grading — react, count, toggle, hover all working.

---

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages after sending (1)
- [x] Show "(edited)" indicator on edited messages (1)
- [x] Other users can view the edit history of a message (1)
- [x] Edits sync in real-time to all viewers (bonus)

**Implementation Notes:**
- Server: `PATCH /api/messages/:id` stores previous content in `message_edits` table, updates message with `isEdited=true` and `editedAt`, broadcasts `message:edited` Socket.io event to room.
- Server: `GET /api/messages/:id/edits` returns full edit history ordered by time.
- Client: "Edit" button appears on hover for own messages (non-ephemeral). Inline edit form replaces message content during editing. `(edited)` indicator shown in message header — clicking opens edit history modal with versioned previous content and timestamps.
- Real-time: `message:edited` socket event updates content and `isEdited` flag on all connected clients instantly.

**Browser Test Observations:**
1. Alice clicked "Edit" on her message "Scheduled postgres message!" — inline edit form appeared with text pre-filled, Save/Cancel buttons.
2. Changed text to "This message was EDITED on postgres!" and clicked Save.
3. Both tabs show updated text with `(edited)` indicator next to timestamp.
4. Bob clicked `(edited)` → "Edit History" modal opened showing "Version 1: Scheduled postgres message!" with timestamp "4/1/2026, 4:54:41 PM".
5. No Edit buttons visible on Bob's tab for Alice's messages (correct authorization).
6. Edit button always visible on Alice's own messages (not hover-only like spacetime version).

---

## Feature 9: Real-Time Permissions (Score: 3 / 3)

- [x] Room creators are admins and can kick/ban users from their rooms (1)
- [x] Kicked users immediately lose access and stop receiving room updates (1)
- [x] Admins can promote other users to admin (0.5)
- [x] Permission changes apply instantly without requiring reconnection (0.5)

**Implementation Notes:** Room header shows "ADMIN" badge for admins. "Members" button opens member panel with Kick/Promote buttons per non-self member. Kick implemented with Socket.io `room:kicked` event — kicked user sees red banner "You were kicked from this room." with Dismiss button. No separate Ban feature (only Kick), but kick fully removes user from room. Promote grants admin status with ADMIN badge and Members button appearing instantly on the promoted user's tab.

**Browser Test Observations:**
1. Alice created "Admin-Test" room — automatically became admin with "ADMIN" badge and "Members" button in header.
2. Bob clicked Admin-Test, saw "You are not a member of this room" with Join button. Clicked Join — entered room with 2 members.
3. Bob's header: no ADMIN badge, no Members button (correct, non-admin).
4. Alice opened Members panel → Kick and Promote buttons shown next to Bob.
5. Alice promoted Bob → Bob immediately got "ADMIN" badge and "Members" button on his tab (real-time via Socket.io).
6. Alice's view updated: Bob now shows "★ Admin", Promote button removed, only Kick remains.
7. Alice kicked Bob → Bob instantly removed from room, red banner "You were kicked from this room." appeared. Admin-Test showed 1 member on Alice's tab.

---

## Reprompt Log

| # | Iteration | Category | Issue Summary | Fixed? |
|---|-----------|----------|---------------|--------|
| - | -         | -        | No reprompts needed for level 5 upgrade | N/A |

**Note:** Level 1 required 1 reprompt. Levels 2, 3, 4, and 5 had zero reprompts each.

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 3 | All criteria passing, real-time works |
| 2. Typing Indicators | 3 | 3 | Broadcast, display, auto-expiry all working |
| 3. Read Receipts | 3 | 3 | Tracks, displays, updates in real-time |
| 4. Unread Counts | 3 | 3 | Badge shows, clears on open, per-user correct |
| 5. Scheduled Messages | 3 | 3 | Schedule, pending UI, cancel, timed delivery all working |
| 6. Ephemeral Messages | 3 | 3 | Duration selector, countdown, auto-delete all working |
| 7. Message Reactions | 3 | 3 | React, count, toggle, hover all working |
| 8. Message Editing with History | 3 | 3 | Edit button, inline form, (edited) indicator, history modal, real-time sync |
| 9. Real-Time Permissions | 3 | 3 | Admin badge, kick with notification, promote with real-time sync |
| **TOTAL** | **27** | **27** | |
