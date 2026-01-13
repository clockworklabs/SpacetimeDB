# Chat App Grading Results

**Model:** Gemini
**Date:** 2026-01-07
**Prompt:** `05_spacetime_edit_history.md`
**Graded By:** Claude Opus 4.5

---

## Overall Metrics

| Metric | Value |
|--------|-------|
| **Prompt Level Used** | 5 (Message Editing with History) |
| **Features Evaluated** | 1-8 (max 8 for this level) |
| **Total Feature Score** | 20.5 / 24 (85.4%) |

- [x] Compiles without errors
- [x] Runs without crashing
- [ ] First-try success (unknown — graded post-deployment)

| Metric | Value |
|--------|-------|
| Lines of code (backend) | ~550 (`schema.ts` ~200 + `index.ts` ~350) |
| Lines of code (frontend) | ~600 (`App.tsx` ~430 + `main.tsx` ~30 + `styles.css` ~200) |
| Number of files created | ~10 (excluding generated bindings) |
| External dependencies | spacetimedb (backend), react, react-dom, spacetimedb, vite (client) |

---

## Feature 1: Basic Chat Features (Score: 2.5 / 3)

- [x] Users can set a display name (0.5)
- [x] Users can create chat rooms (0.5)
- [ ] Users can join/leave rooms (0) ❌
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.5)
- [x] Basic validation exists (0.5)

**Implementation Notes:**
- `set_name` reducer with 50-char limit
- `create_room` reducer with optional description
- `send_message` reducer with 2000-char limit
- Online panel shows users with status indicators (green dot for online)
- Validation for empty names, duplicate memberships, room existence

**Bug Found:**
- **No join/leave UI** — `handleJoinRoom` and `handleLeaveRoom` callbacks are defined but NEVER CALLED in the UI. The room list shows all rooms but clicking only selects them — no join button exists. Users can only be members of rooms they create. This completely breaks multi-user chat functionality.

---

## Feature 2: Typing Indicators (Score: 2.5 / 3)

- [x] Typing state is broadcast to other room members (1)
- [ ] Typing indicator auto-expires after inactivity (0.5) ⚠️
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

**Implementation Notes:**
- `TypingIndicator` table with room/user tracking
- `start_typing`, `stop_typing` reducers
- Indicator cleared on disconnect and message send
- UI shows singular message for one user, plural for multiple
- Cleared on `onBlur` of input field

**Partial Implementation:**
- **No time-based auto-expiry** — Typing indicators only clear on explicit actions (blur, send, disconnect). There is no scheduled reducer to auto-expire indicators after X seconds of inactivity. If a user types, stops typing, but keeps focus on the input, the indicator persists indefinitely. Discord-style indicators auto-expire after ~5 seconds.

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (1)
- [x] Read status updates in real-time (1)

**Implementation Notes:**
- `ReadReceipt` table with message/user/timestamp tracking
- `mark_message_read` reducer for individual messages
- `mark_room_read` reducer to mark all messages in room as read
- UI displays "Seen by X, Y, Z" using `getUserName` helper
- Real-time sync via SpacetimeDB subscriptions

---

## Feature 4: Unread Message Counts (Score: 3 / 3)

- [x] Unread count badge shows on room list (1)
- [x] Count tracks last-read position per user per room (1)
- [x] Counts update in real-time (1)

**Implementation Notes:**
- `RoomReadPosition` table with `lastReadMessageId` field
- `getUnreadCount` callback compares message IDs to last read position
- Badge displayed with `unread-badge` class showing count
- "Mark Read" button in chat header updates position

---

## Feature 5: Scheduled Messages (Score: 2 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [ ] Pending scheduled messages visible to author with cancel option (0) ❌
- [x] Message appears in room at scheduled time (1)

**Implementation Notes:**
- `ScheduledMessage` scheduled table with `send_scheduled_message` reducer
- `schedule_message` reducer with delay validation (10s - 24h)
- `cancel_scheduled_message` reducer exists
- UI code for displaying pending messages exists

**Bug Found:**
- **`ScheduledMessage` table is missing `public: true`** — clients cannot subscribe to this table, so pending scheduled messages are never visible to users. The cancel functionality is broken as a result.

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 2 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [ ] Countdown or disappearing indicator shown in UI (0) ❌
- [x] Message is permanently deleted when timer expires (1)

**Implementation Notes:**
- `EphemeralMessage` scheduled table with `cleanup_ephemeral_message` reducer
- `send_ephemeral_message` reducer with duration validation (10s - 1h)
- Cleanup reducer deletes message and all related data (reactions, receipts, edits)
- UI checkbox to mark message as ephemeral with duration input

**Bug Found:**
- **No visible indicator on ephemeral messages** — The code attempts to show a countdown using `ephemeralExpiresAt`, but the timestamp is incorrectly constructed in the backend (`{ microsSinceUnixEpoch: expiresAt }` instead of a proper Timestamp object). The field appears undefined/malformed on the client, so no indicator is displayed.

---

## Feature 7: Message Reactions (Score: 2.5 / 3)

- [x] Users can add emoji reactions to messages (0.75)
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [ ] Hover/click shows who reacted (0) ❌

**Implementation Notes:**
- `Reaction` table with user/message/emoji tracking
- `toggle_reaction` reducer adds or removes reaction
- 5 emoji options: 👍 ❤️ 😂 😮 😢
- User's own reactions highlighted with `active` class
- Grouped display shows emoji + count

**Missing Feature:**
- **No tooltip showing who reacted** — The code tracks `data.users` in the grouped reactions but never displays this information. Users only see the count, not the names of who reacted.

---

## Feature 8: Message Editing with History (Score: 3 / 3)

- [x] Users can edit their own messages (1)
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.5)

**Implementation Notes:**
- `edit_message` reducer with ownership check
- `MessageEdit` table stores previous content, new content, timestamp, and editor
- "(edited)" badge displayed via `message-edited` class
- "Show History" / "Hide History" button toggles edit history panel
- History shows diff with strikethrough for old content, highlight for new
- All changes sync via SpacetimeDB subscriptions

---

## Features Not Evaluated (Not in Level 5 Prompt)

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 9. Real-Time Permissions | 3 | N/A | Not requested (though some role logic exists) |
| 10. Rich Presence | 3 | N/A | Not requested |
| 11. Message Threading | 3 | N/A | Not requested |
| 12. Private Rooms & DMs | 3 | N/A | Not requested |
| 13. Activity Indicators | 3 | N/A | Not requested |
| 14. Draft Sync | 3 | N/A | Not requested |
| 15. Anonymous Migration | 3 | N/A | Not requested |

---

## Summary Score Sheet

| Feature | Max | Score | Notes |
|---------|-----|-------|-------|
| 1. Basic Chat | 3 | 2.5 | No join/leave room UI |
| 2. Typing Indicators | 3 | 2.5 | No time-based auto-expiry |
| 3. Read Receipts | 3 | 3 | Full marks |
| 4. Unread Counts | 3 | 3 | Full marks |
| 5. Scheduled Messages | 3 | 2 | Missing `public: true` on table |
| 6. Ephemeral Messages | 3 | 2 | No visible indicator (timestamp bug) |
| 7. Message Reactions | 3 | 2.5 | No hover tooltip for reactors |
| 8. Message Editing | 3 | 3 | Full marks |
| **TOTAL** | **24** | **20.5** | **85.4%** |

---

## Technical Notes

### Bugs Found

1. **No join/leave room UI** — `handleJoinRoom` callback is defined but never called. Users cannot join rooms they didn't create, breaking multi-user functionality.

2. **ScheduledMessage table not public** — The `ScheduledMessage` table definition is missing `public: true`, so clients cannot subscribe to see pending scheduled messages. The UI code exists but receives no data.

3. **Ephemeral timestamp incorrectly constructed** — The backend creates `ephemeralExpiresAt: { microsSinceUnixEpoch: expiresAt }` as a plain object instead of a proper Timestamp. This doesn't serialize correctly, so the client never receives the expiration time.

4. **No reaction hover tooltip** — The reaction grouping code tracks which users reacted, but the UI only displays the count, not the user names.

5. **No typing auto-expiry** — No scheduled reducer to auto-delete stale typing indicators. Indicators persist until explicit user action.

6. **Token persistence missing** — No `.withToken()` call and no `localStorage.setItem('auth_token', token)` in onConnect. User identity is lost on every page refresh.

7. **Wrong `useTable` pattern** — Uses `const rows = useTable(table)` instead of `const [rows, isLoading] = useTable(table)`. The hook returns a tuple, not just rows.

8. **Multi-column index usage** — Backend uses `.filter()` on multi-column indexes (e.g., `room_identity`, `room_user`) which the rules state is BROKEN and may cause PANIC or silent empty results.

### Architecture Decisions

- 11 tables defined in `schema.ts` (User, Room, RoomMember, Message, MessageEdit, Reaction, ReadReceipt, RoomReadPosition, TypingIndicator, ScheduledMessage, EphemeralMessage)
- Reducers include full set: set_name, create_room, join_room, leave_room, send_message, edit_message, delete_message, toggle_reaction, mark_message_read, mark_room_read, start_typing, stop_typing, schedule_message, cancel_scheduled_message, send_ephemeral_message
- Scheduled reducers: send_scheduled_message, cleanup_ephemeral_message
- Lifecycle hooks: clientConnected, clientDisconnected
- Single-file React component (`App.tsx`) with dark Discord-like theme
- Role-based permissions (owner/admin/member) partially implemented but not required for this prompt level

### Files Structure

```
backend/spacetimedb/
├── package.json
├── tsconfig.json
├── dist/bundle.js
└── src/
    ├── schema.ts      (~200 lines)
    ├── reducers.ts    (~350 lines)
    └── index.ts       (imports)

client/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── index.html
└── src/
    ├── config.ts
    ├── main.tsx       (~30 lines)
    ├── App.tsx        (~430 lines)
    ├── styles.css     (~200 lines)
    └── module_bindings/ (generated)
```

---

## Comparison to Opus 4.5 (Same Prompt Level)

| Feature | Gemini | Opus 4.5 |
|---------|--------|----------|
| 1. Basic Chat | **2.5** | 3 |
| 2. Typing Indicators | **2.5** | 3 |
| 3. Read Receipts | 3 | 3 |
| 4. Unread Counts | 3 | 3 |
| 5. Scheduled Messages | **2** | 3 |
| 6. Ephemeral Messages | **2** | 2.5 |
| 7. Message Reactions | 2.5 | 2.5 |
| 8. Message Editing | 3 | 3 |
| **TOTAL** | **20.5** | **23** |

**Key Differences:**
- Gemini defined `handleJoinRoom` but never wired it to UI — users can't join rooms
- Gemini has no scheduled auto-expiry for typing indicators
- Gemini missed `public: true` on ScheduledMessage table (critical bug)
- Gemini incorrectly constructed ephemeral timestamp (no visible indicator)
- Gemini missing token persistence (identity lost on refresh)
- Gemini uses wrong `useTable` pattern (tuple vs direct)
- Both implementations missed reaction hover tooltip
- Opus had a partial ephemeral indicator issue but better than Gemini's complete failure
