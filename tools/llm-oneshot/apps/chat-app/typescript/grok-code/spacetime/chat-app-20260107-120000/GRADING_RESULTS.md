# Chat App Grading Results

**Model:** Grok
**Date:** 2026-01-07
**Prompt:** `05_spacetime_edit_history.md`
**Graded By:** Claude Opus 4.5

---

## Overall Metrics

| Metric                  | Value                            |
| ----------------------- | -------------------------------- |
| **Prompt Level Used**   | 5 (Message Editing with History) |
| **Features Evaluated**  | 1-8 (max 8 for this level)       |
| **Total Feature Score** | 18.25 / 24 (76.0%)               |

- [x] Compiles without errors
- [x] Runs without crashing
- [x] First-try success

| Metric                   | Value                                                               |
| ------------------------ | ------------------------------------------------------------------- |
| Lines of code (backend)  | ~516 (`schema.ts` ~154 + `index.ts` ~362)                           |
| Lines of code (frontend) | ~875 (components ~660 + `main.tsx` ~40 + CSS ~175)                  |
| Number of files created  | 12 (excluding generated bindings)                                   |
| External dependencies    | spacetimedb (backend), react, react-dom, spacetimedb, vite (client) |

---

## Feature 1: Basic Chat Features (Score: 2.0 / 3)

- [ ] Users can set a display name (0) ❌
- [x] Users can create chat rooms (0.5)
- [ ] Users can join/leave rooms (0.25) ⚠️
- [x] Users can send messages to joined rooms (0.5)
- [x] Online users are displayed (0.5)
- [ ] Basic validation exists (0.25) ⚠️

**Implementation Notes:**

- `set_display_name` reducer with 50-char limit
- `create_room` reducer with name/description validation
- `send_message` reducer with 2000-char limit and rate limiting (5 msgs/min)
- Online panel shows users with status indicators
- Validation exists in backend but errors go to `console.error` only

**Bugs Found:**

1. **Display name completely broken** — `clientConnected` auto-creates users with default name `User_abc12345`. The `UserSetup` component checks `if (!currentUser)`, but `currentUser` ALWAYS exists after connection because of auto-creation. No way to change name after initial assignment.

```typescript
// clientConnected auto-creates users:
ctx.db.user.insert({
  identity: ctx.sender,
  displayName: `User_${ctx.sender.toHexString().slice(0, 8)}`, // ← Auto-generated!
  // ...
});
```

2. **No leave room UI** — `leave_room` reducer exists and works correctly, but there is NO button anywhere in the UI to trigger it. Users cannot leave rooms they've joined.

3. **No user-facing error feedback** — All validation errors are caught and logged to `console.error`. Users see no indication when:
   - Rate limit is exceeded
   - Message is too long
   - Edit window (5 min) has expired

---

## Feature 2: Typing Indicators (Score: 3 / 3)

- [x] Typing state is broadcast to other room members (1)
- [x] Typing indicator auto-expires after inactivity (1)
- [x] UI shows "User is typing..." or "Multiple users are typing..." (1)

**Implementation Notes:**

- `TypingIndicator` table with room/user tracking and `public: true`
- `start_typing`, `stop_typing` reducers
- Client-side 3-second timeout to call `stop_typing`
- Server-side cleanup on `clientDisconnected` (clears all user's typing indicators)
- UI shows correct singular/plural text format

---

## Feature 3: Read Receipts (Score: 3 / 3)

- [x] System tracks which users have seen which messages (1)
- [x] "Seen by X, Y, Z" indicator displays under messages (1)
- [x] Read status updates in real-time (1)

**Implementation Notes:**

- `ReadReceipt` table with message/user/timestamp tracking
- `mark_message_read` reducer updates both ReadReceipt and RoomMember.lastReadMessageId
- Auto-marks latest message as read when viewing room
- "Seen by X, Y, Z" displays in MessageItem (excludes message author)
- Real-time sync via SpacetimeDB subscriptions

---

## Feature 4: Unread Message Counts (Score: 2 / 3)

- [x] Unread count badge shows on room list (1)
- [ ] Count tracks last-read position per user per room (0.5) ⚠️
- [x] Counts update in real-time (0.5)

**Implementation Notes:**

- `RoomMember.lastReadMessageId` field tracks position
- `getUnreadCount` callback in Sidebar compares message IDs
- Badge displayed with `unread-badge` class
- Counts update reactively via SpacetimeDB subscriptions

**Bug Found:**

- **Own messages show as unread** — `send_message` reducer creates a `ReadReceipt` but does NOT update `RoomMember.lastReadMessageId`. The unread count logic uses `lastReadMessageId`, so when you send a message, it immediately shows as 1 unread in your own room. This is a critical UX bug that makes unread counts unreliable.

---

## Feature 5: Scheduled Messages (Score: 1.5 / 3)

- [x] Users can compose and schedule messages for future delivery (1)
- [ ] Pending scheduled messages visible to author with cancel option (0) ❌
- [x] Message appears in room at scheduled time (0.5) ⚠️

**Implementation Notes:**

- `ScheduledMessage` scheduled table with `send_scheduled_message` reducer
- `schedule_message` reducer with delay validation (1-1440 minutes)
- `cancel_scheduled_message` reducer exists
- UI has "Scheduled (N)" button in header and panel to show pending messages

**Bugs Found:**

1. **`ScheduledMessage` table missing `public: true`** — The table is not marked as public, so clients cannot subscribe to see pending scheduled messages. The UI code exists but receives no data.

**Score Rationale:**

- Backend scheduling logic is correct
- UI code to display scheduled messages exists
- Table not public = clients can't see pending messages
- Cancel functionality broken as a result

---

## Feature 6: Ephemeral/Disappearing Messages (Score: 1.5 / 3)

- [x] Users can send messages with auto-delete timer (1)
- [ ] Countdown or disappearing indicator shown in UI (0) ❌
- [ ] Message is permanently deleted when timer expires (0.5) ⚠️

**Implementation Notes:**

- `EphemeralMessage` scheduled table with `delete_ephemeral_message` reducer
- `send_ephemeral_message` reducer creates both Message and scheduled deletion
- Duration validation: 1-60 minutes
- UI checkbox to mark message as ephemeral with duration input

**Bugs Found:**

1. **No visual indicator on ephemeral messages** — The `MessageItem` component makes NO reference to the `EphemeralMessage` table. No countdown, no special styling. CSS class `.ephemeral-indicator` exists but is never applied.

2. **EphemeralMessage table missing `public: true`** — Even if UI wanted to show indicators, clients can't subscribe to see which messages are ephemeral.

3. **Deletion untestable** — Since table isn't public, can't verify scheduled deletion works correctly from client perspective.

---

## Feature 7: Message Reactions (Score: 2.75 / 3)

- [ ] Users can add emoji reactions to messages (0.5) ⚠️
- [x] Reaction counts display and update in real-time (0.75)
- [x] Users can toggle their own reactions on/off (0.75)
- [x] Hover/click shows who reacted (0.75)

**Implementation Notes:**

- `MessageReaction` table with user/message/emoji tracking
- `toggle_reaction` reducer adds or removes reaction
- 5 emoji options: 👍 ❤️ 😂 😮 😢
- User's own reactions highlighted with `mine` class
- Grouped display shows emoji + count
- Title tooltip shows who reacted

**Bug Found:**

- **Quick reaction buttons only appear on OWN messages** — The message-actions div (containing Edit button and quick reaction buttons) only renders when `isMyMessage && !isEditing`. Users can click EXISTING reactions on any message to toggle them, but cannot ADD a new reaction to someone else's message if that emoji isn't already there.

```tsx
{
  isMyMessage && !isEditing && (
    <div className="message-actions">
      {/* Edit and reaction buttons only for own messages */}
    </div>
  );
}
```

---

## Feature 8: Message Editing with History (Score: 2.5 / 3)

- [ ] Users can edit their own messages (0.5) ⚠️
- [x] "(edited)" indicator shows on edited messages (0.5)
- [x] Edit history is viewable by other users (1)
- [x] Edits sync in real-time to all viewers (0.5)

**Implementation Notes:**

- `edit_message` reducer with ownership check and 5-minute window
- `MessageEdit` table stores previous content, new content, timestamp, and editor
- "(edited)" badge displayed with expandable history button
- History panel shows previous versions with timestamps
- All changes sync via SpacetimeDB subscriptions

**Bugs Found:**

1. **5-minute edit window is undocumented** — The backend enforces a 5-minute limit on edits, but the UI provides NO indication of this. Users clicking "Edit" on an older message will see the form, enter their changes, click "Save", and... nothing happens (error goes to console only).

2. **Edit button only visible on hover** — The Edit button is inside `.message-actions` which has `opacity: 0` by default and only appears on `.message:hover`. This is a UX pattern but makes the feature less discoverable.

3. **Silent failure on edit errors** — All edit failures (expired window, not author) are caught and logged to `console.error`. Users get no feedback.

---

## Features Not Evaluated (Not in Level 5 Prompt)

| Feature                  | Max | Score | Notes         |
| ------------------------ | --- | ----- | ------------- |
| 9. Real-Time Permissions | 3   | N/A   | Not requested |
| 10. Rich Presence        | 3   | N/A   | Not requested |
| 11. Message Threading    | 3   | N/A   | Not requested |
| 12. Private Rooms & DMs  | 3   | N/A   | Not requested |
| 13. Activity Indicators  | 3   | N/A   | Not requested |
| 14. Draft Sync           | 3   | N/A   | Not requested |
| 15. Anonymous Migration  | 3   | N/A   | Not requested |

---

## Summary Score Sheet

| Feature               | Max    | Score     | Notes                                               |
| --------------------- | ------ | --------- | --------------------------------------------------- |
| 1. Basic Chat         | 3      | **2.0**   | Display name broken, no leave UI, no error feedback |
| 2. Typing Indicators  | 3      | 3         | Full marks                                          |
| 3. Read Receipts      | 3      | 3         | Full marks                                          |
| 4. Unread Counts      | 3      | **2**     | Own messages show as unread                         |
| 5. Scheduled Messages | 3      | **1.5**   | Table not public, queue invisible                   |
| 6. Ephemeral Messages | 3      | **1.5**   | Table not public, no indicator                      |
| 7. Message Reactions  | 3      | **2.75**  | Can't add NEW reactions to others' messages         |
| 8. Message Editing    | 3      | **2.5**   | 5-min limit undocumented, silent failures           |
| **TOTAL**             | **24** | **18.25** | **76.0%**                                           |

---

## Technical Notes

### Bugs Found (Summary)

**Critical (Feature-Breaking):**

1. **Display name completely broken** — `clientConnected` auto-creates users with default name. `UserSetup` never shows because `currentUser` always exists. NO way to change name.

2. **No leave room UI** — `leave_room` reducer exists but no button in UI.

3. **Own messages show as unread** — `send_message` creates `ReadReceipt` but not `lastReadMessageId`. Then `markMessageRead` sees ReadReceipt exists and skips update. Result: your own messages always show as unread.

4. **ScheduledMessage table missing `public: true`** — Clients can't subscribe, scheduled messages panel always empty.

5. **EphemeralMessage table missing `public: true`** — Even if UI wanted indicators, can't see the data.

6. **No ephemeral indicator** — Messages that will auto-delete have NO visual distinction.

7. **Reactions only addable on own messages** — Quick reaction buttons don't render for others' messages.

**Moderate:**

8. **5-minute edit window undocumented** — Backend limit with no UI warning, silent failure.

9. **`clientDisconnected` doesn't update `User.isOnline`** — Only updates `UserStatus.isOnline`, leaving `User.isOnline = true` forever after first connection.

10. **Room owner can leave their own room** — No protection, orphans the room with no owner.

11. **`send_ephemeral_message` same unread bug** — Creates ReadReceipt without updating lastReadMessageId.

12. **`send_scheduled_message` same unread bug** — Scheduled reducer has same issue.

**Minor:**

13. **All errors go to console** — No user-facing error messages anywhere.

14. **No duplicate room name check** — Can create multiple rooms with same name.

15. **Redundant `isOnline` field** — Both `User` and `UserStatus` have `isOnline`, can desync.

16. **Identity polling never stops** — `setTimeout(checkIdentity, 100)` runs forever if connection fails.

17. **`onUserCreated` callback is empty** — Does nothing, dead code.

18. **Loading states ignored** — `useTable` returns `[data, isLoading]` but isLoading is never used.

19. **No reconnection handling** — If WebSocket drops, app doesn't recover.

### Architecture Decisions

- 11 tables defined in `schema.ts` (User, Room, RoomMember, Message, MessageEdit, ScheduledMessage, EphemeralMessage, TypingIndicator, ReadReceipt, MessageReaction, UserStatus)
- 15 reducers including: set_display_name, create_room, join_room, leave_room, send_message, edit_message, schedule_message, cancel_scheduled_message, send_ephemeral_message, start_typing, stop_typing, mark_message_read, toggle_reaction
- 2 scheduled reducers: send_scheduled_message, delete_ephemeral_message
- 2 lifecycle hooks: clientConnected, clientDisconnected
- Modular React components: App, Sidebar, ChatArea, MessageItem, MessageInput, UserSetup
- Dark Discord-like theme via CSS custom properties
- Token persistence implemented correctly (localStorage + .withToken())

### Files Structure

```
backend/spacetimedb/
├── package.json
├── tsconfig.json
├── dist/bundle.js
└── src/
    ├── schema.ts      (~154 lines)
    └── index.ts       (~362 lines)

client/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── index.html
└── src/
    ├── config.ts
    ├── main.tsx       (~40 lines)
    ├── App.tsx        (~54 lines)
    ├── index.css      (~175 lines)
    ├── components/
    │   ├── Sidebar.tsx      (~163 lines)
    │   ├── ChatArea.tsx     (~168 lines)
    │   ├── MessageItem.tsx  (~181 lines)
    │   ├── MessageInput.tsx (~195 lines)
    │   └── UserSetup.tsx    (~75 lines)
    └── module_bindings/     (generated)
```

### Compilation Issues (Fixed During Deployment)

1. **`useTable` destructuring** — Changed from `const data = useTable(...)` to `const [data] = useTable(...)` (tuple return)
2. **`readonly` array handling** — Added spread operator `[...array]` before `.sort()` and `.filter()` calls
3. **`NodeJS.Timeout` type** — Changed to `number` for browser environment
4. **`import.meta.env` typing** — Cast to `any` for Vite environment variables
5. **Unused imports** — Removed unused `React` imports from components
6. **Void reducer calls** — Wrapped in try-catch instead of `.catch()`

---

## Comparison to Other Implementations (Same Prompt Level)

| Feature               | Grok      | Gemini   | Opus 4.5 Best |
| --------------------- | --------- | -------- | ------------- |
| 1. Basic Chat         | **2.0**   | 2.5      | 3             |
| 2. Typing Indicators  | 3         | 2.5      | 3             |
| 3. Read Receipts      | 3         | 3        | 3             |
| 4. Unread Counts      | **2**     | 3        | 3             |
| 5. Scheduled Messages | **1.5**   | 2        | 3             |
| 6. Ephemeral Messages | **1.5**   | 2        | 2.5           |
| 7. Message Reactions  | **2.75**  | 2.5      | 2.5           |
| 8. Message Editing    | **2.5**   | 3        | 3             |
| **TOTAL**             | **18.25** | **20.5** | **23**        |

**Key Differences from Gemini:**

- Grok has worse basic chat (display name completely broken vs partially working)
- Grok has better typing indicators (auto-expiry works on both client and server)
- Grok has broken unread counts (own messages show as unread)
- Grok missing `public: true` on BOTH scheduled tables (ScheduledMessage AND EphemeralMessage)
- Grok has proper token persistence (Gemini didn't)
- Both have similar reaction limitations
- Both missed ephemeral indicator

**Key Differences from Opus 4.5:**

- Opus achieved full marks on Basic Chat, Scheduled Messages, and Message Editing
- Opus had better error handling feedback
- Opus had fewer critical UX bugs
- Opus correctly marked scheduled tables as public
