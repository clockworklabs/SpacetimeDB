# Iteration Log

## Run Info
- **Backend:** spacetime
- **Level:** 8 (threading)
- **Started:** 2026-04-01T18:00:00

---

## Level 8 Upgrade — Threading (no iteration needed)

**What was added:**
- `threadReply` table in `schema.ts`: `parentMessageId`, `roomId`, `sender`, `text`, `sentAt` fields; `btree` index on `parentMessageId` and `roomId`
- `sendThreadReply` reducer in `index.ts`: validates sender is registered, parent message exists, sender is a room member and not banned
- Full thread UI in `client/src/App.tsx`:
  - Subscribe to `SELECT * FROM thread_reply`
  - `💬` reply button on each message (shows reply count badge if > 0)
  - Side panel opens when thread button clicked: shows parent message, reply divider with count, all replies, empty state, reply input
  - `handleSendThreadReply` calls `conn.reducers.sendThreadReply({ parentMessageId, text })`
  - Real-time via SpacetimeDB subscription
- Thread panel CSS in `styles.css`: `.thread-panel`, `.thread-panel-header`, `.thread-messages`, `.thread-parent-message`, `.thread-reply-divider`, `.thread-reply-message`, `.thread-empty`, `.thread-input-area`, `.thread-input`
- Bindings regenerated: `thread_reply_table.ts`, `send_thread_reply_reducer.ts`

**Build:** `npx tsc --noEmit` passes with 0 errors.
**Deploy:** Backend republished; bindings regenerated; dev server running on http://localhost:5173.

**Scores (pending browser grading):** Features 1-10: 30/30 (verified in prior levels). Feature 11 (Threading): pending.

---

## Level 9 Upgrade — Private Rooms & DMs (no iteration needed)

**What was added:**
- `isPrivate: bool` and `isDm: bool` fields to `room` table in `schema.ts`
- `roomInvitation` table: `id`, `roomId`, `inviterIdentity`, `inviteeIdentity`, `sentAt`, `status` ('pending'/'accepted'/'declined'); btree indexes on `roomId`, `inviterIdentity`, `inviteeIdentity`
- `createRoom` reducer updated to accept `isPrivate` parameter (was `{ name }`, now `{ name, isPrivate }`)
- `joinRoom` updated to block joining private rooms without an accepted invitation
- New reducers: `inviteToRoom`, `acceptInvitation`, `declineInvitation`, `openDm`
- `openDm`: finds or creates a DM room between sender and target; both users auto-joined as admins; DM rooms are private (`isPrivate: true, isDm: true`)
- Bindings regenerated: `room_invitation_table.ts`, `accept_invitation_reducer.ts`, `decline_invitation_reducer.ts`, `invite_to_room_reducer.ts`, `open_dm_reducer.ts`; `create_room_reducer.ts` updated
- Client App.tsx:
  - Subscribe to `SELECT * FROM room_invitation`
  - `useTable(tables.roomInvitation)` + `myPendingInvitations` derived state
  - Room list filtered: private rooms hidden unless user is a member
  - Room icons: `💬` for DMs, `🔒` for private rooms, `#` for public
  - `private` badge on private non-DM rooms in list
  - Create room form: "Private room" checkbox
  - Invitations panel in sidebar: shows pending invitations with Accept/Decline buttons
  - Invite panel in chat header (admin + private room only): dropdown to select user and send invite
  - DM button (`💬`) next to each user in users list (hover-revealed)
- styles.css: `.private-badge`, `.private-room-toggle`, `.invite-count-badge`, `.invitation-list`, `.invitation-item`, `.invitation-info`, `.invitation-room`, `.invitation-from`, `.invitation-actions`, `.invite-btn`, `.invite-panel`, `.invite-panel-title`, `.invite-panel-row`, `.invite-select`, `.dm-btn`

**Backend:** Re-published with `--delete-data` (schema migration required for new columns). Bindings regenerated.
**Build:** `npx tsc --noEmit` passes; `npm run build` passes.
**Deploy:** Dev server running on http://localhost:5173.

**Scores (pending browser grading):** Features 1-11: pending. Feature 12 (Private Rooms/DMs): pending.

---

## Level 11 Upgrade — Draft Sync (no iteration needed)

**What was added:**
- `messageDraft` table in `schema.ts`: `id`, `userIdentity`, `roomId`, `text`, `updatedAt` fields; btree index on `userIdentity` and `roomId`
- `saveDraft` reducer in `index.ts`: upserts or deletes a draft per user per room (empty text = delete)
- Client App.tsx:
  - Subscribe to `SELECT * FROM message_draft`
  - `useTable(tables.messageDraft)` + `draftSaveTimerRef` + `lastServerDraftRef`
  - `handleMessageInput`: debounced draft save (300ms) via `conn.reducers.saveDraft`
  - `handleSelectRoom`: saves current draft for old room, loads draft for new room synchronously from `messageDrafts` state
  - `handleSendMessage`: clears draft via `conn.reducers.saveDraft({ text: '' })` after sending
  - `useEffect([messageDrafts, selectedRoomId])`: live cross-session sync — updates `messageInput` when server draft changes (only if value differs from last known server draft)
- Bindings regenerated: `message_draft_table.ts`, `save_draft_reducer.ts`

**Build:** `npx tsc --noEmit` passes with 0 errors; `npm run build` passes.
**Deploy:** Backend republished (additive migration, no data loss); bindings regenerated; dev server running on http://localhost:5173.

**Scores (pending browser grading):** Features 1-13: 39/39 (verified in prior levels). Feature 14 (Draft Sync): pending.
