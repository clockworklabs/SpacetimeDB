# SpacetimeDB build friction

Appended after every SpacetimeDB run by `stdb-report.mjs`. The point is not the
score — it is what the model fought with, since that is what SpacetimeDB can
actually fix. Token counts are the CLI's own usage numbers, not estimates.

Runs whose `run.json` says `contaminated: true` must not be read as evidence of
anything: a build that read the marking scheme was not solving the same problem.

---
## 2026-08-07 13:40 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 41/48, $5.968, 1 fix round(s)

**Tokens** (from the CLI's own usage, 2 session(s), 198 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 21,397,543 | 98% |
| cache write | 470,807 | 2% |
| fresh input | 396 | 0% |
| output | 224,802 | — |

**Where it got stuck** — 7 build failure(s) of 118 tool calls (plus 2 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/test-wir |
| 1 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey< |
| 1 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2046 ms: Could not connect to server |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2035 ms: Could not connect to server |
| 1 | Exit code 1 'vswhere.exe' is not recognized as an internal or external command, operable program or |

By SpacetimeDB surface: other (3), server API (schema / reducers) (3), client SDK (subscriptions) (1)

**Re-read** — 9 read(s) of generated bindings

- 2x `client/src/App.tsx`
- 2x `client/src/index.css`
- 1x `spacetime-ecom-run0-20260807125914/app/check-hooks.sh`
- 1x `spacetime-ecom-run0-20260807125914/app/.sandbox-settings.json`
- 1x `src/server/views.ts`
- 1x `src/module_bindings/index.ts`

---
**Behavioural review** — 2 finding(s) with verified evidence

- **View return-type mismatch dumps an unreadable, multi-hundred-token structural type instead of a named type** *(generated bindings)*
  - cost: The model couldn't diagnose the one-line return-type bug from the TS error alone; it had to grep and read three separate SDK source files (schema.ts, views.ts, server/index.ts) across four tool calls just to find the names ViewCtx and ViewReturnTypeBuilder before it could fix the reducer/view code.
  - evidence: `src/index.ts(224,28): error TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<{ id: U64ColumnBuilder<{ isPrimaryKey: true; isAutoIncre`
  - possible fix: Give the reducer/view context and return-type builders named type aliases (e.g. ViewCtx<Schema>) so TypeScript errors report a short recognizable name instead of expanding the full structural type, and document the view() return-type contract in the public API docs so this doesn't require reading server SDK internals.
- **Generated TS bindings split every table and reducer into its own tiny file with no consolidated summary** *(generated bindings)*
  - cost: To learn table accessor names and reducer argument shapes before writing the React client, the model had to issue 8 separate Read calls (one per table binding file) plus a shell loop catting each individual reducer binding file — work a single generated summary/types file would have avoided.
  - evidence: `=== add_to_cart_reducer.ts === export default { itemId: __t.u64(), }; === buy_now_reducer.ts === export default { itemId`
  - possible fix: Emit one consolidated bindings summary (e.g. a types.ts or doc block in index.ts) listing all table row shapes and reducer signatures together, so consumers don't need to open every generated per-table/per-reducer file individually.

---
## 2026-08-07 — module runtime has no crypto (found by hand, both tracks)

Not caught by either automated pass — nothing errored, which is exactly the
"misuse that succeeded" class the review exists for. Found by comparing shipped
auth code across backends.

**Every postgres and mongo build wrote one line:**

- evidence: `import bcrypt from 'bcryptjs';` (postgres-run0, mongodb-run0, mongodb-ecom-run0)

**The SpacetimeDB builds, lacking any crypto in the module runtime, produced three
different outcomes across two runs:**

1. Hand-implemented SHA-256 from scratch, including UTF-8 encoding:
   - evidence: `// Minimal, dependency-free SHA-256 for password hashing inside the deterministic` / `// module runtime (no Node 'crypto', no guaranteed WebCrypto/TextEncoder).` (spacetime-run0 transcript, written to crypto.ts)
2. Shipped a non-cryptographic hash with an apology in the comment:
   - evidence: `// Not a cryptographic hash: SpacetimeDB modules run deterministically and have` / `// no access to system crypto. Good enough to avoid storing plaintext passwords.` (results/spacetime-run0/app/backend/spacetimedb/src/index.ts:22)
3. Shipped PLAINTEXT password storage and comparison:
   - evidence: `if (!acc || acc.password !== password) throw new SenderError('Invalid username or password');` (results/spacetime-ecom-run0/source/backend/spacetimedb/src/index.ts:71)

**Cost:** turns spent reimplementing primitives the other stacks import, and a
security defect shipping silently. **Possible fix:** expose a deterministic hash
primitive (or password-hashing helper) in the module runtime / SDK, and document
the recommended auth pattern in the skill docs.

---

