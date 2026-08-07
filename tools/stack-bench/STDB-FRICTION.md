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

## 2026-08-07 18:43 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 0/0, $10.1812, 0 fix round(s)

**Tokens** (from the CLI's own usage, 6 session(s), 810 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 103,406,541 | 98% |
| cache write | 1,731,376 | 2% |
| fresh input | 1,620 | 0% |
| output | 895,635 | — |

**Where it got stuck** — 23 build failure(s) of 473 tool calls (plus 4 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | TS2554: Expected 1 arguments, but got 0. |
| 2 | Error: Response text: Your cart is empty |
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/test-wir |
| 1 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey< |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2046 ms: Could not connect to server |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2035 ms: Could not connect to server |

By SpacetimeDB surface: server API (schema / reducers) (12), other (9), client SDK (subscriptions) (2)

**Re-read** — 11 read(s) of generated bindings

- 11x `client/src/App.tsx`
- 3x `client/src/App.tsx`
- 2x `client/src/App.tsx`
- 2x `client/src/index.css`
- 2x `spacetimedb/src/index.ts`
- 1x `spacetime-ecom-run0-20260807125914/app/check-hooks.sh`

---
**Behavioural review** — 4 finding(s) with verified evidence

- **spacetime publish fails first-try when typescript devDependency is missing** *(CLI/publish)*
  - cost: Hit identically on all three independent fresh-build sessions (85d6d835, 9f2e3a29, c4920163) — each time the first `publish` attempt failed, requiring an extra `npm install typescript` + package.json edit cycle before a second publish attempt succeeded.
  - evidence: `tsc not found in node_modules. Make sure you have the 'typescript' package as a dev-dependency and that your dependencies are installed.`
  - possible fix: Have `spacetime publish`/`spacetime init` scaffold typescript as a devDependency automatically, or auto-install it when missing, instead of failing with a manual-fix message on every fresh project.
- **View/reducer ctx typing (AnyCtx vs ViewCtx vs ReducerCtx) undiscoverable without reading SDK source** *(generated bindings)*
  - cost: Two independent fresh-build sessions hit the same category of TS2339 errors accessing ctx.sender/ctx.db inside views, each requiring multiple grep/sed dives into bindings-typescript's views.ts, db_view.ts, and schema.ts to work out the right context type.
  - evidence: `src/index.ts(48,57): error TS2339: Property 'sender' does not exist on type 'AnyCtx'. Property 'sender' does not exist on type 'Readonly<{ db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<`
  - possible fix: Document the distinct ctx types (AnonymousViewCtx/ViewCtx/ReducerCtx) and when each applies directly in the TS bindings API docs, with a worked view example, instead of relying on the source being greppable.
- **Zero-arg reducers still require an explicit {} argument in generated client bindings** *(generated bindings)*
  - cost: Two independent fresh-build sessions wrote `conn.reducers.signout()` / `conn.reducers.checkout()` with no arguments (the natural call for a param-less reducer), both failed type-check with TS2554 and needed an edit to pass `({})`.
  - evidence: `src/App.tsx(208,19): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate a default/optional empty-object parameter for reducers with no declared params so `conn.reducers.foo()` type-checks without requiring an explicit `{}`.
- **CLI `call` requires snake_case reducer name while TS API exposes camelCase, with no doc callout** *(CLI/publish)*
  - cost: One failed `spacetimedb-cli call` invocation using the camelCase name matching the TS reducer definition before retrying with the CLI's snake_case wire name.
  - evidence: `Error: No such reducer OR procedure 'signUp' for database 'stackbench-ecom-run0' resolving to identity 'c2002e72349371acb228841462af8f0fa1f9eb3c224bb49567acd009b85eeb43'. A reducer with a similar name exists: 'sign_up'`
  - possible fix: Either have the CLI resolve the equivalent-case reducer name automatically (it already computes the suggestion), or document the camelCase(TS)/snake_case(CLI/SQL) naming convention explicitly.

---
## 2026-08-07 20:52 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 47/49, $22.5091, 3 fix round(s)

**Tokens** (from the CLI's own usage, 8 session(s), 1271 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 168,750,393 | 98% |
| cache write | 2,585,353 | 2% |
| fresh input | 2,542 | 0% |
| output | 1,349,989 | — |

**Where it got stuck** — 39 build failure(s) of 706 tool calls (plus 14 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 4 | name: 'Error' |
| 3 | TS2554: Expected 1 arguments, but got 0. |
| 3 | Exit code 1 Microsoft Windows [Version 10.0.26200.8875] (c) Microsoft Corporation. All rights reser |
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | Error: No such reducer OR procedure `signUp` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 2 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 2 | Error: Response text: Your cart is empty |
| 2 | spacetimedb: error: unrecognized subcommand 'identity' |

By SpacetimeDB surface: other (19), server API (schema / reducers) (16), client SDK (subscriptions) (2), generated bindings (1), CLI / publish (1)

**Re-read** — 2 read(s) of generated bindings

- 11x `client/src/App.tsx`
- 10x `client/src/App.tsx`
- 5x `client/src/index.css`
- 4x `spacetimedb/src/schema.ts`
- 3x `client/src/App.tsx`
- 3x `spacetimedb/src/index.ts`

---
**Behavioural review** — 6 finding(s) with verified evidence

- **Generated no-arg reducer bindings require an empty object argument** *(generated bindings)*
  - cost: Independently broke the client build in at least 3 separate runs (App.tsx TS2554 errors), each requiring a Read of the generated reducer file to discover the fix and an Edit to add `({})`/`{}` before the build would pass
  - evidence: `src/App.tsx(208,19): error TS2554: Expected 1 arguments, but got 0. src/App.tsx(240,8): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate a default/optional empty-object parameter (or overload) for reducers with no fields so `conn.reducers.checkout()` type-checks without callers needing to pass `{}`
- **ViewCtx silently lacks `ctx.sender`, breaking helper functions shared with ReducerCtx** *(server API)*
  - cost: Two separate runs wrote a helper (e.g. findAccountBySession) typed against a broad `AnyCtx = ReducerCtx | ViewCtx`, published, and only discovered at publish-time TS compile that ViewCtx doesn't expose `sender`, forcing a rewrite of the ctx typing and a second publish attempt
  - evidence: `Property 'sender' does not exist on type 'AnyCtx'.`
  - possible fix: Either give ViewCtx a `sender` field (identity of the caller is normally available to anonymous views too) or clearly document/name the type so it's obvious at authoring time that ViewCtx is not a drop-in ReducerCtx substitute
- **CLI `call` requires snake_case reducer names that don't match the camelCase TS source** *(CLI/publish)*
  - cost: Hit independently in at least two separate sessions: called `signUp` (matching the TS export name) via `spacetimedb-cli call`, got a hard error, had to retry with the wire name `sign_up`
  - evidence: `Error: No such reducer OR procedure 'signUp' for database 'stackbench-ecom-run0' resolving to identity 'c2002e72349371acb228841462af8f0fa1f9eb3c224bb49567acd009b85eeb43'. A reducer with a similar name exists: 'sign_up'`
  - possible fix: Have `spacetimedb-cli call` accept either casing (or print the case-conversion rule prominently in --help) since the CLI already detects the correct name as a near-match suggestion
- **Publish always prints a spurious "tsc not found" warning even on successful builds** *(CLI/publish)*
  - cost: Appeared on essentially every single `publish`/`generate` invocation across all sessions even though the build succeeded seconds later and typescript was correctly installed, forcing repeated manual judgment calls about whether the warning was real
  - evidence: `tsc not found in node_modules. Make sure you have the 'typescript' package as a dev-dependency and that your dependencies are installed. Build finished successfully.`
  - possible fix: Fix the tsc-detection check (it's producing a false positive even when `typescript` is a resolvable dependency and the build itself invokes tsc successfully) or remove the warning when the subsequent build succeeds
- **No non-interactive flag to confirm a destructive republish; must pipe `echo y`** *(CLI/publish)*
  - cost: Every republish after a schema change to an already-populated local DB required detecting the interactive y/n prompt and wrapping the command in `echo y | ...`, done ad hoc every time rather than via a documented flag
  - evidence: `echo y | D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/`
  - possible fix: Add a `--yes`/`-y` (or `--delete-data`) flag to `spacetimedb-cli publish` so scripted/automated workflows don't need to pipe stdin to bypass the confirmation prompt
- **CLI suggests a nonexistent `identity` subcommand** *(CLI/publish)*
  - cost: Two separate sessions independently tried `spacetimedb-cli identity list`/`identity --help` to inspect/manage identities (a reasonable guess given `--identity`-style flags elsewhere) and got an unhelpful nearest-match suggestion pointing at the unrelated `init` command
  - evidence: `error: unrecognized subcommand 'identity' tip: a similar subcommand exists: 'init'`
  - possible fix: Either add an `identity` subcommand (list/show configured identities) or make the fuzzy-match suggestion smarter so it doesn't point at an unrelated command

---
## 2026-08-07 — the fix-loop cost, measured against two controls

First cross-backend measurement of WHY iteration costs more, from the
ecommerce L1 proving round (all three audited clean, same spec, same pinned
setup). Fix-round tool calls by kind:

| | db query | ad-hoc scripts | http probes | file inspects | edits | deploy cycles |
|---|---:|---:|---:|---:|---:|---:|
| spacetime | 23 | 50 | 38 | **240** | 120 | **17** |
| postgres | 4 | 67 | 33 | 62 | 98 | **0** |
| mongodb | 2 | 56 | 41 | 121 | 158 | **0** |

Cost split: spacetime's ONE-SHOT build was the cheapest of the three ($6.93 vs
$8.39 / $12.42, 109 turns vs 136 / 191). Its total was the highest ($22.51)
because **69% of it was the fix loop** ($15.58 over 3 rounds). So the expense
is not building on SpacetimeDB — it is iterating on it.

Mechanism: postgres and mongo hot-reload (Express under tsx), so an edit is
live immediately and zero deploy commands appear in their entire fix loop.
The SpacetimeDB loop as we documented it was edit -> publish -> generate ->
re-read regenerated bindings, 17 times. The 240 file inspections (121 client,
29 module_bindings, 28 schema, 20 server) are largely re-learning what
`generate` just changed, and a fix typically spans three coupled files where a
postgres fix touches one.

**CONFOUND, and it is ours: `spacetime dev` already does this.** It watches the
module and auto-rebuilds, auto-publishes and auto-regenerates bindings on save
— the exact hot-reload equivalent — and `backends/spacetime.md` never mentioned
it, prescribing the manual publish/generate loop instead. The numbers above
therefore measure the manual loop we documented, not the loop the product
offers. Docs fixed 2026-08-07 (both prescribed and minimal packs); this
measurement must be REDONE before any of it is quoted.

What survives the confound regardless: bindings regeneration is a re-learning
cost with no analogue in the other stacks (dev mode automates the regenerate,
it does not remove the need to re-read), and a fix spanning schema + server +
client is structural. What does NOT survive: any claim that the deploy cycle
itself is unavoidable overhead.

---

