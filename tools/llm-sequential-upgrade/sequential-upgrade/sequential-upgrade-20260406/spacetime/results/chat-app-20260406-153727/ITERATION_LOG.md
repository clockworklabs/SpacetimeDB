# Iteration Log

## Iteration 1 — Fix (15:38)

**Category:** Compilation/Build
**What broke:** Backend publish failed with `Cannot read properties of undefined (reading 'name')` in `getColName`
**Root cause:** Multi-column named indexes in `table()` options used invalid column references (trying to use snake_case column names that didn't resolve)
**What I fixed:** Removed multi-column `indexes: [...]` from all tables; used inline `.index('btree')` modifiers on individual columns instead
**Files changed:** `backend/spacetimedb/src/schema.ts` (all table definitions)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6173 ✓

## Iteration 2 — Fix (22:05)

**Category:** Runtime/Crash
**What broke:** Ephemeral messages never appeared — `sendEphemeralMessage` reducer panicked with "The instance encountered a fatal error"
**Root cause:** `expiresAt` field was set to a plain JS object `{ microsSinceUnixEpoch: expiryMicros }` instead of a proper `Timestamp` class instance. `Timestamp` is a class (not just an interface) with internal field `__timestamp_micros_since_unix_epoch__` and a getter `microsSinceUnixEpoch`. The SpacetimeDB BSATN serializer could not serialize the plain object, causing a fatal panic.
**What I fixed:** Imported `Timestamp` from `spacetimedb` in `index.ts` and changed `expiresAt: { microsSinceUnixEpoch: expiryMicros }` to `expiresAt: new Timestamp(expiryMicros)`
**Files changed:** `backend/spacetimedb/src/index.ts` (lines 3, 168)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6173 ✓
