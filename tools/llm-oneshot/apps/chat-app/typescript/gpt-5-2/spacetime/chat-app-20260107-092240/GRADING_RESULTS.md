# Chat App Grading Results

**Date:** 2026-01-07
**Platform:** SpacetimeDB (TypeScript module + React client)
**AI Model:** GPT 5.2

---

## Overall Metrics

| Metric                  | Value                            |
| ----------------------- | -------------------------------- |
| **Prompt Level Used**   | 5 (05_spacetime_edit_history.md) |
| **Features Evaluated**  | 1-8 (max 15)                     |
| **Total Feature Score** | 24 / 24                          |
| **Percentage**          | 100%                             |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value                                                 |
| ------------------------ | ----------------------------------------------------- |
| Lines of code (backend)  | 752                                                   |
| Lines of code (frontend) | 1,189                                                 |
| Number of files created  | 13                                                    |
| External dependencies    | spacetimedb ^1.11.0, react ^18.3.1, react-dom ^18.3.1 |

---

## Feature 1: Basic Chat Features (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [x] Users can join/leave rooms (0.5)
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.5)
- [x] Basic validation exists (0.5) — _rate limiting 0.4s, length limits, empty checks_

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1) — _4s TTL via scheduled job_
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (1)
- [x] Read status updates in real-time (1)

---

## Feature 4: Unread Message Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [x] Counts update in real-time (1)

---

## Feature 5: Scheduled Messages (Score: 3 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [x] Pending scheduled messages visible to author with cancel option (1)
- [x] Message appears in room at scheduled time (1) — _uses SpacetimeDB scheduled tables_

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 3 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1) — _live countdown "disappears in Xs"_
- [x] Message is permanently deleted when timer expires (1) — _scheduled cleanup job_

---

## Feature 7: Message Reactions (Score: 3 / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [x] Hover/click shows who reacted (0.75) — _title tooltip on hover_

---

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1) — _modal shows all edits_
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

| Feature               | Max    | Score  |
| --------------------- | ------ | ------ |
| 1. Basic Chat         | 3      | 3      |
| 2. Typing Indicators  | 3      | 3      |
| 3. Read Receipts      | 3      | 3      |
| 4. Unread Counts      | 3      | 3      |
| 5. Scheduled Messages | 3      | 3      |
| 6. Ephemeral Messages | 3      | 3      |
| 7. Message Reactions  | 3      | 3      |
| 8. Message Editing    | 3      | 3      |
| **TOTAL**             | **24** | **24** |

---

## Known Issues

None critical. Minor observations:

1. **Unread count includes own messages** — When you send a message, it counts toward unread until marked read. Discord excludes own messages.
2. **"Seen by" may include yourself** — Current user not filtered from seen list. Minor UX difference from Discord.
3. **Reaction tooltip is basic** — Uses browser `title` attribute rather than custom popover.

_These are not grading deductions as the checklist doesn't require Discord-exact behavior._

---

## Technical Notes

- **Backend:** SpacetimeDB TypeScript module with schema.ts + index.ts
- **Frontend:** React 18 with Vite, spacetimedb/react hooks
- **Auth:** SpacetimeDB identity (automatic)
- **Rate limiting:** 0.4s between messages, 1s between typing broadcasts
- **Scheduled jobs:** Used for typing expiry (4s), ephemeral cleanup, scheduled message delivery
- **Tables:** 13 tables including 3 scheduled job tables
- **Reducers:** 16 reducers + 2 lifecycle hooks (clientConnected/Disconnected)
