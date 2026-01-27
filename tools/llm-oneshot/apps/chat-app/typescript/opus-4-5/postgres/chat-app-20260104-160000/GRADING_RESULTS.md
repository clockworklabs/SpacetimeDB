# Chat App Grading Results

**Date:** 2026-01-04
**Platform:** PostgreSQL (Express + Drizzle ORM + Socket.io + React)
**AI Model:** Claude Opus 4.5

---

## Overall Metrics

| Metric                  | Value                            |
| ----------------------- | -------------------------------- |
| **Prompt Level Used**   | 9 (09_postgres_private_rooms.md) |
| **Features Evaluated**  | 1-12 (max 15)                    |
| **Total Feature Score** | 27.25 / 36                       |
| **Percentage**          | 75.7%                            |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value                                                                                  |
| ------------------------ | -------------------------------------------------------------------------------------- |
| Lines of code (backend)  | 1,004                                                                                  |
| Lines of code (frontend) | 2,285                                                                                  |
| Number of files created  | 21                                                                                     |
| External dependencies    | drizzle-orm, postgres, express, socket.io, jsonwebtoken, cors, react, socket.io-client |

---

## Feature 1: Basic Chat Features (Score: 2.0 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0) — _bug: room appears twice, not persistent — broken UX_
- [x] Users can join/leave rooms (0) — _bug: only non-admin can leave, room hidden after — broken UX_
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.5)
- [x] Basic validation exists (0.5)

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (1)
- [x] Read status updates in real-time (1)

---

## Feature 4: Unread Message Counts (Score: 1.5 / 3)

- [x] Unread count badge shows on room list (0.5) — _very inconsistent_
- [x] Count tracks last-read position per user per room (0.5) — _very inconsistent_
- [x] Counts update in real-time (0.5) — _very inconsistent_

---

## Feature 5: Scheduled Messages (Score: 2.0 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [x] Pending scheduled messages visible to author with cancel option (1)
- [x] Message appears in room at scheduled time (0) — _errors on arrival; needs refresh — real-time broken_

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

## Feature 8: Message Editing with History (Score: 2.75 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.25) — _history window doesn't update in realtime_

---

## Feature 9: Real-Time Permissions (Score: 1.5 / 3)

- [x] Room creator is admin and can kick/ban users (0.5) — _kick exists but doesn't disconnect_
- [ ] Kicked users immediately lose access and stop receiving updates (0) — _completely broken_
- [x] Admins can promote other users to admin (0.5)
- [x] Permission changes apply instantly (0.5)

---

## Feature 10: Rich User Presence (Score: 2.5 / 3)

- [x] Users can set status: online, away, do-not-disturb, invisible (1)
- [x] "Last active X minutes ago" shows for offline users (0.5)
- [x] Status changes sync to all viewers in real-time (0.5) — _not shown in Members view_
- [x] Auto-set to "away" after inactivity period (0.5)

---

## Feature 11: Message Threading (Score: 1.5 / 3)

- [x] Users can reply to specific messages, creating a thread (0.5) — _no visual indication in main view_
- [x] Parent messages show reply count and preview (0.5)
- [x] Threaded view shows all replies to a message (0.5) — _not realtime, need to reopen_
- [ ] New replies sync in real-time to thread viewers (0)

---

## Feature 12: Private Rooms & Direct Messages (Score: 1.5 / 3)

- [x] Users can create private/invite-only rooms (0.75)
- [ ] Room creators can invite specific users by username (0) — _can accept but can't access room — invites useless_
- [x] Direct messages (DMs) between two users work (0.75)
- [ ] Only members can see private room content and member lists (0) — _invite bug breaks membership_

---

## Features 13-15: Not Evaluated

_Not included in prompt level 9_

- Feature 13: Room Activity Indicators — N/A
- Feature 14: Draft Sync — N/A
- Feature 15: Anonymous to Registered Migration — N/A

---

## Summary Score Sheet

| Feature                  | Max    | Score     | Reprompts |
| ------------------------ | ------ | --------- | --------- |
| 1. Basic Chat            | 3      | 2.0       | 0         |
| 2. Typing Indicators     | 3      | 3         | 0         |
| 3. Read Receipts         | 3      | 3         | 0         |
| 4. Unread Counts         | 3      | 1.5       | 0         |
| 5. Scheduled Messages    | 3      | 2.0       | 0         |
| 6. Ephemeral Messages    | 3      | 3         | 0         |
| 7. Message Reactions     | 3      | 3         | 0         |
| 8. Message Editing       | 3      | 2.75      | 0         |
| 9. Real-Time Permissions | 3      | 1.5       | 0         |
| 10. Rich Presence        | 3      | 2.5       | 0         |
| 11. Message Threading    | 3      | 1.5       | 0         |
| 12. Private Rooms & DMs  | 3      | 1.5       | 0         |
| **TOTAL**                | **36** | **27.25** | **0**     |

---

## Known Issues (Critical)

1. **Room duplication** — Created room appears twice in the room list
2. **Room persistence** — Rooms may not persist correctly
3. **Leave room bug** — Only non-admin users can leave; room disappears from list after leave
4. **Unread counts inconsistent** — Badge counts are unreliable
5. **Scheduled message errors** — Message arrival triggers errors; requires page refresh
6. **Edit history not realtime** — History modal doesn't update when message is edited while open
7. **Kicked users still connected** — Kicked users don't immediately lose WebSocket access
8. **Status not in Members panel** — User status changes don't reflect in the Members sidebar
9. **Threading lacks indication** — Replies appear as normal messages; no visual thread indicator
10. **Thread replies not realtime** — Must close and reopen thread panel to see new replies
11. **Private room invite bug** — Users can accept invitations but cannot access the room

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
