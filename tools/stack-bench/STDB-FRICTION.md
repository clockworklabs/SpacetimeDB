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
   - evidence: `// Not a cryptographic hash: SpacetimeDB modules run deterministically and have` / `// no access to system crypto. Good enough to avoid storing plaintext passwords.` (archive/pre-v1/results/spacetime-run0/app/backend/spacetimedb/src/index.ts:22)
3. Shipped PLAINTEXT password storage and comparison:
   - evidence: `if (!acc || acc.password !== password) throw new SenderError('Invalid username or password');` (archive/pre-v1/results/spacetime-ecom-run0/source/backend/spacetimedb/src/index.ts:71)

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

## 2026-08-07 22:06 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 48/48, $10.2855, 0 fix round(s)

**Tokens** (from the CLI's own usage, 9 session(s), 1551 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 212,442,633 | 99% |
| cache write | 3,054,086 | 1% |
| fresh input | 3,180 | 0% |
| output | 1,601,566 | — |

**Where it got stuck** — 44 build failure(s) of 862 tool calls (plus 18 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 4 | name: 'Error' |
| 3 | TS2554: Expected 1 arguments, but got 0. |
| 3 | Error: Aggregate expressions must have column aliases |
| 3 | Exit code 1 Microsoft Windows [Version 10.0.26200.8875] (c) Microsoft Corporation. All rights reser |
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | Error: No such reducer OR procedure `signUp` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 2 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 2 | Error: Response text: Your cart is empty |

By SpacetimeDB surface: other (19), server API (schema / reducers) (18), CLI / publish (3), client SDK (subscriptions) (2), generated bindings (2)

**Re-read** — 2 read(s) of generated bindings

- 11x `client/src/App.tsx`
- 10x `client/src/App.tsx`
- 5x `client/src/index.css`
- 4x `spacetimedb/src/schema.ts`
- 3x `client/src/App.tsx`
- 3x `spacetimedb/src/index.ts`

---
**Behavioural review** — 5 finding(s) with verified evidence

- **Zero-arg reducers still require an empty-object argument** *(generated bindings)*
  - cost: Same TS2554 compile error ('Expected 1 arguments, but got 0') was hit and manually fixed in at least 3 independent build sessions on calls like checkout()/signOut()/signOut()
  - evidence: `src/App.tsx(208,19): error TS2554: Expected 1 arguments, but got 0. src/App.tsx(240,8): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate a zero-parameter overload (or default the arg to {}) for reducers with no fields so client code can call conn.reducers.foo() without an empty object.
- **CLI `call` needs snake_case reducer names while generated bindings use camelCase** *(CLI/publish)*
  - cost: Agent guessed the camelCase reducer name from the TS bindings, got a rejected call, and had to retry with the snake_case wire name — repeated across at least 3 separate sessions (signUp/sign_up, backofficeSetStock/backoffice_set_stock)
  - evidence: `Error: No such reducer OR procedure 'signUp' for database 'stackbench-ecom-run0' resolving to identity 'c2002e72349371acb228841462af8f0fa1f9eb3c224bb49567acd009b85eeb43'. A reducer with a similar name exists: 'sign_up'`
  - possible fix: Make `spacetime call` accept either naming convention, or have the generated TS bindings expose the wire (snake_case) name alongside the camelCase one so scripts built from the bindings don't have to guess.
- **View context types don't expose `ctx.sender`, breaking helpers shared with reducers** *(server API)*
  - cost: Two independent build sessions hit the same class of type error while writing an isAdmin/session-lookup helper meant to work across ReducerCtx and view contexts, each requiring a manual signature rewrite before publish would type-check
  - evidence: `src/index.ts(48,57): error TS2339: Property 'sender' does not exist on type 'AnyCtx'. Property 'sender' does not exist on type 'Readonly<{ db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<`
  - possible fix: Give ReducerCtx, ViewCtx, and AnonymousViewCtx a shared base type that includes `sender` (or document explicitly that views can't see the caller identity) so a single ctx-generic helper function type-checks everywhere it's legitimately usable.
- **CLI has no `identity` subcommand despite it being a natural guess** *(CLI/publish)*
  - cost: Agent tried `spacetimedb-cli identity ...` expecting standard CLI conventions and hit an unrecognized-subcommand error in two separate sessions before falling back to a workaround with separate root-dirs
  - evidence: `error: unrecognized subcommand 'identity' tip: a similar subcommand exists: 'init'`
  - possible fix: Either add an `identity` command (list/new/etc., common in other db CLIs) or surface identity management more discoverably from `--help` so agents don't repeatedly probe for it.
- **Publish always prints a self-contradicting "tsc not found" warning** *(CLI/publish)*
  - cost: Every single `spacetime publish`/`generate` call across all 5 sessions printed this warning even when the build succeeded immediately afterward, undermining trust in the CLI's own diagnostics
  - evidence: `tsc not found in node_modules. Make sure you have the 'typescript' package as a dev-dependency and that your dependencies are installed. Build finished successfully.`
  - possible fix: Only emit the tsc-not-found warning when it actually affects the build outcome, or reword it to reflect that a bundled compiler was used as fallback, since printing it unconditionally trains users/agents to ignore CLI warnings.

---
## 2026-08-08 00:10 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 40/48, $7.0403, 0 fix round(s)

**Tokens** (from the CLI's own usage, 7 session(s), 1421 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 201,137,051 | 99% |
| cache write | 2,536,429 | 1% |
| fresh input | 2,920 | 0% |
| output | 1,369,879 | — |

**Where it got stuck** — 39 build failure(s) of 790 tool calls (plus 19 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 4 | name: 'Error' |
| 3 | TS2554: Expected 1 arguments, but got 0. |
| 3 | Exit code 1 Microsoft Windows [Version 10.0.26200.8875] (c) Microsoft Corporation. All rights reser |
| 3 | Error: Aggregate expressions must have column aliases |
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | Compound command changes working directory (Set-Location/Push-Location/Pop-Location/New-PSDrive) — relative paths cannot |
| 2 | Error: `--module-path` cannot be used when `spacetime.json` contains publish targets. Remove `--module-path` or run with |
| 1 | Exit code 28 --- ---lint--- curl: (28) Operation timed out after 3004 milliseconds with 0 bytes rece |

By SpacetimeDB surface: other (16), server API (schema / reducers) (15), CLI / publish (5), generated bindings (2), client SDK (subscriptions) (1)

**Re-read** — 0 read(s) of generated bindings

- 11x `client/src/App.tsx`
- 10x `client/src/App.tsx`
- 5x `client/src/index.css`
- 4x `spacetimedb/src/schema.ts`
- 3x `spacetimedb/src/index.ts`
- 3x `client/src/App.tsx`

---
**Behavioural review** — 7 finding(s) with verified evidence

- **No-arg reducers still require an explicit empty-object argument** *(generated bindings)*
  - cost: Hit independently in two separate build sessions; each time cost a failed tsc compile, a source dig into checkout_reducer.ts to confirm the signature, and an App.tsx edit to add `{}`/`{}` to calls like `conn.reducers.checkout()` and `conn.reducers.signOut()`.
  - evidence: `src/App.tsx(453,27): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Give reducers with no declared parameters an optional/defaulted params object in the generated TS signature so `reducers.checkout()` type-checks without a required empty `{}` argument.
- **Reducer/view context types are too narrow for shared helper functions** *(generated bindings)*
  - cost: In two independent sessions the model wrote a shared ctx-typed helper (e.g. requireAccountId/isCallerAdmin), got a publish-time TS error because ViewCtx/AnonymousViewCtx lack fields ReducerCtx has (like `sender`), then had to grep through bindings-typescript's views.ts/schema.ts source to discover the right narrower type to use — 10+ exploratory reads per occurrence.
  - evidence: `src/index.ts(48,57): error TS2339: Property 'sender' does not exist on type 'AnyCtx'.`
  - possible fix: Document (or unify) the ReducerCtx/ViewCtx/AnonymousViewCtx type hierarchy so a helper function author doesn't have to read internal source to learn which fields are available on which context type.
- **`spacetime dev` fights with a `spacetime.json` that has publish targets** *(CLI/publish)*
  - cost: Three separate `spacetime dev` launch attempts failed/misbehaved: first it wrote generated bindings to the wrong directory (`app/src/module_bindings` instead of `app/client/src/module_bindings`), then two more attempts failed outright until the agent deleted the auto-created spacetime.json/spacetime.local.json entirely.
  - evidence: `Error: '--module-path' cannot be used when 'spacetime.json' contains publish targets. Remove '--module-path' or run with`
  - possible fix: Make `spacetime dev`'s bindings output path and its interaction with an existing spacetime.json config self-consistent, and give a clearer remediation message (or auto-fix) instead of requiring the user to delete the config file.
- **`spacetime dev` defaults bindings output to the wrong directory** *(CLI/publish)*
  - cost: First `spacetime dev` run silently generated a stray `app/src/module_bindings` tree instead of the client's actual `app/client/src/module_bindings`, requiring the agent to `rm -rf src` and restart with explicit paths.
  - evidence: `The 'dev' watcher generated bindings into the wrong location ('app/src/module_bindings' instead of 'app/client/src/module_bindings').`
  - possible fix: Have `spacetime dev` infer or require an explicit client bindings path up front rather than defaulting to a location next to the module.
- **Reducer names are camelCase in TS bindings but snake_case over the CLI/wire** *(CLI/publish)*
  - cost: A backoffice script built against the camelCase reducer name failed at runtime; the agent had to inspect generated binding filenames to discover the actual wire name and rewrite the script to call `backoffice_set_stock` via the CLI instead.
  - evidence: `The reducer wire names are snake_case ('backoffice_set_stock'), not camelCase.`
  - possible fix: Keep reducer naming consistent between the TS client API and the CLI/`spacetime call` wire protocol, or document the case-conversion explicitly so scripts calling both surfaces don't need to discover it by trial and error.
- **spacetimedb-cli sql/call print an UNSTABLE warning to stderr on every invocation** *(CLI/publish)*
  - cost: The noisy warning appears on essentially every `sql` and `call` invocation throughout every session, and had to be explicitly suppressed in the backoffice script for clean output.
  - evidence: `Now let's suppress the CLI's stderr noise in the script for cleanliness.`
  - possible fix: Print the UNSTABLE warning once per CLI session (or suppress in non-interactive/piped mode) rather than on every single sql/call invocation.
- **SQL surface rejects ORDER BY** *(server API)*
  - cost: A verification query against `order_item` failed outright because ORDER BY isn't supported by the SQL endpoint, forcing the agent to fall back to unordered queries for debugging.
  - evidence: `Error: Unsupported: SELECT * FROM order_item ORDER BY`
  - possible fix: Support ORDER BY in the SQL query surface, or document the unsupported SQL subset so agents don't burn a round-trip discovering it.

---
## 2026-08-08 01:35 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 49/49, $8.5673, 0 fix round(s)

**Tokens** (from the CLI's own usage, 6 session(s), 1180 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 166,495,033 | 99% |
| cache write | 2,167,300 | 1% |
| fresh input | 2,438 | 0% |
| output | 1,126,251 | — |

**Where it got stuck** — 39 build failure(s) of 643 tool calls (plus 17 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 4 | name: 'Error' |
| 3 | Exit code 1 Microsoft Windows [Version 10.0.26200.8875] (c) Microsoft Corporation. All rights reser |
| 3 | Error: Aggregate expressions must have column aliases |
| 2 | Compound command changes working directory (Set-Location/Push-Location/Pop-Location/New-PSDrive) — relative paths cannot |
| 2 | Error: `--module-path` cannot be used when `spacetime.json` contains publish targets. Remove `--module-path` or run with |
| 1 | spacetimedb: error: unrecognized subcommand 'identity' |
| 1 | Error: No such reducer OR procedure `signUp` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 1 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-ecom-run0` resolving to identity `…`. |

By SpacetimeDB surface: other (16), server API (schema / reducers) (13), CLI / publish (7), generated bindings (2), client SDK (subscriptions) (1)

**Re-read** — 3 read(s) of generated bindings

- 9x `client/src/App.tsx`
- 5x `client/src/App.tsx`
- 4x `client/src/index.css`
- 3x `spacetimedb/src/schema.ts`
- 3x `spacetimedb/src/index.ts`
- 3x `client/src/App.tsx`

---
**Behavioural review** — 6 finding(s) with verified evidence

- **spacetime dev writes bindings to the wrong default path** *(CLI/publish)*
  - cost: Recurred in two independent fresh-build sessions; each time the agent had to kill the running dev process, delete spacetime.json/spacetime.local.json/.env.local, and restart with explicit path flags before bindings landed in client/src/module_bindings
  - evidence: `The 'dev' watcher generated bindings into the wrong location ('app/src/module_bindings' instead of 'app/client/src/module_bindings').`
  - possible fix: Make `spacetime dev` default the module-bindings output path to the client dir declared in the project layout (or prompt/ask), instead of writing to <project-root>/src/module_bindings by default.
- **--module-path flag silently conflicts with spacetime.json publish targets** *(CLI/publish)*
  - cost: Two failed `spacetime dev` invocations in a row (same session), each requiring inspection of the opaque error and manual deletion of config files before a third invocation succeeded
  - evidence: `Error: '--module-path' cannot be used when 'spacetime.json' contains publish targets. Remove '--module-path' or run with`
  - possible fix: Have `spacetime dev` auto-detect and reconcile with an existing spacetime.json instead of hard-erroring, or clearly state in the error which of the two (flag vs config) to keep and why they conflict.
- **No-arg reducers still require passing an empty object at the call site** *(generated bindings)*
  - cost: Independent TypeScript compile failures in a fresh build (two call sites: signOut(), checkout()) that had to be diagnosed and fixed after the fact — the reducer's zero-argument name gives no hint that a call needs `({})`
  - evidence: `src/App.tsx(158,19): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate an overload (or default empty-object parameter) for reducers with no fields so `reducers.signOut()` type-checks without requiring `reducers.signOut({})`.
- **Client SDK requires manual token persistence with no built-in helper or error signal** *(client SDK)*
  - cost: An entire session of investigation (reading module_bindings, digging into node_modules/spacetimedb/src/react/SpacetimeDBProvider.ts) was spent tracing 6 separately-reported bugs back to one root cause: the app never read `token` off useSpacetimeDB() and persisted it, so every reload silently reconnected as a new anonymous identity with no error surfaced anywhere
  - evidence: `Confirmed: 'useSpacetimeDB()' exposes 'token', but App.tsx never reads or persists it. That's the missing piece causing session loss on reload.`
  - possible fix: Have the connection builder/provider persist and restore the auth token to localStorage by default (opt-out), or surface a warning when a connection is built without withToken() and no prior token exists.
- **SQL surface rejects ORDER BY and unaliased/unsupported aggregates** *(server API)*
  - cost: Ad hoc `spacetime sql` queries used while writing debug/backoffice scripts failed outright on ORDER BY and on COUNT()/aggregate expressions without aliases, forcing query rewrites via trial and error across multiple sessions
  - evidence: `Error: Unsupported: SELECT * FROM order_item ORDER BY`
  - possible fix: Support ORDER BY and standard aggregate functions in the CLI SQL surface, or document the supported SQL subset so agents don't have to discover the gaps by trial and error.
- **Generated row type constraint expects Infer<T>, not InferSchema<T>** *(generated bindings)*
  - cost: A publish attempt failed on a type error, requiring the agent to dig through crates/bindings-typescript dist .d.ts files to discover that `Infer<T>` (not `InferSchema<T>`, which is used elsewhere for the whole schema) is the correct generic for a single row/view type
  - evidence: `Type 'RowBuilder<{ itemId: U64ColumnBuilder<{ isPrimaryKey: true; }>; name: StringBuilder; price: F64Builder; description: StringBuilder; stock: U64Builder; purchaseCount: U64Builder; avgRating: F64Builder; reviewCount: `
  - possible fix: Export and document Infer<T> alongside InferSchema<T> with clear guidance on when each applies (whole schema vs single row/view type), and/or give the two names less confusable naming.

---
## 2026-08-08 14:02 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 48/48, $7.6908, 0 fix round(s)

**Tokens** (from the CLI's own usage, 1 session(s), 227 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 32,495,259 | 99% |
| cache write | 341,819 | 1% |
| fresh input | 454 | 0% |
| output | 205,080 | — |

**Where it got stuck** — 5 build failure(s) of 133 tool calls (plus 1 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 2 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey<MultiplePrimaryKeyColumns<Ret>> |
| 1 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 1 | TS2554: Expected 1 arguments, but got 0. |

By SpacetimeDB surface: server API (schema / reducers) (4), other (1)

**Re-read** — 1 read(s) of generated bindings

- 6x `src/components/ItemDetail.tsx`
- 4x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`
- 2x `client/src/index.css`
- 1x `spacetime-ecom-run0-20260808133350/app/check-hooks.sh`
- 1x `crates/bindings-typescript/package.json`

---
**Behavioural review** — 3 finding(s) with verified evidence

- **Zero-argument reducers still require passing an empty object from the client** *(client SDK)*
  - cost: two separate TS2554 compile errors in App.tsx, requiring source inspection and two edits before the client typechecked
  - evidence: `src/App.tsx(124,20): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Let generated reducer methods accept zero arguments when the reducer schema has no fields, instead of requiring an explicit {}
- **ReducerCtx/ViewCtx db typing errors are inscrutable and undocumented, forcing repeated dives into node_modules source** *(server API)*
  - cost: two separate compile-error cycles (one for a helper function's db param type, one for ctx.db typing inside a view) each requiring multiple greps/reads of the SDK's dist/*.d.ts and .ts source files to figure out the correct exported type name
  - evidence: `src/index.ts(174,32): error TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<{ accountId: U64ColumnBuilder<{ isPrimaryKey: true; isAu`
  - possible fix: Export a clearly-named, documented type (e.g. AnonymousViewCtx['db']) for 'the readonly db view type' so module authors don't have to reverse-engineer it from generic-heavy inferred types, and add a docs example for typing helper functions that take ctx.db as a parameter
- **View reducers must return an object-wrapped type, not a bare scalar, but this isn't surfaced until a compile error** *(server API)*
  - cost: required editing the adminRevenue view definition after the compiler rejected a bare t.f64() return
  - evidence: `Now fix 'adminRevenue' to return an object wrapped view type instead of a bare 't.f64()'.`
  - possible fix: Either allow scalar view return types directly, or document the object-wrapping requirement for view return schemas alongside the t.row()/t.f64() API reference

---
## 2026-08-08 17:56 — spacetime-ecom-run0 (ecommerce) L2

**Result:** 91/102, $16.0961, 0 fix round(s)

**Tokens** (from the CLI's own usage, 3 session(s), 658 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 100,601,753 | 99% |
| cache write | 1,252,576 | 1% |
| fresh input | 1,316 | 0% |
| output | 638,629 | — |

**Where it got stuck** — 20 build failure(s) of 376 tool calls (plus 7 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 3 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 3 | TS2554: Expected 1 arguments, but got 0. |
| 2 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 2 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey<MultiplePrimaryKeyColumns<Ret>> |
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/test-app |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2024 ms: Could not connect to server |
| 1 | Exit code 1 { "compilerOptions": { "target": "ES2020", "useDefineForClassFields": true, |

By SpacetimeDB surface: server API (schema / reducers) (12), other (8)

**Re-read** — 2 read(s) of generated bindings

- 24x `client/src/App.tsx`
- 6x `src/components/ItemDetail.tsx`
- 4x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`
- 2x `client/src/index.css`
- 2x `client/src/index.css`

---
**Behavioural review** — 5 finding(s) with verified evidence

- **Reducers with zero parameters still require passing an empty object** *(generated bindings)*
  - cost: Same TS2554 compile error recurred independently in two separate fresh build sessions (client/src/App.tsx call sites for signOut/checkout-style reducers), each needing a read+edit cycle to discover `{}` must be passed even though the reducer takes no arguments
  - evidence: `src/App.tsx(124,20): error TS2554: Expected 1 arguments, but got 0. src/App.tsx(154,20): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate reducer call signatures with zero fields as truly zero-arg functions (optional/overloaded), or document this requirement prominently in the TypeScript SDK generation guide
- **Shared helper functions can't be typed against both ReducerCtx and ViewCtx without a huge, unreadable generic type error** *(generated bindings)*
  - cost: Publish failed twice across independent sessions with the identical multi-hundred-character nested-generic TS2345 error; each time the model had to grep into node_modules/spacetimedb dist .d.ts files to reverse-engineer that ViewCtx must be imported and unioned separately from ReducerCtx
  - evidence: `src/index.ts(174,32): error TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<CoerceRow<{ accountId: U64ColumnBuilder<{ isPrimaryKey: true; isAu`
  - possible fix: Export a unified/wider context type (or provide a documented helper pattern) so functions shared between reducers and views don't require manual union typing; truncate/simplify TS error output for these generated types
- **Reordering table fields in schema.ts triggers 'requires a manual migration' even with no semantic change** *(server API)*
  - cost: Publish rejected twice across independent sessions purely because table field order in the schema source differed from the previously published version, forcing a destructive --delete-data republish
  - evidence: `Reordering table order_line requires a manual migration`
  - possible fix: Don't treat column reordering in the schema source as a breaking change requiring manual migration when the column set and types are unchanged
- **Views cannot return a bare scalar type; must be wrapped in an object/row type** *(server API)*
  - cost: An extra edit cycle after the first publish attempt to change adminRevenue from `t.f64()` to a wrapped object row type — not caught until a full build/publish round-trip
  - evidence: `Now fix 'adminRevenue' to return an object wrapped view type instead of a bare 't.f64()'.`
  - possible fix: Either allow views to return primitive/scalar types directly, or document clearly that view outputs must be t.object()/t.row() wrapped
- **CLI reducer invocation name (snake_case) is undiscoverable from the camelCase name shown in client code** *(CLI/publish)*
  - cost: Had to grep generated bindings' types/reducers.ts to find the wire-format reducer name before it could call it via the CLI for verification
  - evidence: `The wire name is 'backoffice_set_stock' (snake_case). Now let's write the backoffice script.`
  - possible fix: Have `spacetime call --help` or generated bindings surface the CLI-invocable (snake_case) reducer name alongside the camelCase TS name, or make `spacetime call` accept either casing

---
## 2026-08-08 18:20 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 0/0, $8.2445, 0 fix round(s)

**Tokens** (from the CLI's own usage, 4 session(s), 885 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 134,572,603 | 99% |
| cache write | 1,700,881 | 1% |
| fresh input | 1,770 | 0% |
| output | 874,669 | — |

**Where it got stuck** — 27 build failure(s) of 513 tool calls (plus 7 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 3 | TS2345: Argument of type 'Readonly<{ sender: Identity; db: ReadonlyDbView<SchemaDefForEntries<{ readonly account: TableSchema<Co |
| 3 | spacetimedb: Aborting because publishing would require manual migration or deletion of data and --delete-data was not specified. |
| 3 | TS2554: Expected 1 arguments, but got 0. |
| 2 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-ecom-run0` resolving to identity `…`. |
| 1 | spacetimedb: error: ERROR_view_return_type_can_have_at_most_one_primaryKey<MultiplePrimaryKeyColumns<Ret>> |
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/test-app |
| 1 | curl: (7) Failed to connect to 127.0.0.1 port 6473 after 2024 ms: Could not connect to server |
| 1 | Exit code 1 { "compilerOptions": { "target": "ES2020", "useDefineForClassFields": true, |

By SpacetimeDB surface: server API (schema / reducers) (16), other (9), CLI / publish (1), generated bindings (1)

**Re-read** — 8 read(s) of generated bindings

- 24x `client/src/App.tsx`
- 6x `src/components/ItemDetail.tsx`
- 5x `client/src/App.tsx`
- 4x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`
- 2x `client/src/index.css`

---
**Behavioural review** — 4 finding(s) with verified evidence, 1 discarded as unverifiable

- **View functions typed against ReducerCtx, not ViewCtx — compile fails on publish** *(generated bindings)*
  - cost: Two independent fresh sessions both hit the same TS2345 compile error on publish, then had to grep through node_modules/spacetimedb dist files to discover the correct exported type (ViewCtx) instead of getting a clear compiler hint or example in scaffolded code.
  - evidence: `SAYS: 'ViewCtx<S>' is exported from 'spacetimedb/server'. Let me update the helper functions to accept both context types.`
  - possible fix: Export a shared context type (or overload) that reducer-helper functions can use for both reducers and views, and surface it in the docs/template so views don't require reverse-engineering dist files to type-check.
- **Zero-argument reducers still require passing {} from the client** *(client SDK)*
  - cost: Recurred across at least two separate sessions building the same app; each time it caused a TS2554 compile failure ('Expected 1 arguments, but got 0') on client code calling no-arg reducers like signOut()/checkout(), forcing a follow-up edit pass across the file.
  - evidence: `SAYS: Need to pass '{}' for zero-arg reducers.`
  - possible fix: Generate an overload (or default empty-object parameter) for zero-argument reducers so callers can invoke them as reducerName() without a required empty-object argument.
- **CLI 'generate' subcommand rejects -s server flag accepted by other subcommands** *(CLI/publish)*
  - cost: One failed CLI invocation before discovering that -s (used successfully for sql and other commands) isn't accepted by generate, requiring a different flag/syntax.
  - evidence: `-> ok: error: unexpected argument '-s' found tip: to pass '-s' as a value, use '-- -s' Usage: generate [DATABASE] --lang <LANG>`
  - possible fix: Standardize the server-selection flag (e.g. -s/--server) across all spacetimedb-cli subcommands so scripts don't need per-command flag lookups.
- **Views cannot return a bare scalar type — must be wrapped in an object** *(server API)*
  - cost: Model wrote a view returning t.f64() directly, which failed to type-check against the reducer/view context, requiring a rewrite to an object-wrapped return type before publish would succeed.
  - evidence: `SAYS: Now fix 'adminRevenue' to return an object wrapped view type instead of a bare 't.f64()'.`
  - possible fix: Either support scalar-returning views directly, or document explicitly (with an example) that view functions must return an object/row type, not a bare primitive builder.

<sub>Discarded (evidence not found verbatim): 'server add' requires interactive fingerprint confirmation with no non-interactive bypass</sub>

---
## 2026-08-09 01:44 — spacetime-run0 (chat) L1

**Result:** 0/49, $16.3767, 0 fix round(s)

**Tokens** (from the CLI's own usage, 1 session(s), 346 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 70,636,444 | 99% |
| cache write | 667,101 | 1% |
| fresh input | 692 | 0% |
| output | 481,799 | — |

**Where it got stuck** — 12 build failure(s) of 183 tool calls (plus 3 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 2 | Error: Invalid arguments provided for reducer `sign_up` for database `stackbench-run0` resolving to identity `…`. |
| 1 | Exit code 1 D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/templates/basic-react/spacetimedb/d |
| 1 | TS2322: Type '…' is not assignable to type '…'. |
| 1 | Error: Unsupported expression: message_id IN (SELECT id FROM message WHERE author_username = 'nonexistent_test_user_xyz' |
| 1 | Error: Unsupported expression: message_id IN (1, 2, 3) |
| 1 | TS2352: Conversion of type 'readonly { id: bigint; kind: string; name: string \| undefined; ownerUsername: string \| undefined; cr |
| 1 | [31merror when starting dev server: |
| 1 | New-Object instantiates .NET type 'System.IO.StreamReader' outside the ConstrainedLanguage allowlist |

By SpacetimeDB surface: server API (schema / reducers) (7), other (5)

**Re-read** — 0 read(s) of generated bindings

- 1x `spacetimedb/src/index.ts`
- 1x `spacetimedb/src/index.ts`
- 1x `client/src/ChatApp.tsx`
- 1x `75e4cef4-2c2b-4282-a6a2-b426a35f1cef/scratchpad/after-signup.png`
- 1x `client/src/AuthScreen.tsx`
- 1x `client/src/App.tsx`

---
**Behavioural review** — 5 finding(s) with verified evidence

- **connectionId typed null but tables require undefined** *(generated bindings)*
  - cost: Publish attempt failed tsc twice; required manual `?? undefined` fix at every call site
  - evidence: `src/index.ts(99,52): error TS2322: Type 'ConnectionId | null' is not assignable to type 'ConnectionId | undefined'. Type 'null' is not assignable to type 'ConnectionId | undefined'. src/index.ts(104,9): error TS2322: Typ`
  - possible fix: Make generated ctx.connectionId and table optional-field types consistent (both null or both undefined) so no manual conversion is needed
- **Generated row types don't structurally match hand-written interfaces, forcing unsafe double-casts** *(generated bindings)*
  - cost: tsc TS2352 error; fixed by widening to `as unknown as X[]`, which silently defeats type safety instead of a real fix
  - evidence: `src/ChatApp.tsx(76,12): error TS2352: Conversion of type 'readonly { id: bigint; kind: string; name: string | undefined; ownerUsername: string | undefined; createdAt: Timestamp; }[]' to type 'ConversationRow[]' may be a `
  - possible fix: Emit exported row interfaces from codegen that app code can reference directly, or ensure query-result types are assignable to the shapes described in docs/examples without a double cast
- **SDK retries forever on stale auth token with no error surfaced** *(client SDK)*
  - cost: ~80 actions of debugging across an hour (IPv4 binding red herring, toggle button red herring, viewport tests, etc.) before finding the real cause
  - evidence: `A stale/invalid 'auth_token' causes an infinite 401 retry loop with no fallback, hanging the app on the connecting screen forever.`
  - possible fix: Surface a distinguishable auth/token error via onConnectError (or reject the connect promise) instead of retrying indefinitely, so apps can react without guesswork
- **CLI sql rejects IN expressions entirely, even literal lists** *(CLI/publish)*
  - cost: Two failed DELETE attempts; had to restructure cascading-delete script around single-equality statements only
  - evidence: `WARNING: This command is UNSTABLE and subject to breaking changes. Error: Unsupported expression: message_id IN (1, 2, 3`
  - possible fix: Support IN (...) in the CLI/HTTP SQL surface, at minimum for literal lists
- **CLI call argument syntax undiscoverable from error message** *(CLI/publish)*
  - cost: Two failed `call` attempts (JSON object, then JSON array) before finding the correct bare-positional-JSON-scalar syntax
  - evidence: `Error: Invalid arguments provided for reducer 'sign_up' for database 'stackbench-run0' resolving to identity 'c20085b1bb93350b0c31f05135dfd4bcaac26110bd906aaa23cf16196c78600a'. The reducer has the following signature: si`
  - possible fix: Include a concrete example invocation (e.g. `call db sign_up "alice" "pw"`) in the error message, not just the reducer type signature

---

---
## 2026-08-09 — spacetime-run0 (chat) L1 — RETRACTED, then corrected

An earlier version of this entry claimed `spacetimedb.view` is not subscribable
from a TypeScript client, and that codegen emitting views as table handles is
what cost this run. **That was wrong and is retracted.** Views work.

What actually happened: the investigation was run against
`archive/pre-v1/results/spacetime-run0/app`, which is a leftover directory from a DIFFERENT
run two days earlier (mtime 2026-08-06; the app itself now builds outside the
results tree, see bench.mjs:274). Its schema, its bindings and its missing
`onError` all belonged to that older app. The run actually graded here is in
`archive/pre-v1/results/spacetime-run0/source`, and it is internally consistent: its module
views and its client bindings match exactly, and it does wire `onError`.

Established while chasing it, and worth keeping:

- Views ARE queryable and subscribable. The host has passing SQL tests over
  views (`test_view`, `test_anonymous_view` in crates/core/src/sql/execute.rs),
  a fresh publish registers them, `select * from account_directory` returns a
  result, and the older app -- once pointed at a module built from its own
  source -- connects, signs up, and renders its room list end to end.
- `reset-db.sh` republishes correctly from the app it is given. Run by hand
  against a matching app dir it produced exactly the expected view set.

The hazard is the stale `results/<run>/app` directory. It looks exactly like
the run's application, sits beside the copy that IS the run, and silently
answers questions about the wrong build. It should be removed at the start of
a run, or never left behind.

**Cause of the 0/49, established.** The harness signed up with a hyphenated
username and the app rejects hyphens. Proven against the run's own module:

    sign_up "lint-ab12cd"       -> rejected (530)   <- the linter's name
    sign_up "Alice-l1features"  -> rejected (530)   <- the grader's name
    sign_up "lintab12cd"        -> accepted         <- same name, no hyphen

The app validates usernames as `^[A-Za-z0-9_]+$` (index.ts:124) -- GitHub's
rule, and an ordinary choice. The level spec never states which characters a
username must accept; it says only "create an account with a username and
password" and "usernames are unique". So the harness demanded something the
contract never asked for, sign-up failed, and all 49 criteria reported
"setup failed" for a defensible implementation.

This is a fairness bug in the benchmark, not a SpacetimeDB defect and not
really a model defect. Any app that validates usernames conservatively scored
zero regardless of quality, on any backend -- the other backends passed only
because their builds happened not to validate.

Fixed by making harness-generated account names alphanumeric: the scope
separator is gone from grade.mjs (four sites) and from both linter walks.
`uniq()` is base36, so the whole identifier is now alphanumeric and every
reasonable rule accepts it. Verified against the same module that rejected the
old names: `lintcd34ef` and `Alicel1features` are both accepted. Room names are
deliberately unchanged -- they are display text and their bases already contain
hyphens ({room:room-a}).

**This run's score is void as a measure of SpacetimeDB.** It measures the
harness. It should not be compared with any earlier or later number.

**Separately, a real behavioural finding:** the model ran its own hook check and
saw the same 8-pass-then-blocked result at least eight times (transcript lines
386 through 603) without resolving it, then ended the session saying it had
"scheduled a check in about a minute" that it never performed. Shipping while
its own verifier reported failure is worth addressing regardless of the
username bug.
## 2026-08-09 06:19 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 48/48, $7.0554, 0 fix round(s)

**Tokens** (from the CLI's own usage, 1 session(s), 206 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 28,371,960 | 99% |
| cache write | 366,751 | 1% |
| fresh input | 412 | 0% |
| output | 178,589 | — |

**Where it got stuck** — 3 build failure(s) of 120 tool calls (plus 3 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…'. |
| 1 | TS2554: Expected 1 arguments, but got 0. |
| 1 | ERROR: Invalid argument/option - 'V:/'. |

By SpacetimeDB surface: server API (schema / reducers) (1), other (1), CLI / publish (1)

**Re-read** — 0 read(s) of generated bindings

- 4x `client/src/App.tsx`
- 1x `test-app/src/App.tsx`
- 1x `client/src/App.css`

---
**Behavioural review** — 4 finding(s) with verified evidence

- **Zero-arg reducers require an explicit empty object argument** *(generated bindings)*
  - cost: TypeScript build failure (TS2554) at two call sites, requiring two separate Edit calls and a rebuild to add `{}` to `signOut()` and `checkout()` calls
  - evidence: `src/App.tsx(211,20): error TS2554: Expected 1 arguments, but got 0. src/App.tsx(243,20): error TS2554: Expected 1 arguments, but got 0.`
  - possible fix: Generate a zero-argument, argument-less call signature for reducers with no parameters instead of requiring callers to pass `{}`.
- **Option<f64> not accepted as a view/column return type by the TypeBuilder API** *(generated bindings)*
  - cost: First publish attempt failed to compile; required two Edit calls to work around before a successful publish
  - evidence: `src/index.ts(198,3): error TS2345: Argument of type 'OptionBuilder<F64Builder>' is not assignable to parameter of type 'ViewReturnTypeBuilder'.`
  - possible fix: Either accept OptionBuilder<F64Builder> as a valid ViewReturnTypeBuilder, or emit a clearer diagnostic explaining what return-type shapes are valid for views.
- **`spacetimedb-cli dev` generated bindings to the wrong directory on first run** *(CLI/publish)*
  - cost: The dev watcher had to be killed and restarted with corrected paths after bindings landed at the project root instead of the client's module_bindings folder; a stray `src` directory had to be removed
  - evidence: `Now let's restart 'dev' with the correct project path pointing bindings to 'client/src/module_bindings'.`
  - possible fix: Make the bindings output destination for `dev` explicit/obvious by default (e.g. infer from an existing client project) rather than defaulting relative to CWD.
- **SenderError/InternalError classes are not discoverable from the SDK's main type declaration files** *(docs)*
  - cost: Five separate grep/cat commands across dist/*.d.ts, dist/sdk/*.d.ts, and compiled .mjs bundles before finding the class under dist/lib/errors.d.ts
  - evidence: `D="D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/dist"; grep -rn "SenderError" "$D"/*.d.ts "$D"/sdk/*.d.ts 2>&1 | head -20`
  - possible fix: Re-export reducer error classes (SenderError, InternalError) from the SDK's main entry point / sdk barrel file, and document them in the client error-handling docs.

---
## 2026-08-09 22:51 — spacetime-ecom-run0 (ecommerce) L1

**Result:** 51/51, $11.2764, 1 fix round(s)

**Tokens** (from the CLI's own usage, 2 session(s), 266 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 34,397,503 | 98% |
| cache write | 663,935 | 2% |
| fresh input | 532 | 0% |
| output | 246,984 | — |

**Where it got stuck** — 9 build failure(s) of 149 tool calls (plus 4 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…'. |
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…' |
| 1 | TS2339: Property '…' does not exist on type 'readonly { id: bigint; itemId: bigint; createdAt: Timestamp; accountId: bigint; |
| 1 | Error: Publishing aborted by user |
| 1 | TS1005: '…' expected. |
| 1 | Error: Aggregate expressions must have column aliases |
| 1 | <tool_use_error>Blocked: sleep 30 followed by: cd "C:/Users/bradl/AppData/Local/Temp/stack-bench-runs/spacetime-ecom-run |
| 1 | Error: Cannot find module 'playwright' |

By SpacetimeDB surface: server API (schema / reducers) (5), other (2), generated bindings (1), client SDK (subscriptions) (1)

**Re-read** — 1 read(s) of generated bindings

- 6x `src/components/ItemDetail.tsx`
- 3x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`
- 1x `.templates/basic-react_spacetimedb/tsconfig.json`
- 1x `src/server/views.ts`
- 1x `src/module_bindings/index.ts`

---
**Behavioural review** — 5 finding(s) with verified evidence

- **View return-type errors surface as deep generic-type mismatches, not a clear message** *(generated bindings)*
  - cost: Two failed `spacetime build`/publish attempts (TS2345 errors) before the fix; had to grep the framework's own views.ts source to figure out what ViewReturnTypeBuilder actually requires instead of getting a clear error or finding it in docs
  - evidence: `src/index.ts(271,3): error TS2345: Argument of type 'F64Builder' is not assignable to parameter of type 'ViewReturnTypeBuilder'. Type 'F64Builder' is not assignable to type 'TypeBuilder<object | undefined, OptionAlgebrai`
  - possible fix: Emit a direct diagnostic for invalid view return types (e.g. "views must return t.object/t.array/t.option of an object type, got F64Builder") instead of letting it surface as a nested structural-typing mismatch against an internal type.
- **Reducer error/rejection contract isn't documented — model had to read SDK source to learn it** *(docs)*
  - cost: Multiple greps/reads across reducers.ts, db_connection_impl.ts and errors.ts just to learn how a failed reducer call rejects and what shape/message the client receives, before it could write .catch(err => ...) error handling in the UI
  - evidence: `Bash: grep -rn "reject\|Promise<void>\|message" "D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/crates/bindings-typescript/src/sdk/reducers.ts" 2>/dev/null | he`
  - possible fix: Document the reducer promise rejection contract (SenderError -> Error.message shape) in the TypeScript SDK client docs so this doesn't require reading db_connection_impl.ts/errors.ts source.
- **Publish prompts for interactive confirmation on schema changes with no non-interactive flag used** *(CLI/publish)*
  - cost: Had to pipe `echo y |` into the publish command to get past a breaking-change confirmation prompt instead of using a documented non-interactive flag
  - evidence: `Bash: cd "C:/Users/bradl/AppData/Local/Temp/stack-bench-runs/spacetime-ecom-run0-20260809215454/app" && echo y | D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/`
  - possible fix: Document (or surface in --help) a --yes/-y flag for `spacetime publish` so scripted/agent workflows don't need to pipe stdin to answer the breaking-change prompt.
- **Client-disconnect lifecycle reducer fires on reload/reconnect, silently wiping session state** *(docs)*
  - cost: An entire second session (~60 actions, including writing a bespoke Playwright test harness) was spent diagnosing 6 separate reported bugs that all traced back to one root cause: the disconnect lifecycle reducer deleting the session row on every websocket drop, including page reloads that reconnect with the same identity/token
  - evidence: `The clearest bug: 'onDisconnect' in 'backend/spacetimedb/src/index.ts' deletes the session row whenever the connection drops — including on a page reload/reconnect using the same identity/token, which would wipe out the `
  - possible fix: Document explicitly that the disconnect lifecycle callback fires on every dropped websocket connection (including reloads/reconnects with the same identity) and is not a reliable 'user logged out' signal, with guidance not to key session cleanup off it directly.
- **SQL aggregate queries fail without column aliases, with only a bare error** *(CLI/publish)*
  - cost: One failed `spacetime sql` invocation before the model figured out it needed to alias the aggregate column
  - evidence: `WARNING: This command is UNSTABLE and subject to breaking changes. Error: Aggregate expressions must have column aliases`
  - possible fix: Either infer a default alias for unaliased aggregate expressions or have the error message state the required syntax (e.g. `SELECT COUNT(*) AS cnt FROM ...`).

---

---
## 2026-08-09 — why a SpacetimeDB build costs more, measured

Both stacks built the same L1 ecommerce spec to a full score. PostgreSQL cost
$7.27, SpacetimeDB $11.28 — a $4.01 gap. This is where it went, from the
transcripts of those two runs rather than from a ratio.

**Not code volume.** Output tokens are identical: 0.097M on both. SpacetimeDB
writes LESS code — 78KB of `Write` input against 84KB. It also makes FEWER tool
calls, 149 against 167.

**Not the guidance pack.** Inlining the skill documents makes the SpacetimeDB
prompt 2.2x larger (42,551 bytes against 19,337), and that is real, but it is
~5,750 tokens re-read over 103 turns: 592K tokens, about 18 cents at cache-read
rates. Roughly 12% of the gap. An earlier note here claimed it was most of the
gap; that was wrong and is retracted.

**It is the TypeScript compiler.**

| | postgres | spacetime |
|---|---|---|
| compile attempts | 3 | 10 |
| compiler output | 1KB | 21KB |
| TypeScript errors | **0** | **21** |

The errors are almost all generated-bindings friction:

- **TS2344 x10** — a generic constraint failure on the generated schema types.
  These print as expanded structural dumps, 414 characters for a single line:
  `Type 'Readonly<{ type: "table"; sourceName: string; accessorName: string;
  cols: Readonly<{ readonly itemId: ColumnExpr<TableToSchema<"myCart",
  TableSchema<CoerceRow<{ itemId: U64ColumnBuilder<{ name: "item_id"; }>; ...`
  does not satisfy the constraint `UntypedTableDef`.
- **TS4104 x3 and TS2339 x2** — query results are `readonly` arrays, so the
  obvious `.push` does not compile: "Property 'push' does not exist on type
  'readonly { id: bigint; itemId: bigint; ... }[]'".
- **TS2345 x2** — `Argument of type 'F64Builder' is not assignable to parameter
  of type 'ViewReturnTypeBuilder'`. A view cannot return a plain f64.

**Why that becomes dollars.** 97-98% of every bill is cache reads: the whole
conversation re-read on each turn. An error dump does not cost once, it costs on
every subsequent turn for the rest of the run. Ten compile attempts returning
expanded structural types is a permanent tax on the remaining context, which is
also why SpacetimeDB carries +22% thinking (284KB against 233KB) — there is more
to reason about, and it never leaves.

**What would actually reduce it**, in order of how much noise each removes:

1. Make the generated table types satisfy their own constraints without
   expanding, or give `UntypedTableDef` a nominal shape, so a mismatch prints a
   name instead of the whole structure.
2. Accept the scalar builders where a view return type is expected, or say in
   the error which builders are valid.
3. Return arrays that can be worked with, or make the readonly-ness obvious at
   the call site rather than at the compile step.

Postgres produced zero compiler errors across the entire run. That is the
comparison.

---
## 2026-08-09 — the cost gap, attributed per call

Three stacks built the same ecommerce L1 spec to 51/51. PostgreSQL $7.27,
MongoDB $5.95, SpacetimeDB $11.28. Cost is (calls x context carried per call),
and both terms are in the usage records:

| stack | calls | start ctx | end ctx | avg ctx | bill |
|---|---|---|---|---|---|
| postgres | 154 | 50K | 174K | 96K | $7.27 |
| mongodb | 90 | 50K | 186K | 121K | $5.95 |
| spacetime | 146 | 59K | 218K | 132K | $11.28 |

**SpacetimeDB takes FEWER steps than PostgreSQL** — 146 against 154, worth -11%.
It costs more entirely because each step carries 36K more context: +9K fixed
(the inlined skill documents, present from the first call) and +35K accumulated.
Context is re-read on every later call, so accumulated weight is paid ~146
times: +35K of conversation becomes +5.6M tokens billed.

MongoDB shows the two levers are independent — its context is HEAVIER than
PostgreSQL's (121K against 96K) and it is still cheapest, because it finished in
90 calls. PostgreSQL wins on context weight, MongoDB on step count, SpacetimeDB
neither.

**What filled the extra context.** 21 TypeScript errors against PostgreSQL's
zero, ten of them TS2344 constraint failures that print expanded structural
types, and two subagents costing $1.44 — 36% of the gap. PostgreSQL dispatched
none.

**What the subagent was doing, exactly.** It was sent to read the bindings
source, and it searched for one thing:

    grep  primaryKey|unique\(|uniqueConstraint|composite
    grep  primary key|primaryKey|must have|at least one
    cat   table.ts, table_schema.ts, constraints.ts, indexes.ts
    grep  RawConstraintDefV10

It was trying to find out whether a unique constraint can span two columns.
The answer is no:

    // crates/bindings-typescript/src/lib/constraints.ts:48
    } & { constraint: 'unique'; columns: [AllowedCol] };

`columns` is a one-element tuple, so `.unique()` is single-column only. The
skill documents describe multi-column INDEXES and never mention this limit. The
shop needs exactly the missing thing — stock is per item per warehouse — so the
model spent 36 tool calls and 1.24M tokens discovering a fact that fits in one
sentence.

**Fixes, cheapest first:**

1. State in the skill document that `.unique()` takes one column, and show how
   to model a composite-keyed table (surrogate id plus a multi-column index).
   Free, and it is the thing the subagent went looking for.
2. Make TS2344 name the offending type instead of expanding it. Ten of these
   landed in context permanently.
3. Support multi-column unique constraints. Larger, and the one that removes
   the modelling problem rather than documenting it.

**Honest scale.** These account for perhaps $1.0-1.5 of a $4.01 gap. The rest is
diffuse — heavier reasoning throughout rather than one identifiable event — so
nothing here should be presented as closing the gap on its own.
## 2026-08-10 03:39 — spacetime-ecom-run0 (ecommerce) L2

> ⚠️ **This run was contaminated** — the build read the harness that grades it.
> The friction below still happened, but it is a floor rather than a measurement:
> a build with the answers fights the SDK less than one without them.

**Result:** 0/0, $12.655, 1 fix round(s)

**Tokens** (from the CLI's own usage, 4 session(s), 643 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 91,209,038 | 98% |
| cache write | 1,758,199 | 2% |
| fresh input | 1,286 | 0% |
| output | 670,420 | — |

**Where it got stuck** — 21 build failure(s) of 370 tool calls (plus 9 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…'. |
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…' |
| 1 | TS2339: Property '…' does not exist on type 'readonly { id: bigint; itemId: bigint; createdAt: Timestamp; accountId: bigint; |
| 1 | Error: Publishing aborted by user |
| 1 | TS1005: '…' expected. |
| 1 | Error: Aggregate expressions must have column aliases |
| 1 | <tool_use_error>Blocked: sleep 30 followed by: cd "C:/Users/bradl/AppData/Local/Temp/stack-bench-runs/spacetime-ecom-run |
| 1 | Error: Cannot find module 'playwright' |

By SpacetimeDB surface: server API (schema / reducers) (13), other (6), generated bindings (1), client SDK (subscriptions) (1)

**Re-read** — 8 read(s) of generated bindings

- 6x `src/components/ItemDetail.tsx`
- 6x `spacetimedb/src/index.ts`
- 3x `client/src/App.tsx`
- 3x `spacetimedb/src/schema.ts`
- 3x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`

---
**Behavioural review** — 7 finding(s) with verified evidence

- **View return-type builder rejects composite field builders with an unhelpful generic error** *(server API)*
  - cost: Two failed publish attempts; had to grep and read the SDK's internal views.ts source to understand the ViewReturnTypeBuilder constraint before fixing the view definition
  - evidence: `src/index.ts(271,3): error TS2345: Argument of type 'F64Builder' is not assignable to parameter of type 'ViewReturnTypeBuilder'. Type 'F64Builder' is not assignable to type 'TypeBuilder<object | undefined, OptionAlgebrai`
  - possible fix: Emit a targeted diagnostic (e.g. 'view return type must be wrapped in row()/table()') instead of a deep structural type-mismatch error, or document the ViewReturnTypeBuilder contract next to the view() API.
- **Generated table/view row arrays are readonly with no documented heads-up** *(generated bindings)*
  - cost: Type-check failed across multiple call sites (push/property-access on generated row arrays) requiring a rewrite of client-side types.ts and several follow-up edits
  - evidence: `src/App.tsx(83,12): error TS2339: Property 'push' does not exist on type 'readonly { id: bigint; itemId: bigint; createdAt: Timestamp; accountId: bigint; rating: number; comment: string; }[]'.`
  - possible fix: Call out in the generated-bindings docs/comments that table/view rows are readonly arrays, or provide a typed helper for deriving mutable view-models from them.
- **spacetime sql rejects ORDER BY / LIMIT with no upfront documentation of the supported subset** *(CLI/publish)*
  - cost: A verification query failed outright, forcing the model to fall back to pulling the whole table and sorting/limiting client-side
  - evidence: `Error: Unsupported: SELECT * FROM customer_order ORDER BY id DESC LIMIT 5`
  - possible fix: Document the supported SQL subset (or the specific unsupported clauses) in `spacetime sql --help` / docs so agents don't have to trial-and-error discover it.
- **spacetime sql rejects mixed wildcard projections (t.*, other_col)** *(CLI/publish)*
  - cost: A second query failed immediately after the ORDER BY failure in the same debugging pass, requiring another manual rewrite to explicit column lists
  - evidence: `Error: Mixed wildcard projections are not supported`
  - possible fix: Support mixed wildcard projections (common in joins) or surface this restriction in the sql command's help text / error message with a suggested rewrite.
- **Aggregate SQL queries require column aliases with no guidance in the error** *(CLI/publish)*
  - cost: One failed sql invocation before the model learned to add an alias to the aggregate expression
  - evidence: `Error: Aggregate expressions must have column aliases`
  - possible fix: Have the error message name the offending expression and suggest the alias syntax, e.g. 'use SUM(x) AS total'.
- **TypeScript SDK client/react hooks are undocumented, forcing repeated source-diving** *(docs)*
  - cost: In nearly every session the model had to grep/read internal SDK source files (useTable.ts, SpacetimeDBProvider.ts, connection_state.ts, errors.ts, reducers.ts) to learn the actual client API surface instead of consulting docs
  - evidence: `SAYS: All matches expectations. Now let's check the 'spacetimedb/react' hooks package to confirm 'useTable'/'useSpacetimeDB' API.`
  - possible fix: Publish reference docs for the react hooks package (useTable, useSpacetimeDB, SpacetimeDBProvider) and the reducer error/rejection contract (SenderError) so this isn't only discoverable by reading source.
- **init reducer silently doesn't re-run on republish, with no CLI warning** *(CLI/publish)*
  - cost: New seed data (categories, staff account) silently failed to appear after a schema-adding publish; diagnosing this consumed a debugging cycle and the eventual fix required a full --delete-data wipe of accumulated state
  - evidence: `I found the issue — categories and the staff account weren't seeded because 'init' only runs on a database's first creation, and this database already existed from Level 1.`
  - possible fix: Have `spacetime publish` print an explicit note when init is skipped because the database already exists, so it's not mistaken for a bug in the module's seed logic.

---
## 2026-08-10 21:32 — spacetime-ecom-run2 (ecommerce) L1

**Result:** 51/51, $6.4508, 0 fix round(s)

**Tokens** (from the CLI's own usage, 1 session(s), 201 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 26,841,346 | 98% |
| cache write | 420,394 | 2% |
| fresh input | 441 | 0% |
| output | 242,606 | — |

**Where it got stuck** — 3 build failure(s) of 129 tool calls (plus 3 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | spacetimedb: error: unexpected argument '-s' found |
| 1 | TS7053: Element implicitly has an '…' type because expression of type '…' can'…'IteratorObject<{ id: bi |
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…' |

By SpacetimeDB surface: server API (schema / reducers) (2), CLI / publish (1)

**Re-read** — 15 read(s) of generated bindings

- 4x `client/src/App.tsx`
- 1x `backend/spacetimedb/package.json`
- 1x `backend/spacetimedb/tsconfig.json`
- 1x `src/module_bindings/index.ts`
- 1x `src/module_bindings/item_table.ts`
- 1x `src/module_bindings/my_account_table.ts`

---
**Behavioural review** — 5 finding(s) with verified evidence

- **server subcommand rejects -s server-selector flag** *(CLI/publish)*
  - cost: one failed CLI call before falling back to a different invocation to find the registered server
  - evidence: `error: unexpected argument '-s' found Usage: spacetimedb-cli.exe server list For more information, try '--help'.`
  - possible fix: Either support a consistent -s/--server flag across all `spacetimedb-cli server` subcommands or make the error suggest the correct syntax for that subcommand
- **basic server ping command flagged UNSTABLE** *(CLI/publish)*
  - cost: no retries incurred, but every basic connectivity check for a server the model is expected to routinely use surfaces a breaking-change warning, undermining confidence in a core CLI command
  - evidence: `WARNING: This command is UNSTABLE and subject to breaking changes. Server is online: http://127.0.0.1:3000`
  - possible fix: Stabilize `server ping` (or `server list`) since it's a basic operation needed just to discover whether a server is reachable, rather than leaving it marked unstable
- **table index filter() returns a non-indexable iterator** *(client SDK)*
  - cost: TypeScript compile failure required locating every call site (grep) and rewriting 5 separate lines (cartItem.byCartItem.filter(...)[0] pattern) across the reducer file
  - evidence: `Element implicitly has an 'any' type because expression of type '0' can't be used to index type 'IteratorObject<{ id: bigint; itemId: bigint; quantity: number; cartId: bigint; }, undefined, unknown>'.`
  - possible fix: Either make unique-index filter() return the single row directly (or an array) instead of a bare iterator, or document/provide a first()-style helper so callers don't reach for array indexing
- **view builder rejects scalar return type without clear guidance, and the object-wrapped fix still fails to typecheck** *(generated bindings)*
  - cost: two separate rounds of edits to the same view (adminRevenue): first wrapping the scalar in an object, then a second TS2345 failure on the resulting builder type before it compiled
  - evidence: `Argument of type 'ProductBuilder<{ total: F64Builder; }>' is not assignable to parameter of type 'ViewReturnTypeBuilder'.`
  - possible fix: Improve the view-return-type builder's error message to state directly that scalar returns must be wrapped in an object type, and fix/align ProductBuilder's type so a correctly-wrapped single-field object satisfies ViewReturnTypeBuilder without a second unrelated type error
- **publish/generate overrides user's tsconfig verbatimModuleSyntax with a warning on every run** *(CLI/publish)*
  - cost: noise repeated on both the publish step and the generate step; no functional break but adds uncertainty about which compiler settings actually take effect
  - evidence: `[CONFIGURATION_FIELD_CONFLICT] Warning: 'compilerOptions.verbatimModuleSyntax' from 'tsconfig.json' is overridden by 'on`
  - possible fix: Either respect the project's tsconfig.json setting for verbatimModuleSyntax or document that spacetime CLI forces this field so scaffolded tsconfig.json files don't fight the tool

---
## 2026-08-10 21:39 — spacetime-ecom-run0 (ecommerce) L2

**Result:** 50/50, $14.6235, 1 fix round(s)

**Tokens** (from the CLI's own usage, 4 session(s), 643 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 91,209,038 | 98% |
| cache write | 1,758,199 | 2% |
| fresh input | 1,286 | 0% |
| output | 670,420 | — |

**Where it got stuck** — 21 build failure(s) of 370 tool calls (plus 9 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…'. |
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…' |
| 1 | TS2339: Property '…' does not exist on type 'readonly { id: bigint; itemId: bigint; createdAt: Timestamp; accountId: bigint; |
| 1 | Error: Publishing aborted by user |
| 1 | TS1005: '…' expected. |
| 1 | Error: Aggregate expressions must have column aliases |
| 1 | <tool_use_error>Blocked: sleep 30 followed by: cd "C:/Users/bradl/AppData/Local/Temp/stack-bench-runs/spacetime-ecom-run |
| 1 | Error: Cannot find module 'playwright' |

By SpacetimeDB surface: server API (schema / reducers) (13), other (6), generated bindings (1), client SDK (subscriptions) (1)

**Re-read** — 8 read(s) of generated bindings

- 6x `src/components/ItemDetail.tsx`
- 6x `spacetimedb/src/index.ts`
- 3x `client/src/App.tsx`
- 3x `spacetimedb/src/schema.ts`
- 3x `client/src/App.tsx`
- 2x `spacetimedb/src/index.ts`

---
**Behavioural review** — 6 finding(s) with verified evidence

- **View return type builder errors give no actionable guidance** *(generated bindings)*
  - cost: two rounds of TS compile failures on the same view definition; had to grep and read internal SDK source (views.ts) to understand ViewReturnTypeBuilder before getting the type right
  - evidence: `src/index.ts(271,3): error TS2345: Argument of type 'F64Builder' is not assignable to parameter of type 'ViewReturnTypeBuilder'. Type 'F64Builder' is not assignable to type 'TypeBuilder<object | undefined, OptionAlgebrai`
  - possible fix: Improve the TS error message (or docs) to state directly how to declare a view row's computed/aggregate field type, instead of surfacing a generic structural-typing mismatch that requires reading SDK internals.
- **clientDisconnected fires on ordinary reconnects, not just logout** *(server API)*
  - cost: root cause of six separate reported bugs (session/cart/order state wiped on page reload); required a whole extra debugging session with an investigation agent, log analysis, and Playwright verification to diagnose and fix
  - evidence: `The clearest bug: 'onDisconnect' in 'backend/spacetimedb/src/index.ts' deletes the session row whenever the connection drops — including on a page reload/reconnect using the same identity/token, which would wipe out the `
  - possible fix: Document explicitly that the disconnect lifecycle hook fires on every WebSocket drop (including page reloads/reconnects with the same identity), and warn against treating it as an explicit-logout signal in the client SDK / module docs.
- **SQL rejects unaliased aggregate expressions** *(server API)*
  - cost: ad hoc verification query failed and had to be rewritten via trial and error against the CLI
  - evidence: `Error: Aggregate expressions must have column aliases`
  - possible fix: Auto-generate a default column alias for aggregate expressions instead of erroring, or surface the aliasing requirement in `spacetime sql --help`/docs.
- **SQL disallows SELECT * combined with ORDER BY/LIMIT** *(server API)*
  - cost: query failed and had to be reissued without ORDER BY/LIMIT, losing the ability to directly inspect the most recent rows
  - evidence: `Error: Unsupported: SELECT * FROM customer_order ORDER BY id DESC LIMIT 5 Caused by: HTTP status client error (400 Bad Request) for url (http://127.0.0.1:3210/v1/database/c2002e72349371acb228841462af8f0fa1f9eb3c224bb4956`
  - possible fix: Support ORDER BY/LIMIT with SELECT * in spacetime sql, or document the restriction alongside the wildcard-projection rules.
- **SQL disallows 'mixed wildcard projections'** *(server API)*
  - cost: query had to be rewritten to explicitly enumerate columns after a second unsupported-SQL error in the same debugging session
  - evidence: `Error: Mixed wildcard projections are not supported Caused by: HTTP status client error (400 Bad Request)`
  - possible fix: Either support selecting `t.*` alongside other columns/tables, or document this SQL subset restriction clearly so it doesn't have to be discovered by trial and error.
- **`init` reducer silently skips reseeding on redeploy to an existing database** *(CLI/publish)*
  - cost: new schema's seed data (categories, staff account) never got created after a normal publish; diagnosed only after functional testing failed, then worked around with a destructive `--delete-data` republish
  - evidence: `I found the issue — categories and the staff account weren't seeded because 'init' only runs on a database's first creation, and this database already existed from Level 1.`
  - possible fix: Warn during `spacetime publish` when the module's init reducer has effectively no-op'd because the database already exists, so schema/seed additions don't silently fail to apply.

---
## 2026-08-10 21:54 — spacetime-ecom-run1 (ecommerce) L1

**Result:** 51/51, $12.1274, 1 fix round(s)

**Tokens** (from the CLI's own usage, 2 session(s), 344 turns)

| | tokens | share of input |
|---|---:|---:|
| cache read | 51,413,497 | 99% |
| cache write | 712,094 | 1% |
| fresh input | 688 | 0% |
| output | 312,587 | — |

**Where it got stuck** — 9 build failure(s) of 190 tool calls (plus 3 refused by the sandbox — harness, not SpacetimeDB)

| times | error |
|---:|---|
| 1 | TS7053: Element implicitly has an '…' type because expression of type '…' can'…'IteratorObject<{ id: bi |
| 1 | TS2345: Argument of type '…' is not assignable to parameter of type '…' |
| 1 | Error: Errors occurred: |
| 1 | ERROR: The process "84397" not found. |
| 1 | Exit code 127 /usr/bin/bash: line 1: wmic: command not found |
| 1 | ERROR: Invalid argument/option - 'V:/'. |
| 1 | Error: `--module-path` cannot be used when `spacetime.json` contains publish targets. Remove `--module-path` or run with |
| 1 | Error: IO error: not a terminal |

By SpacetimeDB surface: generated bindings (4), CLI / publish (3), server API (schema / reducers) (2)

**Re-read** — 2 read(s) of generated bindings

- 6x `client/src/index.css`
- 4x `spacetimedb/src/index.ts`
- 4x `client/src/App.tsx`
- 3x `src/components/ItemDetail.tsx`
- 2x `spacetime-ecom-run1-20260810205827/app/check-hooks.sh`
- 2x `spacetime-ecom-run1-20260810205827/app/spacetime.json`

---
**Behavioural review** — 4 finding(s) with verified evidence

- **Indexed table accessor .filter() returns a lazy iterator, not an array, breaking the natural `[0]` idiom** *(generated bindings)*
  - cost: Two call sites (index.ts lines 71 and 209) failed TypeScript compilation with the same error. First fix attempt used a sed regex to wrap calls in `[...iter][0]`, which corrupted the accessor names (e.g. producing `by_ITEMPLACEHOLDER`), requiring a second corrective pass (a Python script) across the file to repair it.
  - evidence: `src/index.ts(71,20): error TS7053: Element implicitly has an 'any' type because expression of type '0' can't be used to index type 'IteratorObject<{ id: bigint; itemId: bigint; accountId: bigint; }, undefined, unknown>'.`
  - possible fix: Have indexed accessor `.filter()` return an array (or provide a `.first()`/`.findOne()` convenience method) so the obvious 'get the matching row' pattern doesn't require spreading an iterator.
- **view() rejects a bare primitive return-type builder (t.f64()) with a confusing generic-constraint error** *(server API)*
  - cost: Publish attempt failed compilation; required reading the SDK's internal ViewReturnTypeBuilder type in src/server/schema.ts to discover views must return an object-shaped builder, then editing the view to wrap the f64 in an object.
  - evidence: `src/index.ts(377,87): error TS2345: Argument of type 'ProductBuilder<{ total: F64Builder; }>' is not assignable to parameter of type 'ViewReturnTypeBuilder'.`
  - possible fix: Either allow primitive return-type builders for view(), or surface a clear top-level error message ('view return type must be an object; wrap primitives like t.f64() in an object') instead of a generic type-assignability error.
- **Wrapping a view's return type in an object triggered an opaque 'name used for multiple types' publish error** *(server API)*
  - cost: After fixing the prior view-return-type error, republishing failed with a bare name-collision message that didn't say where the conflicting type came from, costing another Read+Edit cycle to guess the correct fix (renaming the inline object type).
  - evidence: `Error: Errors occurred: name 'AdminRevenue' is used for multiple types`
  - possible fix: Include the two conflicting declaration sites (file/line or construct) in the error message so the collision can be resolved without guessing.
- **`spacetime dev --module-path` silently conflicts with a `server` field in spacetime.json** *(CLI/publish)*
  - cost: Dev-mode startup failed after the module bindings/backend were already scaffolded; the agent had to grep the CLI's own Rust source (dev.rs) to understand the constraint, then edit spacetime.json twice (changing the `server` field, restarting the watcher) across three attempts before `dev` started cleanly.
  - evidence: `started pid 84571 Error: '--module-path' cannot be used when 'spacetime.json' contains publish targets. Remove '--module`
  - possible fix: Detect this conflict earlier (at config load) with a message that also states which config fields count as 'publish targets', and/or let `--module-path` override rather than hard-conflict with spacetime.json when both point at the same module.

---
