# Iteration Log — chat-app-20260617-095800

## Reprompt 1 — Backend Compile (Phase 1)

**Category:** Compilation/Build
**What broke:** `tsc` not found; spacetime CLI rejected module with "no exported schema"
**Root cause:** `typescript` was missing from backend `devDependencies`; schema was only exported in `schema.ts` but the CLI entry point is `index.ts`
**What I fixed:** Added `typescript` to `devDependencies`; added `export { default } from './schema.js'` to `index.ts`
**Files changed:** `backend/spacetimedb/package.json`, `backend/spacetimedb/src/index.ts`
**Redeploy:** Server only

## Reprompt 2 — Backend tsconfig moduleResolution (Phase 1)

**Category:** Compilation/Build
**What broke:** `spacetimedb/server` subpath exports not resolved; `t` not found under `node` moduleResolution
**Root cause:** TypeScript's legacy `node` moduleResolution doesn't handle `exports` map in package.json; switching to `NodeNext` resolved the path but still flagged `t`; `bundler` resolution worked correctly
**What I fixed:** Changed `tsconfig.json` `moduleResolution` to `"bundler"` and `module` to `"ESNext"`
**Files changed:** `backend/spacetimedb/tsconfig.json`
**Redeploy:** Server only

## Reprompt 3 — Client unused imports (Phase 4)

**Category:** Compilation/Build
**What broke:** `noUnusedLocals` flagged 4 imported types (`Room`, `User`, `TypingIndicator`, `ReadReceipt`) that were only needed at runtime not as type annotations
**Root cause:** Types imported in App.tsx were not used as explicit TypeScript type annotations
**What I fixed:** Removed unused type imports, keeping only `Message` which is used in `getReadersForMessage`
**Files changed:** `client/src/App.tsx` (line 4)
**Redeploy:** Client only

**Server verified:** Client at http://localhost:6173 ✓
