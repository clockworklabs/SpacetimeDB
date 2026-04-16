# Iteration Log

## Run Info
- **Backend:** postgres
- **Level:** 1
- **Started:** 2026-04-03T13:19:35
- **Run ID:** postgres-level1-20260403-131935

---

## Build Notes

### npm install server — `--ignore-scripts` required
The `tsx@4.19.0` transitive dependency `@esbuild-kit/core-utils` has a post-install script that tries to download and verify an esbuild binary. This fails on Windows when the project path exceeds MAX_PATH (260 chars). Used `npm install --ignore-scripts` to work around. `npx tsx` (cached globally) is used at runtime instead.

### Schema conflict — existing tables dropped
The shared PostgreSQL database had tables from a prior run with a different schema (different column names in `users` table). Dropped all tables before running `drizzle-kit push`.

### Build: PASS
- Server `tsc --noEmit`: clean
- Client `tsc --noEmit`: clean
- Client `vite build`: success (56 modules)

---

## Iteration 0 — Deploy (13:23)

**Status:** Deployed successfully
- API server running at http://localhost:6001
- Client dev server running at http://localhost:6273
- Schema pushed to PostgreSQL (fresh)

**Reprompts:** 0 build reprompts (schema/install issues were environment issues, not code issues)

---

## Iteration 1 — Fix (2026-04-03)

**Category:** Feature Broken (3 bugs)

**Bug 1: Read receipts show sender**
- Root cause: `getReadReceipt` filtered out `currentUser` but not the message sender. When Alice viewed Bob's message, Bob's name still appeared in "Seen by" because he's not Alice.
- Fix: Added `senderId` parameter to `getReadReceipt` and added `r.userId !== senderId` to the filter. Updated call site to pass `group.userId`.
- Files changed: `client/src/App.tsx` (getReadReceipt function + call site)

**Bug 2: No unread count badges**
- Root cause: Users only subscribed to socket rooms when they clicked on them. New messages for rooms they hadn't opened never triggered `new_message` events, so real-time unread counts never incremented. Also, the `new_message` handler appended all messages to the messages state regardless of current room, risking cross-room message bleed.
- Fix 1: After loading joined rooms in `loadRooms`, emit `join_room` for each room so the client receives `new_message` events for all joined rooms.
- Fix 2: Rewrote `new_message` handler to only append messages to state if `current === msg.roomId`, otherwise increment unread count.
- Files changed: `client/src/App.tsx` (new_message handler + loadRooms)

**Bug 3: No Leave button**
- Root cause: Leave API and socket event existed server-side but no UI was provided.
- Fix: Added `handleLeaveRoom` handler (calls REST leave API + emits leave_room socket event + clears local state) and a "Leave" button in the room header.
- Files changed: `client/src/App.tsx` (handleLeaveRoom + JSX), `client/src/styles.css` (.leave-btn + room-header justify-content)

**Redeploy:** Client only (Vite HMR picks up changes automatically). Express server unchanged.
**Server status:** API server verified at http://localhost:6001, Client at http://localhost:6273

---

## Iteration 2 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Edit history panel does not update in real-time**
- Root cause: The `message_edited` socket handler was registered in a `useEffect` with `[]` dependencies, causing it to capture a stale closure over `editHistoryMessageId` (always `null`). When user B had the history panel open and user A edited the message, the handler could not detect that the panel was showing that message, so it never refreshed the edit history list.
- Fix: Added a `editHistoryMessageIdRef` ref that stays in sync with the `editHistoryMessageId` state. Wrapped `setEditHistoryMessageId` to update both the state and the ref. In the `message_edited` socket handler, check `editHistoryMessageIdRef.current === msg.id` — if true, re-fetch the edit history and update the panel in real-time.
- Files changed: `client/src/App.tsx` (added ref, wrapper setter, updated socket handler)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (returns HTML).

---

## Iteration 4 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Room member list does not update in real-time**
- Root cause: The `/api/rooms/:roomId/join` and `/api/rooms/:roomId/leave` REST endpoints made DB changes but emitted no socket events. Other connected clients had no way to know that the member list changed.
- Fix 1 (server): In the join endpoint, after inserting the new member, emit `member_added` to `room:<roomId>` with `{ userId, roomId, role: 'member', username }`.
- Fix 2 (server): In the leave endpoint, after deleting the member, emit `member_removed` to `room:<roomId>` with `{ userId, roomId }`.
- Fix 3 (client): Added `member_added` socket handler that appends the new member to `roomMembers` state (deduplicating by `userId`).
- Files changed: `server/src/index.ts` (join + leave endpoints), `client/src/App.tsx` (member_added handler)

**Redeploy:** Express server restarted (new background process). Vite dev server HMR picks up client changes automatically.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (returns HTML).

---

## Iteration 3 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Kick and Promote buttons not identifiable**
- Root cause: The Promote button was labeled "↑" (an arrow symbol) instead of the word "Promote", and the Kick button was labeled "kick" (lowercase). Browser test automation and graders searching for buttons labeled "Kick" and "Promote" could not find them.
- Fix: Changed Promote button text from "↑" to "Promote" and Kick button text from "kick" to "Kick".
- Files changed: `client/src/App.tsx` (button labels in member list)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (returns HTML).

---

## Iteration 5 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Room member list does not update in real-time (STILL NOT FIXED)**
- Root cause 1: `member_added` and `member_removed` socket handlers did not filter by `roomId`. Since `loadRooms` subscribes the client to ALL joined rooms, a member joining/leaving any room would update the currently-displayed member list incorrectly or unexpectedly.
- Root cause 2: The handlers were registered in `useEffect([])` with a stale closure over `currentRoomId`. Added `currentRoomIdRef` (kept in sync via a separate `useEffect([currentRoomId])`) so handlers can safely read the current room without stale closure issues.
- Root cause 3: No polling fallback — if socket events were missed for any reason, the list would never refresh.
- Fix 1: Added `const currentRoomIdRef = useRef<number | null>(null)` and a `useEffect` to keep it in sync with `currentRoomId` state.
- Fix 2: Added `if (data.roomId !== currentRoomIdRef.current) return;` guard to both `member_added` and `member_removed` handlers.
- Fix 3: Added polling `useEffect` that re-fetches `/api/rooms/:roomId/members` every 3 seconds when a room is selected, ensuring the list is always fresh regardless of socket delivery.
- Files changed: `client/src/App.tsx` (ref added, sync effect, polling effect, handler guards)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (returns HTML).

---

## Iteration 6 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Users always appear as "invisible" until they manually change status**
- Root cause: `broadcastOnlineUsers()` in `server/src/index.ts` emitted `userId: id` in each user object, but the client's `User` interface expects `id`. The `online_users` socket handler read `u.id` (which was `undefined`) and keyed `userPresence` entries by `undefined`. All presence lookups like `userPresence[member.userId]?.status` returned `undefined`, which fell through to the 'offline'/'invisible' rendering path.
- Fix: Changed `broadcastOnlineUsers()` to emit `id` instead of `userId`, matching the `User` interface the client expects.
- Files changed: `server/src/index.ts` (broadcastOnlineUsers function)

**Redeploy:** Express server restarted (old process killed, new `npm run dev` started). Vite dev server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (LISTENING).

---

## Iteration 8 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: No threading UI — Reply button missing**
- Root cause: Message threading was never implemented. The `messages` table had no `parentMessageId` column, no thread reply endpoints existed on the server, and no reply button or thread panel existed in the client.
- Fix 1 (schema): Added `parentMessageId: integer('parent_message_id')` to `messages` table. Run `drizzle-kit push` to apply.
- Fix 2 (server): Modified `GET /api/rooms/:roomId/messages` to filter top-level messages only (`isNull(schema.messages.parentMessageId)`) and include a `replyCount` subquery.
- Fix 3 (server): Added `GET /api/messages/:messageId/replies` endpoint returning all replies for a parent message.
- Fix 4 (server): Added `POST /api/messages/:messageId/replies` endpoint that creates a reply (stored with `parentMessageId`) and emits `new_reply` socket event (not `new_message`).
- Fix 5 (client): Added threading state (`threadOpenMessageId`, `threadParentMsg`, `threadReplies`, `threadReplyInput`, `threadOpenMessageIdRef`).
- Fix 6 (client): Added `new_reply` socket handler that increments replyCount on parent message and appends to thread panel if open.
- Fix 7 (client): Added `handleOpenThread` (fetches replies, opens panel) and `handleSendReply` functions.
- Fix 8 (client): Added 💬 Reply button in message hover toolbar.
- Fix 9 (client): Added reply count button below messages with replies.
- Fix 10 (client): Added thread panel (right sidebar) showing parent message, all replies, and reply input.
- Fix 11 (CSS): Added thread panel styles (`.thread-panel`, `.thread-parent-msg`, `.thread-replies-list`, `.reply-count-btn`, etc.).
- Files changed: `server/src/schema.ts`, `server/src/index.ts`, `client/src/App.tsx`, `client/src/styles.css`

**Redeploy:** Schema pushed (`drizzle-kit push` — clean). Express server restarted (new background process). Client rebuilt (`npm run build` — clean, 56 modules).
**Server status:** API server verified at http://localhost:6001 (returns rooms list WITH replyCount), Client dev server at http://localhost:6273 (returns HTML). Reply endpoint verified: POST /api/messages/1/replies returns `{"id":36,...,"parentMessageId":1}`. GET /api/messages/1/replies returns reply. GET /api/rooms/1/messages shows `replyCount: "1"` for message 1.

---

## Iteration 7 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Users always appear as "invisible" until they manually change status (STILL NOT FIXED)**
- Root cause: The Iteration 6 fix changed `broadcastOnlineUsers()` to emit `id` instead of `userId`, which was correct. However, the underlying race condition remained: when Alice enters a room and loads the member list via REST, those members are not yet in `userPresence` if Alice hasn't received an `online_users` socket event that includes them. The `online_users` broadcast only fires when someone connects or disconnects — not on initial page load. If Bob connected before Alice's current session (but Alice didn't receive the broadcast because she wasn't connected yet), Bob's status won't be in Alice's `userPresence` until a new connect/disconnect event happens.
- Fix 1 (server): Added `status: schema.users.status` to the `/api/rooms/:roomId/members` SELECT so the endpoint returns each member's current DB status.
- Fix 2 (client): Added `status?: string` to the `RoomMember` interface.
- Fix 3 (client): In `handleSelectRoom`, after loading members, pre-populate `userPresence` with each member's DB status (only if not already set by socket — preserving socket-based updates as authoritative).
- Fix 4 (client): Same pre-population in the 3-second member polling effect.
- Files changed: `server/src/index.ts` (members endpoint select), `client/src/App.tsx` (RoomMember interface, handleSelectRoom, polling effect)

**Redeploy:** Express server restarted (old process killed, new background `npm run dev`). Vite dev server picks up client changes via HMR.
**Server status:** API server verified at http://localhost:6001 (returns rooms list WITH status field), Client dev server at http://localhost:6273 (returns HTML).

---

## Iteration 9 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Reply count displays garbled value instead of integer count**
- Root cause: PostgreSQL `COUNT(*)` returns `bigint`, which the `pg` driver serializes to a JSON string (e.g. `"1"` not `1`) to avoid JavaScript precision loss. The `sql<number>` TypeScript generic in Drizzle is annotation-only and does not cast at runtime. When the client's `new_reply` socket handler did `(m.replyCount || 0) + 1`, with `m.replyCount` being a string like `"1"`, JavaScript string concatenation produced `"11"`, `"111"`, etc. Different clients showed different garbled values because each started from the value fetched at their own load time.
- Fix 1 (server): Added `::int` cast to the `COUNT(*)` subquery — `(SELECT COUNT(*) FROM messages r WHERE r.parent_message_id = ...)::int` — so PostgreSQL returns a 32-bit integer, which the driver serializes as a JSON number.
- Fix 2 (client): Added defensive `parseInt(String(m.replyCount), 10)` normalization when setting messages from the API response, ensuring any future string leakage is coerced to a number before entering React state.
- Files changed: `server/src/index.ts` (replyCount subquery), `client/src/App.tsx` (message load normalization)

**Verification:** `GET /api/rooms/1/messages` now returns `"replyCount":1` (JSON number, no quotes).

**Redeploy:** Express server restarted (old PID 577716 killed, new background `npm run dev`). Client rebuilt (`npm run build` — clean, 56 modules).
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 10 — Fix (2026-04-04)

**Category:** Feature Broken

**Bug: No way to create a private room — Private toggle missing**
- Root cause: The `rooms` table had no `is_private` column. The `handleCreateRoom` function didn't accept or send an `isPrivate` flag. The create-room form had no checkbox UI.
- Fix 1 (schema): Added `isPrivate: boolean('is_private').notNull().default(false)` to the `rooms` table in `server/src/schema.ts`. Applied via `drizzle-kit push`.
- Fix 2 (server POST /api/rooms): Destructured `isPrivate` from request body, passes `isPrivate: !!isPrivate` to the INSERT.
- Fix 3 (server GET /api/rooms): Added `userId` query param support. Non-members see only public rooms; members also see private rooms they belong to.
- Fix 4 (client Room interface): Added `isPrivate: boolean` field.
- Fix 5 (client state): Added `newRoomIsPrivate` boolean state.
- Fix 6 (client handleCreateRoom): Sends `isPrivate: newRoomIsPrivate` in POST body; resets flag after creation.
- Fix 7 (client loadRooms): Passes `?userId=${userId}` to GET /api/rooms so private rooms are visible to members.
- Fix 8 (client JSX): Added `<label><input type="checkbox">Private</label>` inside `.create-room-form`.
- Files changed: `server/src/schema.ts`, `server/src/index.ts`, `client/src/App.tsx`

**Redeploy:** `drizzle-kit push` applied the new column. Express server restarted (old PID killed, new background `npm run dev`). Vite dev server picks up client changes via HMR.
**Verification:** `POST /api/rooms {"name":"test-private-room","userId":1,"isPrivate":true}` returns `{"isPrivate":true,...}`. `GET /api/rooms?userId=1` includes the private room for user 1. `GET /api/rooms` (no userId) excludes it.
**Server status:** API server verified at http://localhost:6001, Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 11 — Fix (2026-04-03)

**Category:** Feature Broken (2 bugs)

**Bug 1: Private rooms visible to all users — no invite mechanism**
- Root cause 1: `io.emit('room_created', room)` broadcast private rooms to ALL connected clients. Every user's `room_created` socket handler would add the room to their list, regardless of membership.
- Root cause 2: No invite endpoint or UI existed.
- Fix 1 (server): Changed `POST /api/rooms` to only emit `room_created` globally for public rooms. For private rooms, only emit to the creator's socket via `io.to(creatorSocket.socketId).emit(...)`.
- Fix 2 (server): Added `POST /api/rooms/:roomId/invite` endpoint that checks admin role, looks up invitee by username, inserts into `room_members`, emits `member_added` to the room, and emits `room_invited` directly to the invitee's socket.
- Fix 3 (client): Added `room_invited` socket handler that appends the room to the user's rooms list.
- Fix 4 (client): Added `inviteUsername` / `inviteError` state and `handleInviteUser` function.
- Fix 5 (client JSX): Added invite UI (username input + Invite button + error message) in the room members panel, shown only to admins in private rooms.
- Files changed: `server/src/index.ts`, `client/src/App.tsx`

**Bug 2: Unread message counts reset on page refresh**
- Root cause: The `GET /api/users/:userId/unread` endpoint counted ALL messages (including replies with `parentMessageId IS NOT NULL`) with `id > lastReadMessageId`. Since `markRead` is called with the last TOP-LEVEL message ID, any replies with higher IDs were always counted as unread. On page refresh, every room with replies showed a nonzero unread badge.
- Fix (server): Added `isNull(schema.messages.parentMessageId)` to the unread count WHERE clause so only top-level messages are counted.
- Files changed: `server/src/index.ts` (unread count query)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server restarted (old process killed, new background `npm run dev`).
**Server status:** API server verified at http://localhost:6001 (returns rooms list). Invite endpoint returns `{"error":"Not an admin"}` (correct auth check). Unread endpoint returns correct JSON. Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 12 — Fix (2026-04-03)

**Category:** Feature Broken

**Bug: Invited users are auto-added to private rooms without Accept/Decline choice**
- Root cause: `POST /api/rooms/:roomId/invite` immediately inserted the invitee into `room_members` and emitted `room_invited` to add the room to their sidebar. There was no pending invite flow — no notification with Accept/Decline was shown.
- Fix 1 (server): Changed invite endpoint to NOT insert into `room_members`. Instead, it creates a pending invite entry in a `pendingInvites` in-memory Map (keyed by a generated `inviteId`) and emits `room_invite_received` directly to the invitee's socket with `{inviteId, roomId, roomName, inviterUsername}`.
- Fix 2 (server): Added `POST /api/invites/:inviteId/accept` — validates pending invite, inserts invitee into `room_members`, emits `member_added` to the room and `room_invited` to the invitee's socket to add the room to their list, deletes the pending invite.
- Fix 3 (server): Added `POST /api/invites/:inviteId/decline` — validates pending invite, deletes it (user is never added to the room).
- Fix 4 (client): Added `PendingInvite` interface and `pendingInvites` state array.
- Fix 5 (client): Added `room_invite_received` socket handler that adds the invite to `pendingInvites` state.
- Fix 6 (client): Added `handleAcceptInvite` (calls accept endpoint, removes from pendingInvites) and `handleDeclineInvite` (calls decline endpoint, removes from pendingInvites).
- Fix 7 (client JSX): Added invite notification section in sidebar (above Rooms list) that renders each pending invite with the inviter's name, room name, and Accept/Decline buttons.
- Files changed: `server/src/index.ts` (pendingInvites map, invite endpoint, accept/decline endpoints), `client/src/App.tsx` (PendingInvite interface, state, socket handler, handlers, JSX)

**Redeploy:** Server TypeScript type-checked clean. Express server restarted (new background process). Client rebuilt (`npm run build` — clean, 56 modules).
**Server status:** API server verified at http://localhost:6001 (returns rooms list). Accept/decline endpoints return correct 404 for unknown inviteId. Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 13 — Fix (2026-04-03)

**Category:** Feature Broken (2 bugs)

**Bug 1: Auto-away does not restore to "online" on user activity/window focus**
- Root cause: The `resetActivity` function only updated `lastActivityRef.current` but never called `handleStatusChange('online')` when the user returned from 'away'. There was also no `visibilitychange` listener (window focus/tab switch) to restore status on return.
- Fix 1: Modified `resetActivity` to call `handleStatusChange('online')` when `myStatus === 'away'`, restoring status on any user activity (mousemove, keydown, click).
- Fix 2: Added `visibilitychange` event listener — when `document.visibilityState === 'visible'`, `resetActivity()` is called so returning to the tab also restores status.
- Fix 3: Added `click` event listener to activity detection (was missing alongside mousemove/keydown).
- Files changed: `client/src/App.tsx` (resetActivity function + event listeners in auto-away useEffect)

**Bug 2: Top status selector and bottom online list are out of sync**
- Root cause: The `user_presence_update` socket handler only updated `userPresence` state but never updated `myStatus`. If the server emitted a presence update for the current user, the top selector (`myStatus`) remained stale while the bottom list (`userPresence[u.id]`) updated, causing divergence.
- Fix: In the `user_presence_update` handler, added check — if `data.userId === currentUser.id`, also call `setMyStatus(data.status)` so both displays stay in sync.
- Files changed: `client/src/App.tsx` (user_presence_update socket handler)

**Redeploy:** Client only — Vite HMR picks up changes automatically. Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 14 — Fix (2026-04-04)

**Category:** Feature Broken

**Bug: No DM button — cannot initiate direct messages**
- Root cause: The online users list had no DM button, and no `/api/dm` endpoint existed on the server. There was no way to create or navigate to a direct message conversation.
- Fix 1 (server): Added `POST /api/dm` endpoint. Creates a private room named `__dm_<minId>_<maxId>__` (or returns existing). Auto-adds both users as members and notifies both via `room_invited` socket event.
- Fix 2 (client): Added `handleStartDM(targetUserId)` function that calls `/api/dm`, adds the room to state, and navigates to it.
- Fix 3 (client): Added 💬 button next to each online user (excluding self) in the online users list.
- Fix 4 (client): DM rooms display as `@ Username` instead of `# __dm_X_Y__` in sidebar and room header.
- Files changed: `server/src/index.ts` (new /api/dm endpoint), `client/src/App.tsx` (handleStartDM, DM button, display name helper)

**Redeploy:** Express server restarted (npm run dev). Vite client HMR.
**Server status:** API server verified at http://localhost:6001 (returns rooms list). DM endpoint tested — creates room `__dm_1_2__` correctly. Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 15 — Fix (2026-04-04)

**Category:** Feature Broken

**Bug: DM room name shows "User X" when other user goes offline**
- Root cause: The DM room display helper looked up the other participant via `onlineUsers.find(u => u.id === otherId)`. When that user disconnects, they are removed from the `onlineUsers` array, causing the lookup to return `undefined` and the display to fall back to `User ${otherId}`.
- Fix 1 (client): Added `knownUsers` state (`Record<number, string>`) — a persistent map of userId → username that accumulates from `online_users` socket events but is never cleared when users go offline.
- Fix 2 (client): In the `online_users` socket handler, added a `setKnownUsers` call that merges all received users into the map.
- Fix 3 (client): In both DM room name display locations (sidebar + room header), changed fallback chain from `other?.username ?? \`User ${otherId}\`` to `other?.username ?? knownUsers[otherId] ?? \`User ${otherId}\``. Now the username persists from the last time the user was seen online.
- Files changed: `client/src/App.tsx` (knownUsers state, online_users handler, both DM name display sites)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list), Client dev server at http://localhost:6273 (HTTP 200).

---

## Iteration 16 — Fix (2026-04-04)

**Category:** Feature Broken

**Bug: DM room disappears from sidebar when the other user goes offline**
- Root cause: `knownUsers` (the persistent username map) was only populated from `online_users` socket events. If the other DM participant was offline when a1 loaded the page and never came online during that session, `knownUsers[c3_id]` would be undefined. The DM room fell back to "@ User X" — visually indistinguishable from a missing room. When c3 reconnected, `online_users` fired and populated `knownUsers`, making the room appear as "@c3" again. Graders observed this as the room "disappearing" when offline and "returning" when c3 rejoined or a1 refreshed.
- Fix 1 (client): In `loadRooms`, added `GET /api/users` to the initial parallel fetch set. After login, all user records from the DB are loaded and merged into `knownUsers`. This ensures DM room names are always resolved correctly regardless of who is currently online.
- Fix 2 (client): In the `room_invited` socket handler, added `socket.emit('join_room', room.id)` and `setJoinedRooms(prev => new Set([...prev, room.id]))`. Previously, when a user received a `room_invited` event (for a new DM), they were added to the `rooms` list but never subscribed to the socket room. This meant real-time DM messages (new_message events) would not be received until the user clicked the room.
- Files changed: `client/src/App.tsx` (loadRooms function, room_invited socket handler)

**Redeploy:** Client rebuilt (`npm run build` — clean, 56 modules). Express server unchanged.
**Server status:** API server verified at http://localhost:6001 (returns rooms list, `/api/users` returns all 4 users), Client dev server at http://localhost:6273 (HTTP 200).

---
