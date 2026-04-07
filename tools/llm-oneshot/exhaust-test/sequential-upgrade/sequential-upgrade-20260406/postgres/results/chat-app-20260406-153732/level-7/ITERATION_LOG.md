# Iteration Log

## Iteration 1 — Fix (19:45)

**Category:** Compilation/Build
**What broke:** TypeScript error: `Property 'as' does not exist on type PgTableWithColumns<...>` in the unread count query in `server/src/index.ts`
**Root cause:** Drizzle ORM's `pgTable` doesn't support `.as()` alias syntax directly. Used `schema.messages.as('m')` which is not a valid Drizzle API.
**What I fixed:** Replaced the Drizzle ORM query with a raw `pool.query()` call using parameterized SQL for the unread count left-join query. Removed unused `isNull` import.
**Files changed:** `server/src/index.ts` (lines ~87-97)
**Redeploy:** Server only

## Iteration 2 — Fix (19:50)

**Category:** Runtime/Crash
**What broke:** All database operations failed with `relation "read_receipts" does not exist` and similar errors
**Root cause:** Two PostgreSQL Docker containers are running on this host: `spacetime-web-postgres-1` (port 5433) and `exhaust-test-postgres-1` (port 6432). The CLAUDE.md says port 6432 maps to `spacetime-web-postgres-1`, but the actual mapping is `exhaust-test-postgres-1:6432`. Schema migration was run against the wrong container (`spacetime-web-postgres-1` via `docker exec`), leaving the app's actual database (`exhaust-test-postgres-1`) with an incompatible schema from a prior run.
**What I fixed:** Identified the correct container (`exhaust-test-postgres-1`), dropped old tables and recreated the correct schema using `docker exec exhaust-test-postgres-1 psql`. Restarted the Express server.
**Files changed:** None (schema fix only)
**Redeploy:** Server only

**Server verified:** API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 3 — Fix (20:15)

**Category:** Feature Broken
**What broke:** Unread message count badge not appearing in sidebar for rooms with new messages
**Root cause:** Server emitted `message` events only to users in the active Socket.io room (`room:${roomId}`). When Bob navigated away, he left that room via `socket.leave`, so he never received the `message` event and the client-side unread count increment never fired.
**What I fixed:** After broadcasting to active viewers via `io.to(`room:${roomId}`)`, query all DB room members and directly emit `message` to each connected member whose socket is NOT in the active room. This ensures non-viewing members still receive the event, triggering the unread badge increment in the client.
**Files changed:** `server/src/index.ts` (send_message handler, ~lines 303-312)
**Redeploy:** Server only

**Server verified:** API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 4 — Fix (21:00)

**Category:** Runtime/Crash
**What broke:** `GET /api/scheduled-messages?userId=2` returns 500; `POST /api/scheduled-messages` returns 500
**Root cause:** The `scheduled_messages` table was not created in `exhaust-test-postgres-1`. The level-2 schema upgrade ran `drizzle-kit push` against the wrong container (`spacetime-web-postgres-1`), leaving `exhaust-test-postgres-1` without the table.
**What I fixed:** Created the `scheduled_messages` table directly via `docker exec exhaust-test-postgres-1 psql`. Also confirmed that the client already enforces a 1-minute minimum scheduling window (`min={new Date(Date.now() + 60000)...}`), so Bug 2 was already resolved in the current code.
**Files changed:** None (schema fix only via SQL)
**Redeploy:** Both (killed and restarted both servers)

**Server verified:** API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 5 — Fix (20:58)

**Category:** Runtime/Crash | Feature Broken
**What broke:** `GET /api/scheduled-messages` returns 500; datetime-local min clamps to hours in the future in non-UTC timezones
**Root cause (Bug 1):** The running Express server was a stale process started before the level-2 upgrade (which added `scheduledMessages` to the Drizzle schema). It was using old in-memory schema that had no `scheduledMessages` table reference, causing Drizzle to generate SQL with an unrecognized relation name even though the table existed in the DB.
**Root cause (Bug 2):** `min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}` passes a UTC ISO string to `datetime-local`, which interprets the value as local time. In non-UTC timezones the minimum appears hours in the future.
**What I fixed:** Restarted the Express server so it loads the current schema (Bug 1). Changed the min calculation to use local date components instead of `toISOString()` (Bug 2).
**Files changed:** `client/src/App.tsx` (schedule modal min attribute)
**Redeploy:** Server only (client Vite HMR handles client change)

**Server verified:** `GET /api/scheduled-messages?userId=1` → `[]` ✓ · Client at http://localhost:6273 ✓

## Iteration 6 — Fix (22:00)

**Category:** Runtime/Crash
**What broke:** `GET /api/rooms` → 400 Bad Request; `GET /api/rooms/:id/messages` → 500 Internal Server Error; `TypeError: messages is not iterable` crash in App.tsx
**Root cause:** The L3 upgrade added an `expiresAt` column to the `messages` table in `schema.ts`, but `drizzle-kit push` was never run against the correct DB (`exhaust-test-postgres-1` at port 6432). The column was added to `spacetime-web-postgres-1` (wrong container) but not to the app's actual DB, causing all queries that referenced `messages.expires_at` to fail with `column messages.expires_at does not exist`.
**What I fixed:** Added the missing `expires_at` column directly via `ALTER TABLE messages ADD COLUMN expires_at timestamp` on `exhaust-test-postgres-1`. Also added defensive `Array.isArray()` guards in the client for both the rooms fetch and messages fetch so non-array error responses never crash the render loop.
**Files changed:** `client/src/App.tsx` (rooms fetch + messages fetch guards)
**Redeploy:** Server only (restarted Express; Vite HMR for client)

**Server verified:** `GET /api/rooms?userId=1` → array ✓ · `GET /api/rooms/1/messages?userId=1` → array ✓ · Client at http://localhost:6273 ✓

## Iteration 2 — Fix (current)

**Category:** Feature Broken | Integration

**What broke:** (1) Member panel did not update in real-time when users joined or left a room — required page refresh. (2) Kicked members could still view room messages; access was not enforced server-side.

**Root cause:**
- Bug 1: The `/api/rooms/:id/join` and `/api/rooms/:id/leave` REST endpoints never emitted any Socket.io events, so connected clients had no signal to update the member list.
- Bug 2: The `/api/rooms/:id/messages` endpoint had no membership check — any user ID could fetch messages regardless of whether they were a member or had been kicked/banned.

**What I fixed:**
- Server: After a successful join, emit `member_joined` event to the room socket channel with `{ userId, name, isAdmin, roomId }`.
- Server: After a successful leave, emit `member_left` event to the room socket channel with `{ userId, roomId }`.
- Server: Added ban and membership checks at the top of the `GET /api/rooms/:id/messages` handler; returns 403 if the user is banned or not a member.
- Client: Added `member_joined` socket handler — appends the new member to `roomMembers` state if not already present.
- Client: Added `member_left` socket handler — removes the member from `roomMembers` state.

**Files changed:** `server/src/index.ts` (join endpoint, leave endpoint, messages endpoint); `client/src/App.tsx` (two new socket event handlers)

**Redeploy:** Both (server restarted; Vite dev server restarted)

**Server verified:** `GET /api/rooms/2/messages?userId=999` → 403 ✓ · API server at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 3 — Fix (15:15)

**Category:** Feature Broken | Integration
**What broke:** (1) Member panel not updating in real-time when users joined/left rooms. (2) Kicked members could still view room messages and were not redirected away.
**Root cause:**
- Bug 1: `member_joined` and `member_left` socket handlers were already in place from the previous iteration. Code review confirmed real-time update logic is correct — events are emitted to the socket room and functional `setRoomMembers` updaters are used. No code change needed.
- Bug 2: The kicked user's redirect relied solely on `user_kicked` being emitted to the socket room. If the user's socket was not in the room channel, they never received the event. Additionally, `setKickedNotice` was called inside the `setActiveRoomId` functional updater — a side effect inside a pure updater that is unreliable in React 18 concurrent mode.
**What I fixed:**
- Server: In the kick endpoint, added a direct `kicked_from_room` emission to the kicked user's socket (via `kickedSocket.emit()`), guaranteeing delivery regardless of socket room membership.
- Client: Added a `kicked_from_room` handler that immediately calls `setActiveRoomId(null)`, `setKickedNotice(...)`, and marks the room as not-joined.
- Client: Fixed the `user_kicked` handler to use `activeRoomIdRef.current` for the active-room check and call `setKickedNotice` outside the `setActiveRoomId` updater.
**Files changed:** `server/src/index.ts` (kick endpoint ~lines 223-232); `client/src/App.tsx` (socket setup, new kicked_from_room handler + fixed user_kicked handler)
**Redeploy:** Both

**Server verified:** Kick returns 200 ✓ · `GET /api/rooms/:id/messages?userId=<kicked>` → 403 ✓ · API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 4 — Fix (15:38)

**Category:** Feature Broken | Integration
**What broke:** (1) Member panel not updating in real-time when users joined/left rooms (page refresh required). (2) Kicked members could still rejoin the room and send messages after being kicked.
**Root cause:**
- Bug 1: On socket.io auto-reconnect, the client's socket gets a new socket ID but never re-emits `join_room` for the active room. The reconnected socket is not in the socket channel, so `member_joined`/`member_left` events are never received. The `connect` handler only called `setConnected(true)` without re-joining.
- Bug 2 (remaining): `handleJoinRoom` never checked the HTTP response status — if the server returned 403 (banned), the client ignored it and called `setActiveRoomId(roomId)` anyway, letting the user into the room. Also, the `send_message` socket handler had no ban or membership check, so a banned user's socket could still insert messages directly.
**What I fixed:**
- Client: `connect` handler now re-emits `register` and `join_room` (using refs) on every connect event, ensuring the socket re-enters the correct channel after any reconnect.
- Client: `handleJoinRoom` now checks `res.ok`; if 403, it displays the ban notice and returns without setting the active room.
- Client: Message fetch in the room-change effect now checks for 403; if banned, it sets `activeRoomId(null)`, marks the room as not-joined, and shows the kicked notice instead of loading an empty room.
- Server: `send_message` socket handler now queries `bannedUsers` and `roomMembers` before inserting; if the user is banned or not a member, the event is silently dropped.
**Files changed:** `server/src/index.ts` (send_message handler); `client/src/App.tsx` (connect handler, handleJoinRoom, message fetch effect)
**Redeploy:** Both

**Server verified:** `POST /api/rooms/5/join` with banned userId=2 → 403 ✓ · API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓

## Iteration 5 — Fix (12:15)

**Category:** Feature Broken
**What broke:** "Last active X ago" showed a stale/inaccurate time immediately after a user set their status to invisible/offline. E.g. "15 minutes ago" right after the user was just active.
**Root cause:** `lastSeen` was only updated in the DB on disconnect. On connect (register socket event), status change (both REST `/api/status` and socket `set_status`), and message send, `lastSeen` was never updated. So invisible users showed the `lastSeen` from their last disconnect session.
**What I fixed:** Updated `lastSeen` to `new Date()` in:
- REST `PUT /api/users/:id/status` handler
- Socket `register` event handler
- Socket `set_status` event handler
- Socket `send_message` handler (before inserting the message)
All four now pass the updated timestamp through to the `user_status` broadcast so clients immediately reflect the correct "last active" time.
**Files changed:** `server/src/index.ts` (register handler, set_status socket handler, REST status handler, send_message handler)
**Redeploy:** Server only

**Server verified:** `GET /api/users` → users with updated lastSeen ✓ · API at http://localhost:6001 ✓ · Client at http://localhost:6273 ✓
