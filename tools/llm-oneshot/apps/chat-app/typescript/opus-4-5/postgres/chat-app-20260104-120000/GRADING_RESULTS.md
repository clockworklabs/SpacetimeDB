# Chat App Grading Results - PostgreSQL Implementation

**Date:** 2026-01-04
**Prompt Level:** 9 (Private Rooms)
**AI Model:** Claude Opus 4.5

---

## Overall Metrics

| Metric | Value |
|--------|-------|
| **Prompt Level Used** | 9 |
| **Features Evaluated** | 1-12 |
| **Total Feature Score** | 27.5 / 36 |
| **Percentage** | 76.4% |
| **Reprompts** | 0 |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success (worked without manual fixes)

| Metric | Value |
|--------|-------|
| Lines of code (backend) | 1,689 |
| Lines of code (frontend) | 2,849 |
| Number of files created | 23 |
| External dependencies | drizzle-orm, postgres, express, socket.io, jsonwebtoken, node-cron, react, socket.io-client |

---

## Feature 1: Basic Chat Features (Score: 2.0 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5) - **BUG: Room appears twice in list**
- [x] Users can join/leave rooms (0.5) - Note: Only non-admin can leave
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.5) - **BUG: Only in private rooms**
- [x] Basic validation exists (0.5)

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 0 / 3)

- [ ] System tracks which users have seen which messages (1) - **Not real-time; only updates on room re-entry**
- [ ] "Seen by X, Y, Z" indicator displays under messages (1) - **Not real-time**
- [ ] Read status updates in real-time (1) - **Not working**

---

## Feature 4: Unread Message Counts (Score: 0 / 3)

- [ ] Unread count badge shows on room list (1)
- [ ] Count tracks last-read position per user per room (1)
- [ ] Counts update in real-time (1)

---

## Feature 5: Scheduled Messages (Score: 3 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [x] Pending scheduled messages visible to author with cancel option (1)
- [x] Message appears in room at scheduled time (1)

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1)
- [x] Message is permanently deleted when timer expires (1)

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [x] Hover/click shows who reacted (0.75)

---

## Feature 8: Message Editing with History (Score: 2.5 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.5) - **BUG: History modal doesn't refresh in real-time**

---

## Feature 9: Real-Time Permissions (Score: 2 / 3)

- [x] Room creator is admin and can kick/ban users (0.5) — *kick exists but doesn't force-disconnect socket*
- [ ] Kicked users immediately lose access and stop receiving updates (0) — *client removes UI but socket stays subscribed*
- [x] Admins can promote other users to admin (0.5)
- [x] Permission changes apply instantly (0.5)

**Note:** Code review shows client handles `room:kicked` by removing room from UI but does NOT emit `room:leave` to leave the socket.io room. Kicked users continue receiving messages until page refresh.

---

## Feature 10: Rich User Presence (Score: 3 / 3)

- [x] Users can set status: online, away, do-not-disturb, invisible (1)
- [x] "Last active X minutes ago" shows for offline users (0.5)
- [x] Status changes sync to all viewers in real-time (1)
- [x] Auto-set to "away" after inactivity period (0.5)

---

## Feature 11: Message Threading (Score: 3 / 3)

- [x] Users can reply to specific messages, creating a thread (1) - Note: Replies also show in main view (Discord-like)
- [x] Parent messages show reply count and preview (0.5)
- [x] Threaded view shows all replies to a message (1)
- [x] New replies sync in real-time to thread viewers (0.5)

---

## Feature 12: Private Rooms & Direct Messages (Score: 3 / 3)

- [x] Users can create private/invite-only rooms (0.75)
- [x] Room creators can invite specific users by username (0.75)
- [x] Direct messages (DMs) between two users work (0.75)
- [x] Only members can see private room content and member lists (0.75)

---

## Features Not Evaluated (Not in Prompt Level 9)

- Feature 13: Room Activity Indicators - N/A
- Feature 14: Draft Sync - N/A
- Feature 15: Anonymous to Registered Migration - N/A

---

## Summary Score Sheet

| Feature | Max | Score | Reprompts |
|---------|-----|-------|-----------|
| 1. Basic Chat | 3 | 2.0 | 0 |
| 2. Typing Indicators | 3 | 3 | 0 |
| 3. Read Receipts | 3 | 0 | 0 |
| 4. Unread Counts | 3 | 0 | 0 |
| 5. Scheduled Messages | 3 | 3 | 0 |
| 6. Ephemeral Messages | 3 | 3 | 0 |
| 7. Message Reactions | 3 | 3 | 0 |
| 8. Message Editing | 3 | 2.5 | 0 |
| 9. Real-Time Permissions | 3 | 2 | 0 |
| 10. Rich Presence | 3 | 3 | 0 |
| 11. Message Threading | 3 | 3 | 0 |
| 12. Private Rooms & DMs | 3 | 3 | 0 |
| **TOTAL** | **36** | **27.5** | **0** |

---

## Known Bugs

1. **Room duplication**: Created room appears twice in the room list
2. **Online users**: Only displayed in private rooms, not public rooms
3. **Read receipts**: Not updating in real-time, only on room re-entry
4. **Unread counts**: Not working
5. **Edit history modal**: Doesn't refresh when new edits are made while open
6. **Kicked users still connected**: Client removes room from UI but socket remains subscribed to room channel — kicked users continue receiving messages until page refresh

---

## Scoring Philosophy

Scores reflect **user-facing functionality**, not implementation effort:
- Features with critical bugs that break the user flow receive minimal/zero credit
- "Code exists" ≠ "feature works"
- A broken flow is worse than no flow (confuses users)