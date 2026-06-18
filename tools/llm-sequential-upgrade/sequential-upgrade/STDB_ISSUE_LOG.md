# SpacetimeDB Benchmark — Issue Log & Skills Changelog

Tracks STDB-specific frictions the model hits during benchmark runs, and records changes
made to the official skills (`skills/typescript-server/SKILL.md`, `skills/typescript-client/SKILL.md`).

---

## Issue Log

| # | Issue | First seen | Error | Status |
|---|-------|-----------|-------|--------|
| 1 | **Schema not exported** — `index.ts` (module entry) doesn't re-export the schema default | every L1 (20260617-2, 20260618) | `Error: haven't exported your schema. You must export default schema(...)` | ✅ **resolved 2026-06-18** — added to typescript-server skill (Schema Export section) |
| 2 | **Readonly `useTable` rows** — model mutates/sorts a readonly array in place | L1 (20260618) | `TS4104: 'readonly {...}[]' is readonly and cannot be assigned to the mutable type` | ✅ **added to typescript-client skill** (`[...rows]` before sort) |
| 3 | **bigint in JSX** — bigint id/count rendered directly | L6 (20260617-2) | `TS2322: '0n' is not assignable to ReactNode` | ✅ **added to typescript-client skill** (`{Number(id)}`) |
| 4 | **Nullable `ctx.connectionId`** in lifecycle hooks | fix-L7 (20260617-2) | `TS2322: 'ConnectionId \| null' is not assignable to 'ConnectionId'` | ✅ **added to typescript-server skill** |
| 5 | **Optional columns in `insert`** — must list all optionals as `undefined` | L4–L12 (20260617-2) | type mismatch on `insert({...})` | ✅ **added to typescript-server skill** |
| 6 | **`ScheduleAt` time extraction** — verbose tagged-union handling, re-derived each level | L3–L12 | (no error; friction) | ✅ **added to typescript-server skill** |
| 7 | **Schema-migration publish abort** — plain publish refuses destructive migration + interactive `[y/N]` | L2,L3,L5,L7,L8,L9,L12 (20260617-2) | `Error: would require manual migration … --delete-data was not specified` | migration command added to `spacetime.md` (process, not a skill) |
| 8 | **Cross-shell slips** — PowerShell cmdlets in the Bash tool | both STDB & mongo | `New-Item/Start-Sleep: command not found`, `$null: ambiguous redirect` | note added to root `CLAUDE.md` |
| 9 | **`noUnusedLocals` build break** — unused var fails the build | L9 (20260617-2) | `TS6196: 'X' is declared but never used` | `noUnusedLocals/Parameters: false` in `spacetime-templates.md` |
| 10 | **Duplicate index** — same column indexed both inline (`.index('btree')`) and in the `indexes: [...]` array | L5 (20260618) | `Duplicate index definition` (publish-time, not tsc) | **pending** — candidate 1-line skill note; marginal (1 occurrence, self-fixed in 1 extra publish) |
| 11 | **Used legacy `rightSemijoin` instead of Views** — per-viewer access control's proper primitive is a per-user **View** (`spacetimedb.view`, scoped by `ctx.sender`), which **is already documented** in typescript-server skill (Views section). The model ignored it and grepped SDK source for the legacy semijoin-subscription path; the fix works but isn't idiomatic (RLS/semijoin is legacy, Views is the supported approach) | L6 fix (20260618) | (no error; ~half the $1.04 fix spent grepping SDK source for a legacy API it didn't need) | ✅ **resolved 2026-06-18** — added 3-line use-case framing to skill's Views section (names per-viewer access control + auto-revocation). **Validated** by isolated L6 re-test (run `-133559`): with only that prose added, the model used a per-user `spacetimedb.view` end-to-end, no semijoin, no UI-hide, **one pass, no fix** ($1.70 vs original $1.97 to-done). Also resolved the feared client-consumption gap on its own: views surface in generated bindings as table handles (`tables.myRoomMessages` via `useTable`/`subscriptionBuilder`), so **no client-skill doc needed** |

---

## Skills Changelog

Chronological record of edits to `skills/typescript-server/SKILL.md` and `typescript-client/SKILL.md`.

### 2026-06-18 — typescript-client/SKILL.md
- Added **Gotchas** section: `useTable` rows are `readonly` → copy with `[...rows]` before sort (issue #2);
  bigint not renderable in JSX → wrap in `Number()`/`String()` (issue #3).

### 2026-06-18 — typescript-server/SKILL.md (Views framing)
- Added 3-line use-case intro to the **Views** section: names per-user views (keyed on `ctx.sender`)
  as the per-viewer access-control primitive + the auto-revocation property (issue #11). No new code,
  no anti-pattern warning (kept it non-prescriptive). Validated by isolated L6 re-test: flipped the model
  from legacy semijoin / UI-hide to a proper per-user view, one pass, no fix.

### 2026-06-18 — typescript-server/SKILL.md
- Added: `ctx.connectionId` is nullable `ConnectionId | null` (issue #4); `insert()` requires every column,
  optionals as `undefined` (issue #5); reading time back from a `ScheduleAt` tagged union (issue #6).
- Extended **Schema Export**: module entry must export the schema; re-export
  `export { default } from './schema'` when tables/reducers are split (issue #1 — reclassified from
  "prescriptive" to fair SDK structure doc: it's module wiring, not task logic; recurs 3/3 runs).

Removed the duplicate gotchas from `spacetime.md` so SDK behavior lives only in the skills.
Prescriptive items (schema re-export #1) kept out. Issues #2–#6 are now **resolved** (in-skill).

<!-- Template for entries:
### YYYY-MM-DD — <skill file>
- Added: <what> — addresses issue #N (<short why>)
-->

---

## Run Observations

**20260618 (improvements + 5-min cache):**
- **L1 (generate):** hit #1 (schema-export) + #2 (readonly TS4104). #2's skill fix landed *after* L1,
  so this generate predated it — next run's L1 should avoid #2.
- **L2 (upgrade):** 0 hard errors, **1 publish attempt** (was 3). Migration note (#7) validated; clean.
- **#1 schema-export** (hit every generate, 3/3 runs): **resolved 2026-06-18** — reclassified as fair SDK
  structure doc, added to the skill's Schema Export section. Generate-only, so it takes effect on the
  *next* run's L1; this run's L3–L12 (upgrades) are unaffected.
