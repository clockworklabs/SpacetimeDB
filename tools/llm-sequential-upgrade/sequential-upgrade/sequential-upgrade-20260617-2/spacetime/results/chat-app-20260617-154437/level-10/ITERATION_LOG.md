# Iteration Log — chat-app-20260617-154437

## Generation — Level 1 (Initial)

**Reprompts (build fixes):** 1

### Fix 1 — Backend publish: missing schema re-export

**Category:** Compilation/Build
**What broke:** `spacetime publish` rejected with "You must `export default schema(...)`"
**Root cause:** `index.ts` imported schema from `schema.ts` but did not re-export it; the publish tool looks for `export default` at the module entry point
**What I fixed:** Added `export { default } from './schema';` to `backend/spacetimedb/src/index.ts`
**Files changed:** `backend/spacetimedb/src/index.ts` (added 1 line)
**Redeploy:** Server only (re-published module after fix)

**Server verified:** Client at http://localhost:6173 ✓

## Iteration 1 — Fix (L7 Bug)

**Category:** Feature Broken (presence correctness)
**What broke:** Closing one of several same-identity tabs immediately marked the user offline, even with another tab still active.
**Root cause:** `clientDisconnected` unconditionally set `online: false` on every connection close. SpacetimeDB fires `clientDisconnected` per ConnectionId, but a single Identity can hold multiple simultaneous connections.
**What I fixed:** Added an `active_connection` table (private, keyed by `connectionId`, indexed by `userIdentity`). On `clientConnected`, insert the connection row. On `clientDisconnected`, delete the row then count remaining rows for that identity — only set `online: false` when zero remain.
**Files changed:** `backend/spacetimedb/src/schema.ts` (added `activeConnection` table, updated schema export); `backend/spacetimedb/src/index.ts` (updated `onConnect`/`onDisconnect` to track connections)
**Redeploy:** Both (republished module + regenerated bindings + restarted Vite client)

**Server verified:** Client at http://localhost:6173 ✓
