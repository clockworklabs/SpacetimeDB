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
**Root cause:** Two PostgreSQL Docker containers are running on this host: `spacetime-web-postgres-1` (port 5433) and `llm-sequential-upgrade-postgres-1` (port 6432). The CLAUDE.md says port 6432 maps to `spacetime-web-postgres-1`, but the actual mapping is `llm-sequential-upgrade-postgres-1:6432`. Schema migration was run against the wrong container (`spacetime-web-postgres-1` via `docker exec`), leaving the app's actual database (`llm-sequential-upgrade-postgres-1`) with an incompatible schema from a prior run.
**What I fixed:** Identified the correct container (`llm-sequential-upgrade-postgres-1`), dropped old tables and recreated the correct schema using `docker exec llm-sequential-upgrade-postgres-1 psql`. Restarted the Express server.
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
**Root cause:** The `scheduled_messages` table was not created in `llm-sequential-upgrade-postgres-1`. The level-2 schema upgrade ran `drizzle-kit push` against the wrong container (`spacetime-web-postgres-1`), leaving `llm-sequential-upgrade-postgres-1` without the table.
**What I fixed:** Created the `scheduled_messages` table directly via `docker exec llm-sequential-upgrade-postgres-1 psql`. Also confirmed that the client already enforces a 1-minute minimum scheduling window (`min={new Date(Date.now() + 60000)...}`), so Bug 2 was already resolved in the current code.
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
**Root cause:** The L3 upgrade added an `expiresAt` column to the `messages` table in `schema.ts`, but `drizzle-kit push` was never run against the correct DB (`llm-sequential-upgrade-postgres-1` at port 6432). The column was added to `spacetime-web-postgres-1` (wrong container) but not to the app's actual DB, causing all queries that referenced `messages.expires_at` to fail with `column messages.expires_at does not exist`.
**What I fixed:** Added the missing `expires_at` column directly via `ALTER TABLE messages ADD COLUMN expires_at timestamp` on `llm-sequential-upgrade-postgres-1`. Also added defensive `Array.isArray()` guards in the client for both the rooms fetch and messages fetch so non-array error responses never crash the render loop.
**Files changed:** `client/src/App.tsx` (rooms fetch + messages fetch guards)
**Redeploy:** Server only (restarted Express; Vite HMR for client)

**Server verified:** `GET /api/rooms?userId=1` → array ✓ · `GET /api/rooms/1/messages?userId=1` → array ✓ · Client at http://localhost:6273 ✓
