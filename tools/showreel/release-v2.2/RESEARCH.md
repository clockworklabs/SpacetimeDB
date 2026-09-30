# v2.2 research (v2.1.0 → v2.2.0)

Scope notes (read first):
- `v2.1.0` sits on an empty side commit ("Release 2.1.0", `6981f48b4b`, no file changes) whose parent `10a4779b13` is on master. So `git log v2.1.0..v2.2.0` = 120 real commits, nothing double-counted.
- **Two headline items of the v2.2.0 release notes are NOT in v2.2.0.** #4881 ("Revert breaking PRs", 2026-04-23) reverted them before the tag, and they were not re-landed in the range:
  - `spacetime lock` / `spacetime unlock` (#4502): `crates/cli/src/subcommands/lock.rs` doesn't exist at v2.2.0. It was re-landed as #4888 on 2026-06-16 and is first tagged in **v2.7.0**.
  - "Improved module panic backtraces" (#577): its patch still applies forward cleanly on the v2.2.0 tree, so it is absent. It was never re-landed (checked `git log v2.2.0..master`).
  - Also reverted by #4881 but re-landed inside the range: #4846 → #4909 (publish waits for durability), #4515 → #4897 (C# NativeAOT-LLVM), replay refactors → #4893. #2713 (axum update) stayed reverted.
- Every claim below was checked against code at `v2.1.0`/`v2.2.0` (`git show "<tag>:<path>"`), and the Windows signing claim against the published release binaries.

## Releases in range
- `v2.2.0` — 2026-05-02 (tag commit 2026-04-30). Only release in the range; no patch releases between v2.1.0 and v2.2.0. Release-note themes: faster realtime transport (v3 WebSocket, TS SDK default), "safer production database operations" (lock/unlock — **not actually shipped**, see scope notes — plus delete confirmation, names in `spacetime list`, selective `--yes`), Astro template and React `useProcedure`, smoother schema evolution (drop empty tables, primary-key change fix), table `clear()`, bytes-key B-tree indexes, JS-module crash resistance, `autoinc` persistence fix, code-signed Windows binaries, durability hardening, Unreal duplicate-`OnInsert` fix.

## Candidates

### Remove a table without wiping the database (+ `clear()`) — HIGH
- What changed (exact, verified):
  - #4593: auto-migration now plans `AutoMigrateStep::RemoveTable` instead of failing. At execution (`crates/core/src/db/update.rs:144-156` at v2.2.0) it checks the row count: if the table is empty it logs "Dropping table `<name>`" and drops it with its indexes, constraints and sequences; if not, publish fails with: ``Cannot remove table `<name>`: table contains data. Clear the table's rows (e.g. via a reducer) before removing it from your schema.`` The migration plan printout shows the line `▸ Removed table: <name>` (`format_remove_table`, `termcolor_formatter.rs:234`; prefix from `write_action_prefix`, "Removed" in bold colour, line 439).
  - #4729: new module API to empty a table in one call, returning the number of rows deleted: Rust `fn clear(&self) -> u64` on the `Table` trait (`crates/bindings/src/table.rs:120`, e.g. `ctx.db.old_scores().clear()`), TypeScript `clear(): bigint` (`crates/bindings-typescript/src/lib/table.ts:277`), C# `ulong Clear()` (`Runtime/Internal/ITable.cs`), C++ `uint64_t clear()` (`bindings-cpp/include/spacetimedb/table.h`). Backed by a new host call `datastore_clear`.
- Before this range: removing a table from the schema always failed with "Removing the table <name> requires a manual migration" (`AutoMigrateError::RemoveTable`, `crates/schema/src/auto_migrate.rs:409` at v2.1.0). The way out was `--delete-data`, which clears the **whole database**. There was no `clear()`; you had to delete rows one by one.
- Headline idea: "Delete a table, keep the rest."
- One-sentence description: You can now remove a table from your schema and republish: once it's empty SpacetimeDB drops it, and a new `clear()` call empties it in one line.
- Visual idea: two-step scene. Step 1, code in a reducer: `let n = ctx.db.legacy_items().clear(); log::info!("cleared {n} rows");` → the log line appears in `spacetime logs` (the count is `clear()`'s return value logged by the module, not CLI output). Step 2: the `legacy_items` table definition is deleted from the module file; `spacetime publish` prints the plan line `▸ Removed table: legacy_items`; other tables (players, scores) stay intact beside it. Before/after timeline: old path ends in a red "`--delete-data`: everything wiped".
- Caveats (don't claim): only **empty** tables can be dropped; data is never silently deleted. Removing a table adds a `DisconnectAllUsers` step (`auto_migrate.rs:655-656`), so it counts as a breaking change: publish asks to confirm breaking clients (or needs `--break-clients` / `--yes=break-clients`), and connected clients are disconnected. `clear()` needs a v2.2+ server (new host import), so a module using it can't be published to an older self-hosted server. Removing columns still requires a manual migration.
- Sources: #4593, #4729; paths above at v2.1.0/v2.2.0; v2.2.0 release note "Smoother schema evolution" and "Modules can now clear tables directly from Rust, C#, C++, and TypeScript".

### Faster realtime for TypeScript apps (v3 WebSocket transport) — HIGH
- What changed (exact, verified):
  - #4761 (server) adds WebSocket subprotocol `v3.bsatn.spacetimedb`: a thin framing layer where one binary WebSocket message carries one or more v2 messages, "so long as logical order is preserved" (`crates/client-api-messages/src/websocket/v3.rs` at v2.2.0). v2 clients are unchanged.
  - #4784 (TS SDK) asks for `['v3.bsatn.spacetimedb', 'v2.bsatn.spacetimedb']` (`PREFERRED_WS_PROTOCOLS` in `crates/bindings-typescript/src/sdk/websocket_protocols.ts`), falls back to v2 on older servers, and coalesces outgoing messages sent in the same tick into one frame (flush via `queueMicrotask`, frames capped at 256 KiB, `MAX_V3_OUTBOUND_FRAME_BYTES`, `db_connection_impl.ts:131`).
  - Hot-path work in the same release: TS SDK caches encoded reducer/procedure names and drains inbound messages in a loop instead of a promise chain (#4744, #4640); JS modules run on one long-lived worker thread per module fed by a FIFO queue instead of a pool of isolates (#4663) and the queue hand-off is no longer a rendezvous (#4704); durability workers merged to avoid per-transaction wake-ups (#4767); subscription fan-out worker "lingers" instead of parking (#4805); V8 heap stats cached (#4778); index scans on BSATN without building `AlgebraicValue`s (#4311); event-table insert path (#4310).
- Before this range: every client message was its own WebSocket frame (v2 only); JS reducers borrowed isolates from a pool across OS threads.
- Headline idea: "More messages, fewer frames."
- One-sentence description: The TypeScript SDK now bundles bursts of calls into single network messages, and the server-side JavaScript runtime got a round of speed work.
- Visual idea: a pipe between browser and server; before, many small packets each with its own envelope; after, the same calls ride in a few bigger envelopes. A throughput needle climbing, without a specific number.
- Caveats (don't claim): HIGH only if the scene works without numbers. **No official before/after numbers** were published for 2.2 (none in the release note, `templates/keynote-2/README.md` or docs). PR authors report local measurements on an Apple M2 with the keynote-2 benchmark: JS module 50K → 85K TPS (#4663) and TS client 100K → 130K TPS (#4744); if used at all, attribute them as such. Only the **TypeScript** SDK uses v3; Rust, C# and Unreal SDKs still use `v2.bsatn.spacetimedb` at v2.2.0. Batching is **client → server**; the server's v3 path sends its messages with the v2 serialization and was not seen coalescing outbound messages. The single-worker JS change (#4663) introduced heap-growth/OOM risk that #4684 and #4746 fixed inside the range, so don't list those as separate wins.
- Sources: #4761, #4784 (re-open of #4762), #4744, #4640, #4663, #4704, #4767, #4805, #4778, #4311, #4310; paths above at v2.2.0; release note "Faster realtime transport and client throughput".

### Safer `spacetime delete`, clearer `spacetime list`, precise `--yes` — HIGH
- What changed (exact, verified):
  - #4770: `spacetime delete` always asks `Are you sure you want to delete database <name> (<identity>)? This action cannot be undone. [y/N]`; anything but `y`/`yes` prints `Aborting` (`crates/cli/src/subcommands/delete.rs:54-60`). `--yes`/`-y` skips it ("Skipping confirmation due to --yes").
  - #4769: `spacetime list` shows a two-column table `Database Name(s) | Identity` (`list.rs:31-37`), names found by reverse DNS lookup.
  - #4885: `spacetime publish --yes` takes optional values: `--yes` (= `--yes=all`), or one or more of `remote`, `migrate`, `break-clients`, `skip-login`, `delete-data`, comma-separated (`--yes=migrate,break-clients`) or repeated. The value must be attached with `=` ("so `--yes my-db` treats `my-db` as the database name"). `--break-clients` is documented as equivalent to `--yes=break-clients`.
- Before this range: a plain `spacetime delete` executed immediately (only deleting a database with child databases asked; `delete.rs` at v2.1.0). `spacetime list` showed identities only. `--yes` on publish was all-or-nothing.
- Headline idea: "Measure twice. Delete once."
- One-sentence description: Deleting a database now asks you to confirm, `spacetime list` shows names instead of just hex IDs, and CI scripts can skip only the prompts they mean to.
- Visual idea: terminal. `spacetime list` → a clean table with `my-game-prod`, `my-game-dev` next to identities. `spacetime delete my-game-prod` → the red confirmation line with `[y/N]`; user presses Enter → `Aborting`. Final beat: a CI script line `spacetime publish --yes=migrate`.
- Caveats (don't claim): HIGH as one combined "safety" scene; each piece alone is a card. **Do not show or mention `spacetime lock`/`unlock`** — not in 2.2.0 (first in v2.7.0) even though the release note says so. `spacetime list` code differs from the PR text: an unnamed database shows an empty name cell (not `(unnamed)`) and a failed lookup fails the whole command (no `(lookup failed)`). `delete` now reads stdin: in CI stdin is usually closed, `read_line` returns empty, the command prints `Aborting` and exits 0, so scripts that don't pass `-y` silently stop deleting while reporting success. The `--yes=<values>` form exists only on `publish`; `delete` and others keep boolean `--yes`. `spacetime list` still prints the `UNSTABLE_WARNING` banner.
- Sources: #4770, #4769, #4885, #4881, #4888; `crates/cli/src/subcommands/{delete,list,publish}.rs` at v2.1.0/v2.2.0, `crates/cli/src/util.rs:310` (`y_or_n`); release note "Safer production database operations".

### React: `useProcedure` and pausable `useTable` — MEDIUM
- What changed (exact, verified): #4752 adds `useProcedure` to `spacetimedb/react` (`crates/bindings-typescript/src/react/index.ts:5`); TS codegen now also emits `export const procedures = __convertToAccessorMap(...)` in module bindings (`crates/codegen/src/typescript.rs:305`; absent at v2.1.0). Usage from the PR: `const doSomeThing = useProcedure(procedures.doSomeThing); const result = await doSomeThing({ foo: "..." });` Calls made before the connection is ready are queued and flushed when it connects. #4721 adds `enabled?: boolean` (default `true`) to React `useTable`'s options: `useTable(tables.messages, { enabled: isChatOpen })`; when `false` it returns `[[], true]` and doesn't subscribe.
- Before this range: React apps could call procedures only through the connection object (`getConnection().procedures.x(...)`), without a typed hook or an exported `procedures` map; `useTable` always subscribed while mounted.
- Headline idea: "Procedures, the React way."
- One-sentence description: React apps get a typed `useProcedure` hook that works like `useReducer`, and `useTable` subscriptions can be switched on and off with one flag.
- Visual idea: a React component in an editor: `const askAi = useProcedure(procedures.askAi)` → button click → awaited result renders. Second beat: a chat panel toggle; `enabled: isChatOpen` flips and the subscription indicator turns on/off.
- Caveats (don't claim): React only (not Vue/Svelte/TanStack/Angular). Procedures themselves aren't new.
- Sources: #4752 (closes #4751), #4721; files above at v2.2.0; release note "Better TypeScript app ergonomics".

### Astro template — MEDIUM
- What changed (exact, verified): #4688 adds `templates/astro-ts` (`.template.json`: "Astro app with a TypeScript server module", `client_framework: "Astro"`; built with astro, react, react-dom) with Astro SSR, a React island for realtime updates and a `server:defer` example, plus quickstart `docs/docs/00100-intro/00200-quickstarts/00152-astro.md` (`spacetime dev --template astro-ts`). Templates are auto-discovered from `templates/*/.template.json` at CLI build time (`crates/cli/build.rs:225-247`), so it also appears in the interactive picker as `TypeScript/Astro (1 template)`.
- Before this range: no Astro template (not in `templates/` at v2.1.0).
- Headline idea: "Astro, live."
- One-sentence description: A new starter template pairs an Astro site, rendered on the server, with a live React island that updates in real time.
- Visual idea: `spacetime dev --template astro-ts` → a static-looking Astro page where one island's list updates live as rows are inserted from a second window.
- Caveats (don't claim): community-contributed (first-time contributor @leovoon). The PR says it wasn't added to the `spacetime dev` docs' template list; it is still selectable because discovery is automatic.
- Sources: #4688; `templates/astro-ts/.template.json` and quickstart at v2.2.0.

### Signed Windows binaries — MEDIUM
- What changed (exact, verified): #4906 signs the Windows executables on tag builds with a DigiCert KeyLocker EV certificate (`smctl sign`). Verified on the published assets: in v2.2.0 `spacetimedb-cli.exe`, `spacetimedb-standalone.exe` (inside `spacetime-x86_64-pc-windows-msvc.zip`) and `spacetimedb-update-x86_64-pc-windows-msvc.exe` carry an Authenticode signature whose signer is `CN=Clockwork Laboratories, Inc.` (issuer "DigiCert Trusted G4 Code Signing RSA4096 SHA384 2021 CA1", EV attributes present). The same files in v2.0.3, v2.0.4 and v2.1.0 are unsigned.
- Before this range: unsigned, despite #4473 (v2.0.4) adding a signing job.
- Headline idea: "Verified on Windows."
- One-sentence description: SpacetimeDB's Windows downloads are now digitally signed by Clockwork Labs, so Windows can verify where they came from.
- Visual idea: Windows file-properties "Digital Signatures" tab showing "Clockwork Laboratories, Inc."; or an installer launching without the scary unknown-publisher prompt (see caveat).
- Caveats (don't claim): don't promise "no SmartScreen warning ever" — reputation behaviour wasn't tested. Only Windows binaries; macOS/Linux unchanged.
- Sources: #4906, #4473; release assets of v2.0.3/v2.0.4/v2.1.0/v2.2.0 inspected (PE security directory + `openssl pkcs7 -print_certs`); release note "Windows CLI binaries are now code-signed".

### Your connect rules now guard HTTP SQL too — MEDIUM
- What changed (exact, verified): #4563: the HTTP SQL endpoint `/v1/database/:name_or_identity/sql` (used by `spacetime sql`) now runs the module's `client_connected` reducer before the query and `client_disconnected` after. If `client_connected` rejects, the request returns **403 Forbidden** and the query doesn't run (`crates/client-api/src/routes/database.rs:541-550`, `client_connected_error_to_response` at v2.2.0).
- Before this range: `/sql` skipped the connect hook, while `/call` already ran it.
- Headline idea: "One gate for every door."
- One-sentence description: If your module refuses a connection in its `client_connected` hook, that refusal now also applies to SQL queries over HTTP.
- Visual idea: a module's `onConnect` code rejecting an identity; a WebSocket client bounces off; then a `spacetime sql` query from the same identity bounces off with 403.
- Caveats (don't claim): the Postgres wire protocol path (`sql_direct`) is unchanged and does **not** run the hook. The existing SQL authorization rules still apply as before; this adds a check, it isn't a new permission system. Side effect: modules that reject unknown identities now also block `spacetime sql` for them.
- Sources: #4563; `database.rs` at v2.2.0.

### Unreal: builds on Mac, no duplicate inserts — MEDIUM
- What changed (exact, verified from diffs/PRs): #4712 wraps `Builtins.h` in `#pragma push_macro("Nil")`/`#undef Nil`/`pop_macro` so projects using the SDK compile on macOS (Objective-C++ defines `Nil`, colliding with `FSpacetimeDBUuid::Nil()`); `Nil()` existed at 2.0.0 and v2.1.0, so macOS builds of the Unreal SDK failed before 2.2. #4903 stops spurious `OnInsert` for rows already in the cache when overlapping subscriptions bump a refcount. #4835 switches connection ticking from `FTickableGameObject` to `FTSTicker` (initialisation-order bug). #4861 makes the SDK test suite run on Mac.
- Before this range: macOS UE builds failed at `Builtins.h`; overlapping subscriptions could fire duplicate `OnInsert`.
- Headline idea: "Unreal on Mac, too."
- One-sentence description: The Unreal SDK now compiles on macOS, and overlapping subscriptions no longer fire duplicate insert events.
- Visual idea: Unreal Editor on a Mac building a Blackholio project successfully; an event log where one `OnInsert` appears instead of two.
- Caveats (don't claim): don't say "Unreal now supports Mac" as a platform launch; it's a build fix plus Mac test runs.
- Sources: #4712, #4903, #4835 (copy of #4006), #4861; `grep push_macro` in `sdks/unreal/.../Public/Types/Builtins.h` at v2.1.0 (0) vs v2.2.0 (1); release note bullet on #4903.

### Schema-change fixes: primary keys and auto-increment — LOW
- #4666: removing or changing a `#[primary_key]` succeeded once but broke the **next** publish with `Primary key mismatch` (`ensure_eq!` in `crates/schema/src/schema.rs:1053` at v2.1.0). Now a `ChangePrimaryKey` migration step updates the stored schema. Pre-existing bug (#3934).
- #4902: after a migration that added columns (`add_columns_to_table`, since #3230 in 1.5), `autoinc` counters were only kept in memory and reset after a restart; now persisted. Pre-existing since 1.5.
- Caveat: bug fixes, not features; don't imply primary-key changes were impossible before. Could be a subtitle in the schema scene.

### JavaScript modules: less likely to crash the server on out-of-memory — LOW
- What changed: #4777 registers a V8 near-heap-limit callback: on approaching the limit it terminates the running call and temporarily doubles the limit instead of letting V8 abort the whole process (`crates/core/src/host/v8/mod.rs:1797-1816` at v2.2.0).
- Caveats (don't claim): the PR itself hedges ("This should hopefully fix…"). #4684 (isolate rotation) and #4746 (per-call `HandleScope`) fix heap growth introduced by #4663's single long-lived worker **inside this range**, so they aren't news versus 2.1. #4684 states the crash risk isn't removed entirely. Keep as a list line, not a scene.
- Sources: #4777, #4684, #4746, #4663.

### Other user-visible changes — LOW
- `Timestamp` usable as an index filter value (Rust `FilterableValue`, `crates/lib/src/filterable_value.rs`) and in C#/TS query builders (#4693).
- Multi-column B-tree indexes use a byte-string key encoding (#4733) — mainly an optimisation; it also fixes pre-existing wrong results for `Excluded` range bounds on multi-column indexes. The release note's "more capable multi-column range scans" overstates it.
- TypeScript modules: `fetch()` responses in procedures now include response headers (were always empty, #4691); falsy column defaults (`0`, `''`, `false`) no longer ignored (#4838); single-column B-tree `filter`/`delete` accept `Range` (crashed before, #4737); codegen "access before initialization" fix (#4709); `AuthCtx`/`JwtClaims` exported from `spacetimedb/server` (#4649).
- TS client: `withCompression('brotli')` accepted where the runtime's `DecompressionStream` supports it (#4561).
- C# views may return `IEnumerable<T>` (#4486).
- SQL: negative numbers in `INSERT … VALUES (-100.0)` (#4660); Postgres wire returns execution metrics instead of empty rows for non-`SELECT` statements (#3771).
- Config errors name the bad file, e.g. `config file …/cli.toml is invalid` + TOML line/column (#4815). `spacetime start --listen-addr` help now shows the real default `0.0.0.0:3000` (#4812). `--server-issued-login` hidden from `spacetime login --help` (#4905).
- `spacetime dev <db> --template … --server maincloud` into a new folder no longer fails with "Module directory does not exist" (#4809).
- Publishing an update now waits until the update is durable before returning (#4909, re-land of #4846).
- Local durability hardening: snapshot files, `metadata.toml` and the pid file are fsynced / replaced atomically (#4891, #4892, #4890).
- Suspended/bootstrapping databases return 503 instead of 500 (#4918); error responses carry the right `request_id` (#3368).
- C# NativeAOT-LLVM path updated; `spacetime publish --native-aot` added with help "Use NativeAOT-LLVM compilation for C# modules (experimental, Windows only)" (#4897 re-land of #4515; the PR says it also fixed Linux x64 — conflicting, keep it out of the video).
- Quickstart templates ship `.gitignore` files (#4609).

## Left out (not user-visible or not promo-worthy)
<!-- generated from the full PR list of v2.1.0..v2.2.0 minus the candidates above; reasons annotated -->
- #4916 Version bump 2.2.0 — version bump
- #4450 Case conversion tests — tests only
- #4919 Update the C++ smoketest to use `latest` — tests only
- #2164 WIP: Start SDK tests for delete_all_by_eq_bsatn — tests (C# change is a comment typo)
- #4896 Remove Python smoketests — tests only
- #4893 Re-land Replay extraction PRs — internal refactor re-land
- #3999 align UUID reducer examples with allowed signatures — doc-comment examples only
- #3654 Include location in nginx config for logs specific directives — docs only
- #4884 Properly handle execution time<->energy conversion in v8 host — JS-module energy accounting conversion; billing internals, not promo
- #4231 CI - Use new internal test inputs — CI / repo tooling, not user-visible
- #4834 Add some tests and metadata to LockedFile — internal (lock-file error now includes pid/timestamp)
- #4881 Revert breaking PRs — reverted #4850 #4849 #4515 #577 #4846 #4502 #4807 #2713 #4804 before the tag (#4846→#4909, #4515→#4897 and the replay PRs→#4893 were re-landed; #4502 and #577 were not)
- #4876 bindings: reuse panic message for datastore_index_scan_point_bsatn — internal panic-message reuse
- #4850 Finish refactoring out replay — internal refactor (reverted by #4881, re-landed via #4893)
- #4782 Indices: house keeping (privatize stuff + fix minor bug in `insert_index`) — internal housekeeping
- #4873 Smoketests - Fix another name collision — tests only
- #4871 CI - Move simple jobs into `cargo ci` — CI / repo tooling, not user-visible
- #4874 CI - `cargo ci update-flow` runs on Windows — CI / repo tooling, not user-visible
- #4869 CI - Merge workflow files — CI / repo tooling, not user-visible
- #4868 CI - Move the DLL updating code into a function — CI / repo tooling, not user-visible
- #4860 `cargo ci self-docs` uses doc comments as well as explicit helptext — CI / repo tooling, not user-visible
- #4224 CI - Merge hooks — CI / repo tooling, not user-visible
- #4856 CI - Fold typescript lint into `cargo ci lint` — CI / repo tooling, not user-visible
- #4849 Replay: some code motion & reuse `ReplayCommittedState` — internal refactor (reverted by #4881, re-landed via #4893)
- #4515 Update NativeAOT-LLVM infrastructure to current ABI — reverted by #4881; re-landed as #4897 (listed under LOW)
- #4840 Add `sdk-test-procedure-cs` test module — tests only
- #4855 CI - move `pnpm build` into `cargo ci test` — CI / repo tooling, not user-visible
- #4854 CI - Move the `git diff` check under `cargo ci smoketests` — CI / repo tooling, not user-visible
- #4853 Add `cargo lint` as alias for `cargo ci lint` — CI / repo tooling, not user-visible
- #577 Better module backtraces for panics and whatnot — NOT IN v2.2.0 despite the release notes ("Improved module panic backtraces"): reverted by #4881 and never re-landed (patch still applies forward on v2.2.0)
- #4846 Wait for database update to become durable — reverted by #4881; re-landed as #4909 (listed under LOW)
- #3233 Adds a non-repeating scheduled reducer test — tests only
- #4502 Add `spacetime lock/unlock` to prevent accidental database deletion — NOT IN v2.2.0 despite the release notes: reverted by #4881 before the tag; re-landed as #4888 (2026-06-16), first tagged in v2.7.0. `crates/cli/src/subcommands/lock.rs` does not exist at v2.2.0
- #1125 Add a test for #1121 — tests only
- #4768 docs(fix): add missing `ctx.db` — docs only
- #4624 Clean up keynote-2 template README & DEVELOP — benchmark tooling, not shipped to users
- #4779 move 00300-spacetime-json.md into the right docs folder — docs only
- #4278 Windows VM runner test — CI / repo tooling, not user-visible
- #4836 Make Tyler a codeowner for CI and tools — repo admin
- #4807 Move field `replay_table_updated` to `ReplayCommittedState` — internal refactor (reverted by #4881, re-landed via #4893)
- #2713 Update axum — dependency update, reverted by #4881; internal anyway
- #4748 fix: reorder Vue component — cosmetic reorder in the vue-ts template
- #4808 durability: Simplify shutdown — internal shutdown simplification
- #4668 docs: Add commitlog reference document — docs only
- #2938 Add internal docs for C# bindings packages — docs only
- #3665 Unity tutorial part 2 CLI call fix — docs only
- #4804 Extract replay stuff out of `CommittedState`, part 1 — internal refactor (reverted by #4881, re-landed via #4893)
- #4269 CI - rust smoketests lib expansion — CI / repo tooling, not user-visible
- #2898 Add TESTING.md, which documents some of our testing — tests only
- #4802 durability: Use `async-channel` to allow blocking send — internal channel change
- #4801 Record metrics periodically in batches — internal (metrics recording batched off the hot path)
- #4783 Increase timeout for typescript query builder tests — test timeout only
- #4696 fix: Replace unwrap with proper error handling in WebSocket subscribe handler — rare server robustness fix (panic → HTTP 500)
- #4760 Bump the esm (gzip) package size again — CI size-limit check
- #4759 Improve a test with new `TableIndex::iter` & simplify index iterator defs — internal
- #4694 core: Enable instrumentation of multiple tokio runtimes — internal instrumentation
- #4757 Remove warmup from distributed keynote bench — benchmark tooling, not shipped to users
- #4753 Remove rust client from keynote bench — benchmark tooling, not shipped to users
- #4745 Update client defaults in keynote bench — benchmark tooling, not shipped to users
- #4743 Configure compression for keynote benchmark — benchmark tooling, not shipped to users
- #4676 Stop setting core affinity on macos — no behavior change (PR: was already a no-op)
- #4606 docs: Add supported index key types to Index docs page — docs only
- #4702 Add more context around some errors — internal error context
- #4652 core: Bounded channel for durability worker — internal backpressure on the durability queue
- #4703 fix(keynote-2): split demo and bench CLI parsing — benchmark tooling, not shipped to users
- #4698 Add distributed typescript benchmark harness — benchmark tooling, not shipped to users
- #4647 Improve benchmark cli, make compatible with deno — benchmark tooling, not shipped to users

## Open doubts
- **Release notes vs tag**: the v2.2.0 notes advertise `spacetime lock/unlock` (#4502) and better panic backtraces (#577); neither is in v2.2.0 (reverted by #4881). lock/unlock belongs to the v2.7 video. Anyone reusing the 2.2 release-note copy must drop both.
- **Performance numbers**: only PR-author measurements on one laptop exist (#4663, #4744). Without an official benchmark, keep the perf scene qualitative or skip it.
- **v3 server-side batching**: the protocol allows the server to coalesce messages, but at v2.2.0 no outbound coalescing for v3 was found (messages go through the v2 serializer). The claim is limited to TS client → server batching.
- **Windows SmartScreen**: signing verified, the SmartScreen effect not tested.
- **#4884** (execution-time ↔ energy conversion for JS modules) may change Maincloud energy accounting for TypeScript modules; left out as billing internals, not checked against pricing.
- **`--native-aot`** help says "Windows only" while #4515 claims a Linux x64 fix; not resolved.
- **Durable publish (#4909)**: the PR says the wait can't be opted out of, only the timeout extended; no user-facing flag for it was checked.
