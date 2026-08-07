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

## 2026-08-07 17:16 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 47/49, $12.5737, 2 fix round(s)

**Tokens** (from the CLI's own usage, 5 session(s), 537 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 60,960,859 | 98% |
| cache write | 1,335,404 | 2% |
| fresh input | 1,074 | 0% |
| output | 654,883 | — |

**Where it got stuck** — 19 build failure(s) of 308 tool calls (plus 2 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | Error: Response text: Your cart is empty |
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/test-wir |
| 1 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey< |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2046 ms: Could not connect to server |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2035 ms: Could not connect to server |
| 1 | Exit code 1 'vswhere.exe' is not recognized as an internal or external command, operable program or |

By SpacetimeDB surface: server API (schema / reducers) (10), other (7), client SDK (subscriptions) (2)

**Re-read** — 11 read(s) of generated bindings

- 3x `client/src/App.tsx`
- 2x `client/src/App.tsx`
- 2x `client/src/index.css`
- 2x `spacetimedb/src/index.ts`
- 1x `spacetime-ecom-run0-20260807125914/app/check-hooks.sh`
- 1x `spacetime-ecom-run0-20260807125914/app/.sandbox-settings.json`

---
**Behavioural review** — 4 finding(s) with verified evidence

- **View handler type errors dump unreadable nested generic types** *(server API)*
  - cost: Model had to abandon the TS compiler error message entirely and grep/read SDK internals (views.ts, index.ts) across two tool calls just to discover the ViewCtx/ViewReturnTypeBuilder types needed to fix a view's return type
  - evidence: `src/index.ts(224,28): error TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<{ id: U64ColumnBuilder<{ isPrimaryKey: true; isAutoIncre`
  - possible fix: Give view handlers a named, exported ctx/return type (e.g. ViewCtx<Schema>) so type mismatches surface as a short, readable diagnostic instead of a multi-hundred-character structural type dump; document the view-return-type pattern directly rather than requiring a source read.
- **Zero-argument reducers still require passing an empty object** *(generated bindings)*
  - cost: Two TS2554 compile errors on client calls to zero-parameter reducers (signOut, checkout-style calls), requiring two separate Edit passes to add {} to every no-arg reducer call site
  - evidence: `src/App.tsx(208,19): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate a zero-arity call signature for reducers with no fields so callers can write reducers.signOut() instead of reducers.signOut({}).
- **CLI reducer invocation requires wire (snake_case) name, not the TS-declared name** *(CLI/publish)*
  - cost: One failed `spacetimedb-cli call` attempt using the camelCase reducer name as declared in TS source (signUp) before discovering the actual callable name is sign_up
  - evidence: `Error: No such reducer OR procedure 'signUp' for database 'stackbench-ecom-run0' resolving to identity 'c2002e72349371acb228841462af8f0fa1f9eb3c224bb49567acd009b85eeb43'. A reducer with a similar name exists: 'sign_up'`
  - possible fix: Have `spacetimedb-cli call`/`describe` accept and normalize the TS-declared (camelCase) reducer name, or print the schema's full reducer name list up front instead of only suggesting on failure.
- **Fresh backend scaffold fails to publish because typescript isn't installed** *(CLI/publish)*
  - cost: First publish attempt failed with a tsc-not-found message in two independent fresh-build sessions, forcing an extra `npm install typescript` (plus a package.json edit) before the build could proceed
  - evidence: `tsc not found in node_modules. Make sure you have the 'typescript' package as a dev-dependency and that your dependencie`
  - possible fix: Have `spacetimedb-cli generate`/module scaffolding include `typescript` as a devDependency in the generated backend package.json by default, or bundle a compiler with the publish tool so a stock TS module compiles without a manual npm install step.

---
**Follow-up 2026-08-07:** the tsc-not-found scaffold gap is real in the product,
not just in benchmark builds — `basic-react`, `basic-typescript` and
`quickstart-chat-typescript` templates ship a server package.json with no
`typescript` devDependency while seventeen sibling TS templates carry it.
Validated locally by patching `crates/cli/.templates` (vendored, gitignored —
upstream fix belongs in the templates source repo). Benchmark spec-side fix
landed in backends/spacetime.md.

---

