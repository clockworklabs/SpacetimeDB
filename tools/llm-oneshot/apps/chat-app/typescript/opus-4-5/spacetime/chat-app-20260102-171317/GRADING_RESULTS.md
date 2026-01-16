# Chat App Grading Checklist

**Instructions:** For each criterion, mark with a number:
- `[x]` = Working (0 reprompts needed)
- `[1]` = Working after 1 reprompt
- `[2]` = Working after 2 reprompts
- `[3+]` = Working after 3+ reprompts
- `[ ]` = Not working / not implemented

---

## Overall Metrics

| Metric | Value |
|--------|-------|
| **Prompt Level Used** | 09 (Private Rooms) |
| **Features Evaluated** | 1-12 (max 15) |
| **Total Feature Score** | 34.5 / 36 |
| **Percentage** | 95.8% |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success (worked without manual fixes)

| Metric | Value |
|--------|-------|
| Lines of code (backend) | ~1,500 |
| Lines of code (frontend) | ~1,700 |
| Number of files created | 12 (+ ~90 generated bindings) |
| External dependencies | spacetimedb, react, vite |

---

## Feature 1: Basic Chat Features (Score: 3 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [x] Users can join/leave rooms (0.5)
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

## Feature 4: Unread Message Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [x] Counts update in real-time (1)

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

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.5)

---

## Feature 9: Real-Time Permissions (Score: 1.5 / 3)

- [x] Room creator is admin and can kick/ban users (1) → 0.5 (private rooms only)
- [x] Kicked users immediately lose access and stop receiving updates (1) → 0.5 (private rooms only, backend works correctly)
- [x] Admins can promote other users to admin (0.5) → 0.25 (private rooms only)
- [x] Permission changes apply instantly (0.5) → 0.25 (private rooms only)

**Note:** UI only shows admin tools for private rooms (line 821: `selectedRoom.isPrivate` condition). Backend works for all rooms.

---

## Feature 10: Rich User Presence (Score: 3 / 3)

- [x] Users can set status: online, away, do-not-disturb, invisible (1)
- [x] "Last active X minutes ago" shows for offline users (0.5)
- [x] Status changes sync to all viewers in real-time (1)
- [x] Auto-set to "away" after inactivity period (0.5)

---

## Feature 11: Message Threading (Score: 3 / 3)

- [x] Users can reply to specific messages, creating a thread (1)
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

## Feature 13: Room Activity Indicators (Score: 0 / 3)

- [ ] Activity badges show on rooms (e.g., "Active", "Hot") (1)
- [ ] Activity level reflects recent message velocity (1)
- [ ] Indicators update in real-time as activity changes (1)

**Note:** Not in prompt level 09

---

## Feature 14: Draft Sync (Score: 0 / 3)

- [ ] Message drafts save automatically as user types (1)
- [ ] Drafts sync across devices/sessions in real-time (1)
- [ ] Each room maintains its own draft per user (0.5)
- [ ] Drafts persist until sent or manually cleared (0.5)

**Note:** Not in prompt level 09

---

## Feature 15: Anonymous to Registered Migration (Score: 0 / 3)

- [ ] Users can join and send messages without an account (1)
- [ ] Anonymous identity persists for the session (0.5)
- [ ] Registration preserves message history and identity (1)
- [ ] Room memberships transfer to registered account (0.5)

**Note:** Not in prompt level 09

---

## Summary Score Sheet

| Feature | Max | Score | Reprompts |
|---------|-----|-------|-----------|
| 1. Basic Chat | 3 | 3 | 0 |
| 2. Typing Indicators | 3 | 3 | 0 |
| 3. Read Receipts | 3 | 3 | 0 |
| 4. Unread Counts | 3 | 3 | 0 |
| 5. Scheduled Messages | 3 | 3 | 0 |
| 6. Ephemeral Messages | 3 | 3 | 0 |
| 7. Message Reactions | 3 | 3 | 0 |
| 8. Message Editing | 3 | 3 | 0 |
| 9. Real-Time Permissions | 3 | 1.5 | 0 |
| 10. Rich Presence | 3 | 3 | 0 |
| 11. Message Threading | 3 | 3 | 0 |
| 12. Private Rooms & DMs | 3 | 3 | 0 |
| 13. Activity Indicators | 3 | 0 | N/A |
| 14. Draft Sync | 3 | 0 | N/A |
| 15. Anonymous Migration | 3 | 0 | N/A |
| **TOTAL (1-12)** | **36** | **34.5** | **0** |
| **TOTAL (1-15)** | **45** | **34.5** | **0** |

---

## Notes

- **Feature 9 Issue:** Admin tools (kick/ban/promote) only appear in UI for private rooms due to unnecessary condition `selectedRoom.isPrivate` in App.tsx line 821. Backend supports all rooms and correctly deletes memberships.
- **Model:** Claude Opus 4.5
- **Date:** 2026-01-02

---

## Scoring Philosophy

Scores reflect **user-facing functionality**, not implementation effort:
- Features with critical bugs that break the user flow receive minimal/zero credit
- "Code exists" ≠ "feature works"
- Actual testing trumps code analysis — if it doesn't work in practice, it doesn't get credit
- Feature 9 penalized for UI-only limitation (admin tools hidden for public rooms despite working backend)