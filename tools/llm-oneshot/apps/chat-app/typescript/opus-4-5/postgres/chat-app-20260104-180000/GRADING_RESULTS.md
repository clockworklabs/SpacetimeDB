# Chat App Grading Results

**Date:** 2026-01-04
**Platform:** PostgreSQL (Express + Drizzle ORM + Socket.io + React)
**AI Model:** Claude Opus 4.5

---

## Overall Metrics

| Metric | Value |
|--------|-------|
| **Prompt Level Used** | 9 (09_postgres_private_rooms.md) |
| **Features Evaluated** | 1-12 (max 15) |
| **Total Feature Score** | 23.0 / 36 |
| **Percentage** | 63.9% |

- [x] Compiles without errors
- [x] Runs without crashing
- [ ] First-try success (multiple real-time sync issues, reactions broken)

| Metric | Value |
|--------|-------|
| Lines of code (backend) | 1,131 |
| Lines of code (frontend) | 2,222 |
| Number of files created | 20 |
| External dependencies | drizzle-orm, postgres, express, socket.io, jsonwebtoken, cors, react, socket.io-client |

---

## Feature 1: Basic Chat Features (Score: 2.0 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [x] Users can join/leave rooms (0) — *need to refresh before joining; shows many duplicates of room name while showing current room contents — basically unusable until refresh*
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.25) — *not shown in Members view*
- [x] Basic validation exists (0.25)

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 2.5 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (0.5) — *names constantly switch positions (e.g., "ted, bradley" → "bradley, ted" flipping back and forth)*
- [x] Read status updates in real-time (1)

---

## Feature 4: Unread Message Counts (Score: 1.0 / 3)

- [x] Unread count badge shows on room list (0.5) — *inconsistent*
- [x] Count tracks last-read position per user per room (0.5) — *inconsistent*
- [ ] Counts update in real-time (0) — *do not go to 0 when entering room*

---

## Feature 5: Scheduled Messages (Score: 1.5 / 3)

- [x] Users can compose and schedule messages for future delivery (0.5) — *very slow to respond after clicking send*
- [ ] Pending scheduled messages visible to author with cancel option (0) — *takes a while to show up and cannot cancel*
- [x] Message appears in room at scheduled time (1) — *does not update in realtime, may need refresh*

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1)
- [x] Message is permanently deleted when timer expires (1)

---

## Feature 7: Message Reactions (Score: 0 / 3)

- [ ] Users can add emoji reactions to messages (0)
- [ ] Reaction counts display and update in real-time (0)
- [ ] Users can toggle their own reactions on/off (0)
- [ ] Hover/click shows who reacted (0)

*Feature appears completely non-functional*

---

## Feature 8: Message Editing with History (Score: 2.75 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.25) — *edits don't update in realtime when history window is open*

---

## Feature 9: Real-Time Permissions (Score: 1.25 / 3)

- [x] Room creator is admin and can kick/ban users (1)
- [ ] Kicked users immediately lose access and stop receiving updates (0)
- [x] Admins can promote other users to admin (0.25) — *very laggy to update*
- [ ] Permission changes apply instantly (0) — *not working*

---

## Feature 10: Rich User Presence (Score: 2.5 / 3)

- [x] Users can set status: online, away, do-not-disturb, invisible (1)
- [x] "Last active X minutes ago" shows for offline users (0.5)
- [x] Status changes sync to all viewers in real-time (0.5) — *does not show up in the Members view*
- [x] Auto-set to "away" after inactivity period (0.5)

---

## Feature 11: Message Threading (Score: 1.5 / 3)

- [x] Users can reply to specific messages, creating a thread (0.5) — *thread disappears when page is refreshed*
- [x] Parent messages show reply count and preview (0.5) — *does not update in realtime*
- [x] Threaded view shows all replies to a message (0.5) — *delay to load, not realtime*
- [ ] New replies sync in real-time to thread viewers (0)

---

## Feature 12: Private Rooms & Direct Messages (Score: 2.0 / 3)

- [x] Users can create private/invite-only rooms (0.75)
- [x] Room creators can invite specific users by username (0.25) — *users cannot accept or decline invites properly*
- [x] Direct messages (DMs) between two users work (0.75)
- [x] Only members can see private room content and member lists (0.25)

---

## Features 13-15: Not Evaluated

*Not included in prompt level 9*

- Feature 13: Room Activity Indicators — N/A
- Feature 14: Draft Sync — N/A
- Feature 15: Anonymous to Registered Migration — N/A

---

## Summary Score Sheet

| Feature | Max | Score | Reprompts |
|---------|-----|-------|-----------|
| 1. Basic Chat | 3 | 2.0 | 0 |
| 2. Typing Indicators | 3 | 3 | 0 |
| 3. Read Receipts | 3 | 2.5 | 0 |
| 4. Unread Counts | 3 | 1.0 | 0 |
| 5. Scheduled Messages | 3 | 1.5 | 0 |
| 6. Ephemeral Messages | 3 | 3 | 0 |
| 7. Message Reactions | 3 | 0 | 0 |
| 8. Message Editing | 3 | 2.75 | 0 |
| 9. Real-Time Permissions | 3 | 1.25 | 0 |
| 10. Rich Presence | 3 | 2.5 | 0 |
| 11. Message Threading | 3 | 1.5 | 0 |
| 12. Private Rooms & DMs | 3 | 2.0 | 0 |
| **TOTAL** | **36** | **23.0** | **0** |

---

## Known Issues (Critical)

1. **Room join/leave broken** — Need to refresh page before joining; shows many duplicate room names while keeping current room contents visible — unusable without refresh
2. **Message reactions completely broken** — Feature appears non-functional
3. **Read receipt name flicker** — Names in "Seen by X, Y" constantly swap positions back and forth
4. **Unread counts don't clear** — Badge counts do not go to 0 when entering a room
5. **Scheduled messages slow/broken** — Very slow response after clicking send; cannot cancel scheduled messages
6. **Kicked users not disconnected** — Kicked/banned users don't immediately lose WebSocket access
7. **Permissions update laggy** — Admin promotions very slow to reflect in UI
8. **Status not in Members panel** — User status changes not visible in Members sidebar
9. **Threading not persistent** — Thread view disappears on page refresh
10. **Thread replies not realtime** — Must close and reopen thread panel to see new replies; slow to load
11. **Invite accept/decline broken** — Users cannot properly accept or decline room invitations

## Scoring Philosophy

Scores reflect **user-facing functionality**, not implementation effort:
- Features with critical bugs that break the user flow receive minimal/zero credit
- "Code exists" ≠ "feature works"
- A broken flow is worse than no flow (confuses users)

---

## Technical Notes

- **Backend:** Express.js with Socket.io for real-time, Drizzle ORM for PostgreSQL
- **Frontend:** React with Vite, Socket.io-client
- **Auth:** JWT tokens stored in localStorage
- **Database:** PostgreSQL 16 (Docker)
- **Rate limiting:** 500ms between message sends
