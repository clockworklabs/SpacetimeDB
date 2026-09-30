# v2.5 research (v2.4.0 → v2.5.0)

Scope note: `v2.4.1` was cut from a release branch (`release/v2.4.1`) and is **not** an ancestor of `v2.5.0`;
its two PRs (#5111, #5145) also landed on master and are inside `v2.4.0..v2.5.0`. So "everything a v2.4.0 user
notices in v2.5.0" = `git log v2.4.0..v2.5.0` (33 commits). All claims below were checked with
`git show "<tag>:<path>"` at `v2.4.0` and `v2.5.0`.

## Releases in range
- `v2.4.1` — GitHub release 2026-06-05. Two patches: primary keys on procedural views (Rust + TypeScript, #5111) and a fix for index names rebuilt from system tables (#5145, issue #4701).
- `v2.5.0` — GitHub release 2026-06-11 (tag commit 2026-06-11). "Graduates procedures to stable", C# view primary keys, layout-altering automigrations for event tables, CLI fixes, billing-metric fix.
- (Not in range: `v2.5.0-hotfix1`, 2026-06-12, is covered by the 2.6 research.)

## Candidates

### Procedures are stable (no more `unstable` opt-in) — HIGH
- What changed (exact, verified): #5164 removed the `unstable` gates from procedures in Rust, C# and C++ module libraries.
  - Rust (`crates/bindings/src/lib.rs`): `#[cfg(feature = "unstable")]` removed from `pub use spacetimedb_bindings_macro::procedure`, `ProcedureContext`, `TxContext`, `with_tx`/`try_with_tx`, `sleep_until`, and from `pub mod http`. In `crates/bindings/src/http.rs` only the outgoing client (`HttpClient`, `ctx.http.get(...)` / `ctx.http.send(...)`) is ungated; `HandlerContext`, `Handler`, `Router` (HTTP handlers) are still `#[cfg(feature = "unstable")]`.
  - C# (`crates/bindings-csharp/Runtime/ProcedureContext.cs`): `[Experimental("STDB_UNSTABLE")]` removed from `WithTx` and `TryWithTx` (so no more `#pragma warning disable STDB_UNSTABLE` needed for them).
  - C++: procedure ABI, `procedure_context.h`, `tx_execution.h` and the outgoing HTTP client (`http.h`) compile without `#define SPACETIMEDB_UNSTABLE_FEATURES`; handler/router headers still `#error` without it.
  - Docs: the banner "Procedures are currently in beta, and their API may change in upcoming SpacetimeDB releases." was removed from `docs/docs/00200-core-concepts/00200-functions/00400-procedures.md`.
- Before this range: at `v2.4.0` Rust needed `spacetimedb = { version = "2.*", features = ["unstable"] }`, C# needed `#pragma warning disable STDB_UNSTABLE` to call `WithTx`, C++ needed `#define SPACETIMEDB_UNSTABLE_FEATURES`. TypeScript procedures had **no** gate (only the beta banner).
- Headline idea: "Procedures: out of beta." / "No flags. Just ship."
- One-sentence description: Procedures — server functions that can call external HTTP APIs and open their own transactions — now work in Rust, C# and C++ modules without turning on any experimental feature flag.
- Visual idea: a `Cargo.toml` line `features = ["unstable"]` gets struck through and deleted; the procedure code below (`#[spacetimedb::procedure] fn fetch_weather(ctx: &mut ProcedureContext) { ctx.http.get(...) ... ctx.with_tx(...) }`) still compiles with a green check. A "BETA" badge peels off a "Procedures" card.
- Caveats (don't claim):
  - Don't say procedures are *new* — they existed since v1.10 behind the flag; this is graduation to stable.
  - For TypeScript nothing changed functionally (no flag existed); only the "beta" label went away.
  - HTTP **handlers/webhooks** (inbound HTTP), RLS (`client_visibility_filter`) and `volatile_nonatomic_schedule_immediate` are still unstable — don't imply "HTTP endpoints" or "webhooks" are stable.
  - The release note says "views ... remain gated behind unstable" — that's wrong; views are not gated at v2.5.0 (no `cfg` on `#[view]`). Don't repeat it.
  - The in-repo procedures doc at both `v2.5.0` and `v2.6.0` still contains the old "Unstable Feature" instructions for C#/Rust/C++ (lines telling users to add `features = ["unstable"]` / `#pragma warning disable STDB_UNSTABLE` / `#define SPACETIMEDB_UNSTABLE_FEATURES`); only the beta banner was removed. Don't screenshot those docs as proof.
  - "Scheduled procedures" were covered by the ungating; don't claim new scheduling capabilities.
- Sources: PR #5164; `git diff v2.4.0 v2.5.0 -- crates/bindings/src/lib.rs crates/bindings/src/http.rs crates/bindings-csharp/Runtime/ProcedureContext.cs`; `git show "v2.4.0:docs/docs/00200-core-concepts/00200-functions/00400-procedures.md"` lines 50-75; v2.5.0 note: "Procedures-scheduled, transaction-capable server-side functions-and the outgoing HTTP client (`ctx.http`) are now available without opting into unstable features".

### SolidJS integration + `solid-ts` template — HIGH
- What changed (exact, verified): #5052 (merged 2026-06-03, community contribution by @MAST1999) added a `spacetimedb/solid` entry point to the TypeScript SDK (`crates/bindings-typescript/package.json` export `"./solid"`) exporting `SpacetimeDBProvider`, `useSpacetimeDB`, `useTable`, `useReducer`, `useProcedure` (`src/solid/index.ts`); a new built-in template `templates/solid-ts` ("SolidJS web app with TypeScript server"; templates are embedded in the CLI at build time via `crates/cli/build.rs`); and a docs quickstart `docs/docs/00100-intro/00200-quickstarts/00162-solid.md` whose first step is `spacetime dev --template solid-ts`.
- Before this range: no `solid` export in `v2.4.0:crates/bindings-typescript/package.json`, no `templates/solid-ts`. Existing framework templates at v2.4.0: React, Vue, Svelte, Angular.
- Headline idea: "Now speaking SolidJS."
- One-sentence description: SolidJS apps get first-class SpacetimeDB support — hooks that keep your UI in sync with live database tables — plus a ready-made starter you can launch with one command.
- Visual idea: terminal types `spacetime dev --template solid-ts`; a browser opens "SpacetimeDB SolidJS App" with a green "Connected" status, a name input and an "Add Person" button; a second window adds a name and the list updates live in both. Framework logo row (React, Vue, Svelte, Angular) gets a new Solid logo sliding in. Code overlay: `const [people] = useTable(() => tables.person);`.
- Caveats (don't claim):
  - Not mentioned in the v2.5.0 release notes body (only as a "New Contributors" entry) — but it is in the v2.5.0 tag and CLI.
  - The PR says it was lightly tested ("I haven't tested it too much except for the example that I added") and docs were LLM-written; don't call it "battle-tested".
  - Template id is `solid-ts` (TypeScript server). Don't imply Rust/C# server variants of the Solid template.
  - Auto-reconnect for the provider came later (#5185, v2.6.0) and is only documented for React.
- Sources: PR #5052; `git show "v2.5.0:crates/bindings-typescript/src/solid/index.ts"`; `v2.5.0:templates/solid-ts/.template.json`; `v2.5.0:templates/solid-ts/src/App.tsx`; `v2.5.0:docs/docs/00100-intro/00200-quickstarts/00162-solid.md`.

### Primary keys on procedural views → clients get row updates — HIGH (or MEDIUM)
- What changed (exact, verified): Views whose body is ordinary code returning rows ("procedural views", returning `Vec<T>`/`Option<T>` / arrays) can now declare a primary key, so subscribed clients receive **update** events (`OnUpdate` / `onUpdate`) instead of only delete+insert.
  - Rust (#5111, first shipped in v2.4.1): `#[spacetimedb::view(accessor = my_players, public, primary_key = id)]` (arg parsed in `crates/bindings-macro/src/view.rs`).
  - TypeScript (#5111): the primary key comes from the row type, e.g. `id: t.u64().primaryKey()` in `t.row(...)`, then `spacetimedb.view({ public: true }, t.array(players.rowType), ctx => ...)`.
  - C# (#5246, v2.5.0): `[SpacetimeDB.View(Accessor = "sender_left_view", Public = true, PrimaryKey = "id")]` (new `PrimaryKey` property in `crates/bindings-csharp/Runtime/Attrs.cs`; new diagnostic "View primary key column type is not supported").
- Before this range: at v2.4.0 `ViewDef.primary_key` was only set "for query-builder views when the underlying table has a primary key" (`crates/schema/src/def.rs` doc comment). Procedural views had no way to declare one → no update events.
- Headline idea: "Views that know what changed."
- One-sentence description: A view can now say which column identifies each row, so apps see "Alice's score went from 10 to 12" as one update instead of a row vanishing and reappearing.
- Visual idea: split screen before/after. Left (v2.4): a leaderboard row blinks out and back in (`onDelete` + `onInsert` log lines). Right (v2.5): the same row's score ticks 10 → 12 in place with a single `onUpdate(old, new)` log line. Code chip: `primary_key = id`.
- Caveats (don't claim):
  - Only **procedural** views are new; query-builder views already inherited PKs.
  - C++ modules: not supported (v2.6.0 note: "C++ support for primary keys in views will be added in a future release").
  - The view must never return duplicate primary keys; if it does, the transaction that triggered the view refresh is rolled back (PR #5111).
  - Adding/changing a view's primary key requires a client update (bindings regen); on an existing DB it is treated as an incompatible view change: automigration removes and re-adds the view and calls `ensure_disconnect_all_users()` (`old.primary_key != new.primary_key` in `v2.5.0:crates/schema/src/auto_migrate.rs`).
  - TypeScript: the PK is **implicit** — `views.ts` registers a view PK whenever the returned row type has a `.primaryKey()` column (`viewPrimaryKeyColumns(ret)`). So an existing TS view returning e.g. `t.array(players.rowType)` gains a PK just by upgrading the `spacetimedb` package, and the next republish becomes a client-disconnecting view change. Don't show a TS "add `primary_key`" step like the Rust one; show Rust (explicit `primary_key = id`) or C# (`PrimaryKey = "id"`).
  - v2.6.0 release notes re-announce this as a 2.6 feature; it is **not** new in 2.6 (Rust/TS in 2.4.1, C# in 2.5.0). Put it in the 2.5 video only.
- Sources: PRs #5111, #5246; `v2.5.0:modules/sdk-test-procedural-view-pk/src/lib.rs`, `v2.5.0:modules/sdk-test-procedural-view-pk-ts/src/index.ts`, `v2.5.0:modules/sdk-test-procedural-view-pk-cs/Lib.cs`; v2.4.1 note: "Now clients can receive update events when subscribed to such views."

### `spacetime call` accepts a plain hex Identity — MEDIUM
- What changed (exact, verified): #5254, `crates/cli/src/subcommands/call.rs`: for a reducer/procedure parameter of type `Identity`, an argument that starts with `0x` or with `c200` (the prefix of SpacetimeDB identities) is rewritten into the JSON tuple form `["0x…"]`. The tuple form and `{"__identity__": "0x…"}` still work. Smoketest `test_call_reducer_procedure_with_identity_argument` covers all four encodings for a reducer and a procedure.
- Before this range: only the JSON forms (`'["0x…"]'` or `'{"__identity__": "0x…"}'`) were accepted.
- Headline idea: "Paste the identity. Done."
- One-sentence description: When calling a function from the terminal, you can now paste a user's identity as-is instead of wrapping it in JSON brackets and quotes.
- Visual idea: terminal: `spacetime call my-game ban_player '["0xc200ab…"]'` fades to `spacetime call my-game ban_player c200ab…` → "✓".
- Caveats (don't claim): only `0x…` or `c200…`-prefixed strings are rewritten; other hex strings are passed through unchanged. Only the `call` CLI changed — server-side deserialization (HTTP API) is unchanged ("Proper deserialization on the server is more complicated to change").
- Sources: PR #5254; `git diff v2.4.0 v2.5.0 -- crates/cli/src/subcommands/call.rs`; `v2.5.0:crates/smoketests/tests/smoketests/call.rs`.

### `spacetime publish --delete-data` works with `spacetime.json` — MEDIUM
- What changed (exact, verified): #5256 removed `.requires("name|identity")` from the `clear-database` arg in `crates/cli/src/subcommands/publish.rs`, so `spacetime publish -c=always` (alias `--delete-data`, `--clear-database`) takes the database name from `spacetime.json` like plain `spacetime publish` does.
- Before this range: bug present at v2.4.0 (the `.requires` dates from #1880); `spacetime publish -c=always` failed with `error: required arguments not provided: <name|identity>` (issue #5253).
- Headline idea: "Fresh start, zero arguments."
- One-sentence description: Wiping and republishing a database now works with your project config alone — no need to retype the database name.
- Visual idea: project folder with `spacetime.json` (`"database": "space-transactions"`); terminal `spacetime publish -c=always` → before: red `error: required arguments not provided: <name|identity>`; after: publish succeeds.
- Caveats (don't claim): it still destroys data (that's the point of `-c`); the flag needs `=` for values (`-c=always`, `-c=on-conflict`) because of `require_equals(true)`.
- Sources: PR #5256, issue #5253; `git diff v2.4.0 v2.5.0 -- crates/cli/src/subcommands/publish.rs`; `v2.5.0:crates/cli/src/common_args.rs` (`clear_database()`).

### Event tables: reshape freely on republish — LOW (could be MEDIUM for a dev audience, but risky)
- What changed (exact, verified): #5269 lets automigration accept changes to **event tables** that are rejected for normal tables: removing columns, reordering, layout-incompatible type changes. New migration step `ReschemaEventTable`, printed as "schema of event table `<name>`" (`crates/schema/src/auto_migrate/termcolor_formatter.rs`). The plan calls `ensure_disconnect_all_users()`, i.e. it is a client-breaking change (needs `--break-clients` / confirmation).
- Before this range: event tables were subject to the same automigration rules as regular tables (column removal → `AutoMigrateError::RemoveColumn`, etc.).
- Headline idea: "Event tables, reshaped on the fly."
- One-sentence description: Because event tables never keep rows, you can now remove, reorder or retype their columns on republish without wiping the database.
- Visual idea: a code diff on an event table struct (delete a field, reorder two) → `spacetime publish` shows "~ Changed schema of event table damage_event" and a "clients will be disconnected" warning → published.
- Caveats (don't claim):
  - Disconnects all clients and breaks old clients' bindings.
  - **It shipped with a regression**: dropping an event table in 2.5.0 could make the database fail to restart (commitlog replay error), fixed in v2.5.0-hotfix1 / v2.6.0 (#5288, #5289). Avoid promoting this feature in a 2.5 video, or at least don't suggest dropping event tables.
  - PR motivation was internal (ControlDB schema change); migration plan formatting is minimal (just the table name).
- Sources: PR #5269; `v2.5.0:crates/schema/src/auto_migrate.rs` (`ReschemaEventTable`, `ensure_disconnect_all_users`); `v2.5.0:crates/smoketests/tests/smoketests/auto_migration.rs` (`automigrate_reschema_event_table_arbitrarily`); PR #5288 ("a deterministic commitlog replay failure introduced in #5269").

### Faster bulk inserts (no more quadratic slowdown) — LOW
- What changed (exact, verified): #5071 stores a table's non-full pages in a `BTreeSet` sorted by free var-len granules instead of a `Vec` (`crates/table/src/pages.rs`). Fixes an accidentally-quadratic insert path for tables whose rows carry lots of variable-length data (strings/arrays), and makes the page a row lands in deterministic across restarts.
- Before this range: linear scan over non-full pages per insert; order of `non_full_pages` changed after a restart.
- Headline idea: "Big imports stay fast."
- One-sentence description: Inserting many rows with lots of text or lists no longer gets slower and slower as the table grows.
- Visual idea: a progress bar filling at constant speed vs a "before" bar that crawls near the end.
- Caveats (don't claim): no public benchmark numbers (PR cites internal TPCC experiments and "did not observe a decrease in throughput" on keynote-2). Only affects tables with var-len-heavy rows. "Deterministic insertion" is an internal property, not a user feature.
- Sources: PR #5071; v2.5.0 note "fixes accidentally-quadratic behavior during bulk inserts".

### TypeScript modules: "No such index" after restart + republish fixed (v2.4.1) — LOW
- What changed (exact, verified): #5145 (`crates/datastore/src/locking_tx_datastore/state_view.rs`): when the server rebuilds `IndexSchema` from system tables (e.g. after a restart) it now reads `st_index_accessor` to restore the index accessor name. Smoketest `typescript_index_source_name.rs`: TS module with camelCase names → restart server → republish with new indexed columns → reducers using `ctx.db.AppUsers.emailAddress.filter(...)` now work.
- Before this range: issue #4701 — TS modules whose accessor names differ from canonical snake_case names could crash with `Uncaught No such index` after publish ("The instance encountered a fatal error").
- Headline idea: "Restarts don't lose your indexes."
- One-sentence description: Fixes a crash where a TypeScript module could stop finding its own indexes after the server restarted and the module was republished.
- Visual idea: log line `Uncaught No such index` struck out → reducer call succeeds.
- Caveats (don't claim): data was never lost; it's a name-lookup bug. Most visible for TypeScript; the fix is server-side (upgrade the server/Maincloud, not the npm package).
- Sources: PR #5145, issue #4701, v2.4.1 release note.

### CLI update notice at most once a day — LOW
- What changed (exact, verified): #5184 (`crates/update/src/update_notice.rs`): "A new version of SpacetimeDB is available: vX (current: vY)" is printed at most once per 24 h per advertised version (`NOTICE_INTERVAL = 24h`), instead of on every command.
- Before this range: the notice (added in #4363, present since v2.1.0) printed on every invocation (issue #5183).
- Headline idea: "Less nagging."
- One-sentence description: The CLI now reminds you about a new version once a day instead of on every command.
- Visual idea: terminal with the yellow notice repeated on 5 commands → only on the first.
- Caveats: lives in the `spacetime` launcher (`crates/update`), so users get it once their launcher is updated.
- Sources: PR #5184, issue #5183.

### Templates pin `major.minor` versions — LOW
- What changed (exact, verified): #5228 (`crates/cli/src/subcommands/init.rs`): `spacetime init` now writes `2.5.*`-style constraints (`to_major_minor_patch_wildcard`) for `spacetimedb` (npm), `spacetimedb`/`spacetimedb-sdk` (Cargo), etc. Previously some templates got `^x.y.z`, some `2.*`, some exact versions.
- Before this range: inconsistent constraints could make a new CLI generate projects pointing to package versions not yet published.
- Headline idea: n/a (list item: "new projects always resolve").
- One-sentence description: New projects created by `spacetime init` now always point at package versions that actually exist.
- Caveats: fixes a release-timing edge case; don't frame as a feature.
- Sources: PR #5228, issue #5229.

## Left out (not user-visible or not promo-worthy)
- #5131 Improve accuracy and change semantics of metric `wasm_memory_bytes` — Maincloud billing/metrics semantics; "Expect recorded usage per database to increase". Not promo material (see doubts).
- #4930 Stop conflating EnergyQuanta and FunctionBudget — internal energy accounting; "corrects the v8 energy calculation" and changes what the `spacetime-energy-used` HTTP header carries (`FunctionBudget`). Billing internals, not promo.
- #5172 Use the same single threaded runtime for all wasm operations — internal threading (one worker thread instead of two); PR says it doesn't meaningfully change runtime behavior.
- #5016 Deterministic runtime crate — internal simulation-testing infrastructure.
- #4955 Add some tests of procedure concurrency — tests.
- #5139 Bump Angular SDK version in angular-ts template — template dependency bump within Angular 21 (`^21.1.1` → `^21.2.12`).
- #5178 Add launchpad tag to llm-chat and chat-react — website templates-page tagging only.
- #4993 Add BetterAuth tutorial — docs-only (new tutorial; could be a footnote if the video mentions docs).
- #5166 docs: consolidate outstanding docs fixes — docs-only.
- #5227 / #5216 / #5258 version bumps — release mechanics.
- #5157 Remove `cargo bump-versions`, #5143 `bump-versions` improvements — internal release tooling.
- #5181 CI - fix LLM benchmark workflows, #4817 LLM Benchmark: Sequential Upgrades Test — benchmark tooling.
- #5233, #5237 test flake fixes; #5155 Port `bindings-doctests.sh` into CI; #5146 CI `RUST_BACKTRACE=full`; #5234 CLA retry workflow; #5182 CODEOWNERS — CI/test infra.
- Small CLI help-text tweaks (all in #5166 "docs: consolidate outstanding docs fixes", commit f83d41c75c, per `git log v2.4.0..v2.5.0 -- crates/cli/src/subcommands/{generate,dev}.rs crates/update/src/cli/self_install.rs`): `spacetime generate` usage string now `generate [DATABASE] --lang <LANG> [--module-path <DIR> | --bin-path <PATH> | --js-path <PATH>] [--out-dir <DIR> | --uproject-dir <DIR>] [--unreal-module-name <MODULE_NAME>] [OPTIONS]`; `spacetime dev --client-lang` help lists `rust, unrealcpp` instead of `python`; post-install link → `https://spacetimedb.com/docs/`. Cosmetic.

## Open doubts
- The v2.5.0 release note says views remain gated behind `unstable`; the code at v2.5.0 shows no gate on `#[view]` (Rust) or `ViewAttribute` (C#). Treat the note as wrong.
- In-repo procedures docs still show the unstable opt-in at v2.5.0 and v2.6.0 — the live website may have been fixed separately; check spacetimedb.com before showing docs in the video.
- #5131 changes how Maincloud memory is billed (sum of `wasm_memory_bytes` + `v8_used_heap_size_bytes`; usage per DB expected to rise). Not something to advertise; flag to marketing in case customers ask.
- SolidJS: confirmed on the npm registry that `spacetimedb@2.5.0` exports `./solid` (`npm view spacetimedb@2.5.0 exports`). Solid benefits from the React-style auto-reconnect only from 2.6.0, and that is undocumented for Solid (see 2.6 research).
- Event-table reschema (#5269) shipped with the drop-table replay regression (fixed in hotfix1). Recommend not featuring it.
