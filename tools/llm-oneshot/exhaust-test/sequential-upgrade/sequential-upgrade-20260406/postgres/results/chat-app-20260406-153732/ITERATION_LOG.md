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
