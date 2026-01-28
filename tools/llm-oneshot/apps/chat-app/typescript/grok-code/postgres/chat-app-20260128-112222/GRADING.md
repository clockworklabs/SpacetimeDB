# Chat App Grading Checklist

**Instructions:** For each criterion, mark with a number:

- `[x]` = Working (0 reprompts needed)
- `[1]` = Working after 1 reprompt
- `[2]` = Working after 2 reprompts
- `[3+]` = Working after 3+ reprompts
- `[ ]` = Not working / not implemented

---

## Overall Metrics

| Metric                  | Value                      |
| ----------------------- | -------------------------- |
| **Prompt Level Used**   | 05_edit_history            |
| **Features Evaluated**  | 1-8 (max 15)               |
| **Total Feature Score** | **14 / 24** (max 45)       |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success (worked without manual fixes)

| Metric                   | Value                                                                  |
| ------------------------ | ---------------------------------------------------------------------- |
| Lines of code (backend)  | **731**                                                                |
| Lines of code (frontend) | **1252**                                                               |
| Number of files created  | **27**                                                                 |
| External dependencies    | express, socket.io, drizzle-orm, postgres, react, vite, date-fns, etc. |

---

## Feature 1: Basic Chat Features (Score: **2** / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [x] Users can join/leave rooms (0.5)
- [ ] Users can send messages to joined rooms (0.5) — messages appear in all rooms bug
- [x] Online users are displayed (0.5)
- [ ] Basic validation exists (0.5) — member count always shows 1

---

## Feature 2: Typing Indicators (Score: **3** / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: **1** / 3)

- [x] System tracks which users have seen which messages (1)
- [ ] "Seen by X, Y, Z" indicator displays under messages (1)
- [ ] Read status updates in real-time (1) — cannot verify without UI

---

## Feature 4: Unread Message Counts (Score: **0** / 3)

- [ ] Unread count badge shows on room list (1)
- [ ] Count tracks last-read position per user per room (1)
- [ ] Counts update in real-time (1)

---

## Feature 5: Scheduled Messages (Score: **0** / 3)

- [ ] Users can compose and schedule messages for future delivery (1)
- [ ] Pending scheduled messages visible to author with cancel option (1)
- [ ] Message appears in room at scheduled time (1)

---

## Feature 6: Ephemeral/Disappearing Messages (Score: **3** / 3)

- [x] Users can send messages with auto-delete timer (1)
- [x] Countdown or disappearing indicator shown in UI (1)
- [x] Message is permanently deleted when timer expires (1)

---

## Feature 7: Message Reactions (Score: **3** / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [x] Hover/click shows who reacted (0.75)

---

## Feature 8: Message Editing with History (Score: **2** / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [ ] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.5)

---

## Feature 9: Real-Time Permissions (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 10: Rich User Presence (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 11: Message Threading (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 12: Private Rooms & Direct Messages (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 13: Room Activity Indicators (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 14: Draft Sync (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Feature 15: Anonymous to Registered Migration (Score: N/A / 3)

Not included in prompt level 05_edit_history.

---

## Summary Score Sheet

| Feature                  | Max    | Score      | Reprompts  |
| ------------------------ | ------ | ---------- | ---------- |
| 1. Basic Chat            | 3      | 2          | 0          |
| 2. Typing Indicators     | 3      | 3          | 0          |
| 3. Read Receipts         | 3      | 1          | 0          |
| 4. Unread Counts         | 3      | 0          | 0          |
| 5. Scheduled Messages    | 3      | 0          | 0          |
| 6. Ephemeral Messages    | 3      | 3          | 0          |
| 7. Message Reactions     | 3      | 3          | 0          |
| 8. Message Editing       | 3      | 2          | 0          |
| 9. Real-Time Permissions | 3      | N/A        | N/A        |
| 10. Rich Presence        | 3      | N/A        | N/A        |
| 11. Message Threading    | 3      | N/A        | N/A        |
| 12. Private Rooms & DMs  | 3      | N/A        | N/A        |
| 13. Activity Indicators  | 3      | N/A        | N/A        |
| 14. Draft Sync           | 3      | N/A        | N/A        |
| 15. Anonymous Migration  | 3      | N/A        | N/A        |
| **TOTAL**                | **24** | **14**     | **0**      |

---

## Known Bugs

1. **Messages appear in all rooms** — New messages show up in every room instead of just the target room
2. **Member count always 1** — Room member count doesn't update when users join
3. **Unread badges not showing** — Badge counts not displaying on room list
4. **Scheduled messages broken** — Cannot schedule messages for future delivery
5. **No "Seen by" UI** — Read receipts tracked but not displayed
6. **No edit history view** — Edit history stored but not accessible in UI
