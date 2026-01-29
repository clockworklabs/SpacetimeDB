# Chat App Grading Checklist

**Instructions:** For each criterion, mark with a number:

- `[x]` = Working (0 reprompts needed)
- `[1]` = Working after 1 reprompt
- `[2]` = Working after 2 reprompts
- `[3+]` = Working after 3+ reprompts
- `[ ]` = Not working / not implemented

---

## Overall Metrics

| Metric                  | Value                              |
| ----------------------- | ---------------------------------- |
| **Prompt Level Used**   | ****\*\*\*\*****\_****\*\*\*\***** |
| **Features Evaluated**  | 1-\_\_\_ (max 15)                  |
| **Total Feature Score** | **_ / _** (max 45)                 |

- [ ] Compiles without errors
- [ ] Runs without crashing
- [ ] First-try success (worked without manual fixes)

| Metric                   | Value                              |
| ------------------------ | ---------------------------------- |
| Lines of code (backend)  | **\_\_\_**                         |
| Lines of code (frontend) | **\_\_\_**                         |
| Number of files created  | **\_\_\_**                         |
| External dependencies    | ****\*\*\*\*****\_****\*\*\*\***** |

---

## Feature 1: Basic Chat Features (Score: \_\_\_ / 3)

- [ ] Users can set a display name (0.5)
- [ ] Users can create chat rooms (0.5)
- [ ] Users can join/leave rooms (0.5)
- [ ] Users can send messages to joined rooms (0.5)
- [ ] Online users are displayed (0.5)
- [ ] Basic validation exists (0.5)

---

## Feature 2: Typing Indicators (Score: \_\_\_ / 3)

- [ ] Typing state is broadcast to other room members (1)
- [ ] Typing indicator auto-expires after inactivity (1)
- [ ] UI shows "User is typing..." or "Multiple users are typing..." (1)

---

## Feature 3: Read Receipts (Score: \_\_\_ / 3)

- [ ] System tracks which users have seen which messages (1)
- [ ] "Seen by X, Y, Z" indicator displays under messages (1)
- [ ] Read status updates in real-time (1)

---

## Feature 4: Unread Message Counts (Score: \_\_\_ / 3)

- [ ] Unread count badge shows on room list (1)
- [ ] Count tracks last-read position per user per room (1)
- [ ] Counts update in real-time (1)

---

## Feature 5: Scheduled Messages (Score: \_\_\_ / 3)

- [ ] Users can compose and schedule messages for future delivery (1)
- [ ] Pending scheduled messages visible to author with cancel option (1)
- [ ] Message appears in room at scheduled time (1)

---

## Feature 6: Ephemeral/Disappearing Messages (Score: \_\_\_ / 3)

- [ ] Users can send messages with auto-delete timer (1)
- [ ] Countdown or disappearing indicator shown in UI (1)
- [ ] Message is permanently deleted when timer expires (1)

---

## Feature 7: Message Reactions (Score: \_\_\_ / 3)

- [ ] Users can add emoji reactions to messages (0.75)
- [ ] Reaction counts display and update in real-time (0.75)
- [ ] Users can toggle their own reactions on/off (0.75)
- [ ] Hover/click shows who reacted (0.75)

---

## Feature 8: Message Editing with History (Score: \_\_\_ / 3)

- [ ] Users can edit their own messages (1)
- [ ] "(edited)" indicator shows on edited messages (0.5)
- [ ] Edit history is viewable by other users (1)
- [ ] Edits sync in real-time to all viewers (0.5)

---

## Feature 9: Real-Time Permissions (Score: \_\_\_ / 3)

- [ ] Room creator is admin and can kick/ban users (1)
- [ ] Kicked users immediately lose access and stop receiving updates (1)
- [ ] Admins can promote other users to admin (0.5)
- [ ] Permission changes apply instantly (0.5)

---

## Feature 10: Rich User Presence (Score: \_\_\_ / 3)

- [ ] Users can set status: online, away, do-not-disturb, invisible (1)
- [ ] "Last active X minutes ago" shows for offline users (0.5)
- [ ] Status changes sync to all viewers in real-time (1)
- [ ] Auto-set to "away" after inactivity period (0.5)

---

## Feature 11: Message Threading (Score: \_\_\_ / 3)

- [ ] Users can reply to specific messages, creating a thread (1)
- [ ] Parent messages show reply count and preview (0.5)
- [ ] Threaded view shows all replies to a message (1)
- [ ] New replies sync in real-time to thread viewers (0.5)

---

## Feature 12: Private Rooms & Direct Messages (Score: \_\_\_ / 3)

- [ ] Users can create private/invite-only rooms (0.75)
- [ ] Room creators can invite specific users by username (0.75)
- [ ] Direct messages (DMs) between two users work (0.75)
- [ ] Only members can see private room content and member lists (0.75)

---

## Feature 13: Room Activity Indicators (Score: \_\_\_ / 3)

- [ ] Activity badges show on rooms (e.g., "Active", "Hot") (1)
- [ ] Activity level reflects recent message velocity (1)
- [ ] Indicators update in real-time as activity changes (1)

---

## Feature 14: Draft Sync (Score: \_\_\_ / 3)

- [ ] Message drafts save automatically as user types (1)
- [ ] Drafts sync across devices/sessions in real-time (1)
- [ ] Each room maintains its own draft per user (0.5)
- [ ] Drafts persist until sent or manually cleared (0.5)

---

## Feature 15: Anonymous to Registered Migration (Score: \_\_\_ / 3)

- [ ] Users can join and send messages without an account (1)
- [ ] Anonymous identity persists for the session (0.5)
- [ ] Registration preserves message history and identity (1)
- [ ] Room memberships transfer to registered account (0.5)

---

## Summary Score Sheet

| Feature                  | Max    | Score      | Reprompts  |
| ------------------------ | ------ | ---------- | ---------- |
| 1. Basic Chat            | 3      | \_\_\_     | \_\_\_     |
| 2. Typing Indicators     | 3      | \_\_\_     | \_\_\_     |
| 3. Read Receipts         | 3      | \_\_\_     | \_\_\_     |
| 4. Unread Counts         | 3      | \_\_\_     | \_\_\_     |
| 5. Scheduled Messages    | 3      | \_\_\_     | \_\_\_     |
| 6. Ephemeral Messages    | 3      | \_\_\_     | \_\_\_     |
| 7. Message Reactions     | 3      | \_\_\_     | \_\_\_     |
| 8. Message Editing       | 3      | \_\_\_     | \_\_\_     |
| 9. Real-Time Permissions | 3      | \_\_\_     | \_\_\_     |
| 10. Rich Presence        | 3      | \_\_\_     | \_\_\_     |
| 11. Message Threading    | 3      | \_\_\_     | \_\_\_     |
| 12. Private Rooms & DMs  | 3      | \_\_\_     | \_\_\_     |
| 13. Activity Indicators  | 3      | \_\_\_     | \_\_\_     |
| 14. Draft Sync           | 3      | \_\_\_     | \_\_\_     |
| 15. Anonymous Migration  | 3      | \_\_\_     | \_\_\_     |
| **TOTAL**                | **45** | **\_\_\_** | **\_\_\_** |
