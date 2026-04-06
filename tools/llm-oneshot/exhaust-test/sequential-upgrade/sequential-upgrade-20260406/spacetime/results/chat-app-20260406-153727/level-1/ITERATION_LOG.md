# Iteration Log

## Iteration 1 — Fix (15:38)

**Category:** Compilation/Build
**What broke:** Backend publish failed with `Cannot read properties of undefined (reading 'name')` in `getColName`
**Root cause:** Multi-column named indexes in `table()` options used invalid column references (trying to use snake_case column names that didn't resolve)
**What I fixed:** Removed multi-column `indexes: [...]` from all tables; used inline `.index('btree')` modifiers on individual columns instead
**Files changed:** `backend/spacetimedb/src/schema.ts` (all table definitions)
**Redeploy:** Server only

**Server verified:** Client at http://localhost:6173 ✓
