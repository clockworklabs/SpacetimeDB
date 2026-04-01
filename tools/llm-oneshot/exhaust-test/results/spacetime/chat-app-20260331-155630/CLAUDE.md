# Backend: SpacetimeDB

Instructions for generating, building, and deploying the **SpacetimeDB** backend.

---

## SDK Rules (CRITICAL)

**You MUST read the SDK rule file before generating ANY code.**

Find and read **`spacetimedb-typescript.mdc`** in `docs/static/ai-rules/` of the repo. This file contains everything you need:
- Table definition syntax (`table(OPTIONS, COLUMNS)` — indexes go in OPTIONS)
- Reducer definition syntax (export name becomes reducer name — NO string arg)
- Schema export syntax (`schema({ table1, table2 })` — object, NOT spread args)
- Lifecycle hooks must be `export const` (NOT bare calls)
- Client patterns (useTable returns tuple, connectionBuilder must be memoized)
- Hallucinated APIs to avoid
- Scheduled tables, timestamps, data visibility, React integration, project structure

---

## Pre-flight Check

```bash
spacetime server ping local
```

If SpacetimeDB is not running, STOP and report the error.

---

## Directory Structure

```
<app-dir>/
  backend/spacetimedb/
    package.json
    tsconfig.json
    src/
      schema.ts      # All tables and indexes
      index.ts       # All reducers and lifecycle hooks
  client/
    package.json
    vite.config.ts
    tsconfig.json
    index.html
    src/
      config.ts      # Module name and SpacetimeDB URI
      main.tsx       # React entry point
      App.tsx        # Main application component
      styles.css     # Dark theme styling
      module_bindings/  # Auto-generated (Phase 2)
```

---

## Phase 1: Generate Backend

- Create `backend/spacetimedb/package.json` (use template from patterns-typescript.mdc)
- Create `backend/spacetimedb/tsconfig.json` (use template from patterns-typescript.mdc)
- Create `backend/spacetimedb/src/schema.ts` — all tables and indexes
- Create `backend/spacetimedb/src/index.ts` — all reducers and lifecycle hooks
- Install and publish:
  ```bash
  cd <backend-dir> && npm install
  spacetime publish chat-app-<timestamp> --module-path <backend-dir>
  ```

**Module naming:** Use the timestamped folder name as the module name (e.g. `chat-app-20260330-143000`).

---

## Phase 2: Generate Bindings

```bash
spacetime generate --lang typescript --out-dir <client>/src/module_bindings --module-path <backend-dir>
```

Read the generated bindings to know the exact type names (table names, reducer signatures) before writing client code.

---

## Phase 3: Generate Client

Generate client files using the REAL binding types from Phase 2.

- Create `client/package.json` (use template from patterns-typescript.mdc)
- Create `client/vite.config.ts` (port 5173, NEVER 3000)
- Create `client/tsconfig.json` (use template)
- Create `client/index.html` (use template)
- Create `client/src/config.ts` — module name and SpacetimeDB URI
- Create `client/src/main.tsx` — React entry point
- Create `client/src/App.tsx` — main application component
- Create `client/src/styles.css` — dark theme styling

**CRITICAL:** Import from `./module_bindings` using the REAL generated type names, not guessed ones.

---

## Phase 4: Verify

```bash
cd <client-dir> && npm install
npx tsc --noEmit          # Type-check
npm run build             # Full production build
```

Both must pass. If either fails:
1. Read the error
2. Fix the code
3. Retry (up to 3 attempts)
4. Each fix counts as a **reprompt** — log it

---

## Phase 5: Deploy

```bash
# Kill any existing dev server
npx kill-port 5173 2>/dev/null || true

# Start dev server in background
cd <client-dir> && npm run dev &
```

Wait for the dev server to be ready (poll `http://localhost:5173` up to 30 seconds).

---

## Redeploy (for fix iterations)

- If **backend changed**: re-publish module, regenerate bindings if schema changed
  ```bash
  spacetime publish chat-app-<timestamp> --module-path <backend-dir>
  spacetime generate --lang typescript --out-dir <client>/src/module_bindings --module-path <backend-dir>
  ```
- If **client changed**: Vite HMR handles it automatically (or restart dev server if needed)

---

## Reference Files

| File (search for it) | Purpose |
|------|---------|
| `spacetimedb-typescript.mdc` | TypeScript SDK reference — the only file you need to read |
