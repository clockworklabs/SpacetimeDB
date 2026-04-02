# Chat App Grading Results

**Model:** Claude Code (Sonnet 4.6)
**Date:** 2026-04-01
**Prompt:** `09_private_rooms.md` (upgraded from `08_threading.md`)
**Backend:** spacetime
**Grading Method:** Automated browser interaction (exhaust-test)

---

## Overall Metrics

| Metric                  | Value                          |
| ----------------------- | ------------------------------ |
| **Prompt Level Used**   | 9 (private rooms)              |
| **Features Evaluated**  | 1-12                           |
| **Total Feature Score** | 36 / 36                        |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value  |
| ------------------------ | ------ |
| Lines of code (backend)  | 595 (index.ts 399 + schema.ts 196) |
| Lines of code (frontend) | 1038 (+ auto-gen bindings) |
| Number of files created  | 84     |
| External dependencies    | react, react-dom, spacetimedb, vite, @vitejs/plugin-react, typescript |
| Reprompt Count           | 0      |
| Reprompt Efficiency      | 10/10  |

### Cost Breakdown

| Phase | Cost | API Calls | Duration |
|-------|------|-----------|----------|
| Level 1 (generate) | $1.20 | 21 | ~11 min |
| Level 2 (upgrade)  | $0.79 | 30 | ~5 min  |
| Level 3 (upgrade)  | $1.54 | 56 | ~6 min  |
| Level 4 (upgrade)  | $1.14 | 41 | ~4 min  |
| Level 5 (upgrade)  | $0.43 | 16 | ~1.7 min |
| Level 6 (upgrade)  | $1.99 | 62 | ~8.4 min |
| Level 7 (upgrade)  | $1.27 | 40 | ~8.4 min |
| Level 8 (upgrade)  | $2.18 | 21 | ~6 min |
| Level 9 (upgrade)  | $2.05 | 62 | ~6.5 min |
| **Cumulative**      | **$12.59** | **349** | **~57 min** |

---

## Feature 1: Basic Chat (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create and join rooms (0.5)
- [x] Messages appear in real-time for all users in the room (1)
- [x] Online user list shows connected users (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state broadcasts to other users in the room (1)
- [x] Typing indicator displays in the UI (1)
- [x] Typing indicator auto-expires after inactivity (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by" indicator displays under messages (1)
- [x] Read status updates in real-time when another user views the room (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 4: Unread Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Badge clears when room is opened (1)
- [x] Count tracks per-user, per-room correctly (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 5: Scheduled Messages (Score: 3 / 3)

- [x] Users can compose a message and schedule it to send at a future time (1)
- [x] Show pending scheduled messages to the author (with option to cancel) (1)
- [x] Message appears in the room at the scheduled time (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages that auto-delete after a set duration (1)
- [x] Show a countdown or indicator that the message will disappear (1)
- [x] Message is permanently deleted from the database when time expires (1)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can react to messages with emoji (1)
- [x] Show reaction counts on messages that update in real-time (1)
- [x] Users can toggle their own reactions on/off (1)
- [x] Display who reacted when hovering over reaction counts (bonus — title attribute)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages after sending (1)
- [x] Show "(edited)" indicator on edited messages (1)
- [x] Other users can view the edit history of a message (1)
- [x] Edits sync in real-time to all viewers (bonus)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 9: Real-Time Permissions (Score: 3 / 3)

- [x] Room creators are admins and can kick/ban users from their rooms (1)
- [x] Kicked users immediately lose access and stop receiving room updates (1)
- [x] Admins can promote other users to admin (0.5)
- [x] Permission changes apply instantly without requiring reconnection (0.5)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 10: Rich User Presence (Score: 3 / 3)

- [x] Users can set status: online, away, do-not-disturb, invisible (1)
- [x] "Last active X minutes ago" shows for offline users (0.5)
- [x] Status changes sync to all viewers in real-time (1)
- [x] Auto-set to "away" after inactivity period (0.5)

**Browser Test Observations:** Verified in prior levels.

---

## Feature 11: Message Threading (Score: 3 / 3)

- [x] Users can reply to specific messages, creating a thread (1)
- [x] Show reply count and preview on parent messages (1)
- [x] Threaded view to see all replies to a message (1)
- [x] New replies sync in real-time to thread viewers (bonus)

**Implementation Notes:** `threadReply` table in SpacetimeDB with `parentMessageId`, `roomId`, `sender`, `text`, `sentAt`. `sendThreadReply` reducer validates membership/ban status. Client: `💬` button on each message (hover) opens a side panel showing the parent message, reply count divider, all replies with sender/time, and a reply input field. Reply count badge ("💬 N") appears on the button. Subscribes to `SELECT * FROM thread_reply`.

**Browser Test Observations:**
1. Alice sent "Hello everyone! Let's test threading." — message appeared in #General.
2. Hovered over message — 💬 button appeared alongside ✏️ edit and reaction picker.
3. Clicked 💬 — Thread panel opened on right side showing parent message, "0 REPLIES", "No replies yet. Start the thread!", and "Reply to thread..." input.
4. Alice typed "First reply from Alice!" and clicked Reply — reply appeared in thread panel, counter updated to "1 REPLY", badge on parent message showed "💬 1".
5. Bob joined General, hovered over message — saw "💬 1" badge, clicked it — thread panel opened showing Alice's reply.
6. Bob typed "Bob's reply in the thread!" and clicked Reply — reply appeared on both tabs instantly, counter updated to "2 REPLIES", badge updated to "💬 2".
7. No app-related console errors during threading test.

---

## Feature 12: Private Rooms & DMs (Score: 3 / 3)

- [x] Users can create private rooms that are hidden from non-members (1)
- [x] Room creators can invite users; invitees accept/decline (1)
- [x] Users can open direct messages (DMs) with any online user (1)

**Implementation Notes:** `isPrivate` and `isDm` fields on `room` table. `roomInvitation` table with `inviterIdentity`, `inviteeIdentity`, `roomId`, `status`. `inviteToRoom` reducer validates admin + private room. `acceptInvitation` auto-joins room. `openDm` creates private DM room with auto-accepted invitations for both parties. Client filters private rooms by membership, shows 🔒 + "private" badge, 💬 DM button on hover in user list.

**Browser Test Observations:**
1. Alice clicked "+" → room creation form showed "Room name..." input and "Private room" checkbox.
2. Alice typed "Secret-Room", checked "Private room", clicked Create → room appeared with 🔒 icon and "private" badge in sidebar.
3. Bob's tab showed "No rooms yet. Create one!" — private room correctly hidden from non-members.
4. Alice entered Secret-Room → header showed "# Secret-Room 1 members" with "Manage" and "+ Invite" buttons.
5. Alice clicked "+ Invite" → "INVITE USER BY IDENTITY" panel with "Select user..." dropdown appeared. Selected Bob → clicked "Send Invite".
6. Bob's sidebar instantly showed "INVITATIONS 1" section with "Secret-Room from Alice" and Accept/Decline buttons (real-time via SpacetimeDB subscription).
7. Bob clicked Accept → Secret-Room appeared in his room list with 🔒 icon and "private" badge. Invitation section disappeared. Alice's header updated to "2 members".
8. Bob hovered over Alice in user list → 💬 DM button appeared. Clicked it → "💬 Alice & Bob" DM room created on both tabs simultaneously.
9. No app-related console errors during private rooms/DM testing.

---

## Reprompt Log

| # | Iteration | Category | Issue Summary | Fixed? |
|---|-----------|----------|---------------|--------|
| - | -         | -        | No reprompts needed | N/A |

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 3 | All criteria passing |
| 2. Typing Indicators | 3 | 3 | All working |
| 3. Read Receipts | 3 | 3 | Real-time updates |
| 4. Unread Counts | 3 | 3 | Badge + clear working |
| 5. Scheduled Messages | 3 | 3 | All working |
| 6. Ephemeral Messages | 3 | 3 | Countdown + auto-delete |
| 7. Message Reactions | 3 | 3 | React, count, toggle, hover all working |
| 8. Message Editing with History | 3 | 3 | Edit, (edited) badge, history panel, real-time sync |
| 9. Real-Time Permissions | 3 | 3 | Promote, kick, ban all working with real-time sync |
| 10. Rich User Presence | 3 | 3 | Status selector, last active, real-time sync, auto-away |
| 11. Message Threading | 3 | 3 | Thread panel, reply count badge, real-time sync all working |
| 12. Private Rooms & DMs | 3 | 3 | Private creation, invite flow, DM via user list, real-time sync |
| **TOTAL** | **36** | **36** | |
