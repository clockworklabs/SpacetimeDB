# v2.7 research (v2.6.0 → published 2.7.0 = `v2.7.0-hotfix3`)

## Range note (read first)
- The git tag `v2.7.0` (a08663c7b9, 2026-06-22) is only the "Version bump 2.7.0 (#5399)" commit on master. `v2.6.0..v2.7.0` has 27 commits.
- The **published** 2.7.0 (GitHub release "Release v2.7.0", published 2026-07-22) is tagged `v2.7.0-hotfix3` (d220349adb). It is master at 9b4b7c7451 (#5564) plus one commit, "Revert 963bec1" (it re-adds the JWT deps that #5427 removed). `v2.7.0..v2.7.0-hotfix3` has 73 more commits, and they hold most of the 2.7.0 release notes: MCP, .NET 10, the unique-constraint migration, `sql --format json`, the Svelte reconnect and the TS camelCase handles.
- `v2.7.0-hotfix4` is the same commit as hotfix3.
- So this file covers **`v2.6.0..v2.7.0-hotfix3` (100 commits)**, which is what users of 2.6.0 actually got in 2.7.0. End-state checks use `git show "v2.7.0-hotfix3:<path>"`.
- Each candidate is tagged **[in git v2.7.0]** (in the literal `v2.6.0..v2.7.0` range) or **[published-only]** (only in `v2.7.0..v2.7.0-hotfix3`). To re-split by the literal tag, move the [published-only] items to the 2.8 video. That would make 2.8 advertise things 2.7.0 users already had, so it isn't recommended.
- Items in the literal tag range: #4888 lock/unlock, #4810 Unreal query builder, #5354 C++ view PKs, #5075 TS Uuid literal, #5343 BSATN, #5331 metrics, plus the three v2.6.1 fixes (#4940, #5323, #5264).

## Releases in range
- v2.6.1, 2026-07-01 (tag `v2.6.1`, off master; 4 commits): TS generated `Option<T>` fields become optional keys (small breaking change), a fix for procedures' `ctx.sender`/`connectionId` being empty (a regression since 2.4), and `spacetime init --template` with no value lists the templates.
- v2.7.0, 2026-07-22 (GitHub release tag `v2.7.0-hotfix3`): MCP endpoint per database, `spacetime lock`/`unlock`, adding unique/primary-key constraints via automigration, .NET 10 NativeAOT-LLVM C# modules, Unreal typed query builder, Svelte `reconnect(builder)`, TS camelCase table handles, TS `onSchedule`, `spacetime sql --format json`, view PK `Find()` in C#/Unreal, C++ view PKs, Rust SDK traits, plus fixes and metrics.
- (Not GitHub releases: `v2.6.1-hotfix1..3` backported #5440, #5471, #5497, #5498, the views cleanup and the replay `ST_TABLE_ID` fix to 2.6.1, probably for Maincloud deploys. They are all inside this range anyway.)

## Candidates

### MCP server for every database (AI agents can query and call your DB): HIGH [published-only]
- What changed (exact, verified): new route `POST /v1/database/:name_or_identity/mcp` (`crates/client-api/src/routes/mcp.rs`, wired in `routes/database.rs` `.route("/mcp", self.mcp_post)`). It is MCP over plain HTTP JSON-RPC, protocol version `"2025-06-18"`, server name `spacetimedb`, and it handles `initialize`, `ping`, `tools/list` and `tools/call`. There are four tools:
  - `ping`: "Health check that echoes an optional message back." Returns `pong` or `pong: <message>`.
  - `get_schema`: "Get the schema for this database as JSON, including its typespace, tables, and reducers." Returns the `RawModuleDefV9` JSON.
  - `sql`: "Run a SQL query against this database and return the rows as JSON. Write queries require ownership of the database." Args: `sql`, optional `confirmed`.
  - `call`: "Invoke a reducer with positional JSON arguments, for example ["alice"] or [42]. The reducer runs with your identity and is the standard way to write." Args: `reducer`, `args` (array).
  - Server instructions string: "Tools for the addressed SpacetimeDB database: ping, get_schema, sql, and call. … Everything runs with your identity, exactly as over the HTTP API."
  - Auth uses the same `anon_auth_middleware` as the other `/v1/database/:x/*` routes. With a `Authorization: Bearer <token>` header the calls run as that identity. With no token, an anonymous identity is minted, as on the other routes.
- Before this range: no MCP support anywhere (`mcp.rs` doesn't exist at v2.6.0).
- Headline idea: "Your AI agent, meet your database." / "Ask your database anything."
- One-sentence description: Every SpacetimeDB database now speaks MCP, the protocol AI coding agents use for tools, so an agent can read its schema, run SQL and call reducers with your identity.
- Visual idea: an agent chat panel beside a glowing database cube. User: "What tables does my game have?" A `get_schema` chip lights up and a table list appears. User: "How many players are online?" A `sql` chip shows `SELECT * FROM player WHERE online = true` and rows stream back. User: "Give alice 100 gold." A `call` chip shows `add_gold ["alice", 100]` and the row updates live. Label the endpoint `POST /v1/database/<name>/mcp` as a small URL bar.
- Caveats (don't claim):
  - **Not on Maincloud at this point.** The v2.8.1 and v2.8.2 release notes both say "Note: SpacetimeDB MCP is not yet available for Maincloud." The v2.10.0 notes announce "MCP support on Maincloud" ("The SpacetimeDB MCP endpoint (`/v1/mcp`) is now available on Maincloud"). Show a local or self-hosted server (`localhost:3000`). The 2.7.0 notes themselves say "Standalone databases".
  - There is no `spacetime mcp` command (it arrived in v2.8.1), no host-wide `/v1/mcp` and no `list_databases`. The agent is pointed at one database URL at a time, with a bearer token configured manually.
  - There are no docs pages for it in this range (no `mcp` hits in `docs/` at the tag).
  - The agent can't create, build or publish modules through it. Only the 4 tools above exist.
  - SQL writes require owning the database. Private tables follow the normal visibility rules.
  - Transport is request/response JSON only, with no SSE/streaming GET route. I didn't test it end to end with a specific agent client (Claude Code, Cursor, …). Don't name a client as "supported" without trying it.
- Sources: #5489 (merged commit 71f7fe42b2); `git show "v2.7.0-hotfix3:crates/client-api/src/routes/mcp.rs"`; release note: "Standalone databases now expose an authenticated MCP endpoint at `POST /v1/database/:name_or_identity/mcp`. It provides tools to check health, retrieve module schemas, run SQL, and invoke reducers."

### Lock a database against deletion (`spacetime lock` / `spacetime unlock`): HIGH [in git v2.7.0]
- What changed (exact, verified):
  - New CLI commands `spacetime lock [database] [-s/--server] [--no-config]`, about text "Lock a database to prevent accidental deletion", and `spacetime unlock [database] …`, about text "Unlock a database to allow deletion". They call `POST /v1/database/<identity>/lock` and `/unlock`.
  - On success `lock` prints: `Database <identity> is now locked. It cannot be deleted until unlocked.`
  - Server side (standalone), a locked database refuses `DELETE` with 403 `"Database is locked and cannot be deleted. Run `spacetime unlock` first."`. It also refuses a reset (`spacetime publish --delete-data` / `-c`) with 403 `"Database is locked and cannot be reset with --delete-data. Run `spacetime unlock` first."`.
  - Locking requires the same permission as deleting (`Action::DeleteDatabase`). Locking twice is idempotent (smoketest).
- Before this range: not in any release. It first landed as #4502 and was reverted in #4881 (in "Revert breaking PRs") before any tag contained it. `crates/cli/src/subcommands/lock.rs` doesn't exist at v2.6.0 or v2.6.1.
- Headline idea: "Production, protected." / "Oops-proof your database."
- One-sentence description: One command locks a database so nobody can delete it or wipe its data by accident until it's explicitly unlocked.
- Visual idea: terminal scene. `spacetime lock my-game` shows a padlock snapping shut on a database icon, then the success line. `spacetime delete my-game` is blocked: red flash, padlock shakes, "403 Forbidden". Optionally `spacetime publish --delete-data my-game` gets bounced the same way. End with `spacetime unlock my-game` and the padlock opens.
- Caveats (don't claim):
  - `spacetime delete` handles errors with `response.error_for_status()` (`crates/cli/src/subcommands/delete.rs`), so the CLI probably shows reqwest's generic `HTTP status client error (403 Forbidden) for url (…)` and **not** the friendly "Database is locked…" text. Don't mock the friendly text as CLI output unless you've run it.
  - It protects against deletion and data reset only. It does **not** block publishing updates, SQL writes or reducer calls. It is not access control.
  - Verified only for standalone (`crates/standalone`). Maincloud's control plane is private code; I couldn't verify lock there (see doubts).
- Sources: #4888 (f8ccbbed7a); `v2.7.0-hotfix3:crates/cli/src/subcommands/lock.rs`, `unlock.rs`; `crates/client-api/src/routes/database.rs` lines ~865 and ~1349; `crates/smoketests/tests/smoketests/database_lock.rs`; release note: "`spacetime lock` and `spacetime unlock` prevent accidental deletion of locked databases."

### Add `unique` / `primary key` to a live table without wiping data: HIGH [published-only]
- What changed (exact, verified): adding a unique constraint (so `#[unique]` / `#[primary_key]` / TS `.unique()` / `.primaryKey()`) to a column of an existing table is now an automatic migration step (`AutoMigrateStep::AddConstraint`). The datastore makes the index unique inside the migration transaction.
  - If duplicates exist, the migration fails and nothing changes. Error format (`crates/datastore/src/locking_tx_datastore/mut_tx.rs`): `Cannot add unique constraint on table {table_id} column(s) {cols:?} ({source}):\n{N} duplicate group(s) found.\n  - {value:?} appears {count} times …`, showing up to 10 groups and then `... and more`.
  - The migration plan prints `▸ Created unique constraint <name> on [<cols>] of table <table>`.
- Before this range: automigration refused with `AutoMigrateError::AddUniqueConstraint`: "Adding a unique constraint {constraint} requires a manual migration". In practice that meant clearing the database (`v2.6.0:crates/schema/src/auto_migrate.rs` line ~428/1112).
- Headline idea: "Tighten your schema. Keep your data."
- One-sentence description: You can now mark an existing column as unique or as the primary key and republish; SpacetimeDB checks your live data and applies the rule in place, or tells you exactly which values are duplicated.
- Visual idea: before/after split. Left, "2.6": the code gains `#[unique]` on `email`, `spacetime publish` shows a red "requires a manual migration" error, and the table icon is emptied (data wiped). Right, "2.7": the same edit gives a green `▸ Created unique constraint … on [email] of table user`, and the rows stay put with a small key/lock badge on the column. Optional third beat: a duplicate `alice@example.com` row highlights red with "2 duplicate group(s) found".
- Caveats (don't claim):
  - It doesn't dedupe for you. With duplicates the publish fails, and you have to fix the data first.
  - The real error text is technical: it shows a numeric table id and debug-formatted values. The prettier message in the PR description (`'Users_email_key' on table 'Users'`) is **not** what ships. Don't copy it.
  - Changing or removing other constraint kinds isn't part of this change. Only adding a unique constraint (which primary keys imply) is.
  - The data check is a scan of the table during publish. Don't claim "instant" on big tables.
- Sources: #4465 (de4516d0d3); `v2.7.0-hotfix3:crates/schema/src/auto_migrate.rs` (~line 1124), `crates/datastore/src/locking_tx_datastore/mut_tx.rs` (~2230-2255), `crates/schema/src/auto_migrate/termcolor_formatter.rs` (~284); release note: "Adding a `#[unique]` or `#[primary_key]` constraint to an existing table is now a non-breaking migration when existing data satisfies the constraint. If duplicate data exists, the migration fails safely and identifies duplicate values."

### C# modules on .NET 10, compiled ahead-of-time (NativeAOT-LLVM): MEDIUM [published-only]
- What changed (exact, verified):
  - `SpacetimeDB.Runtime` NuGet now targets `net8.0;net10.0` (`crates/bindings-csharp/Runtime/Runtime.csproj`). A `net10.0` project automatically builds with NativeAOT-LLVM (`_UseNativeAotLlvm` in `Runtime/build/SpacetimeDB.Runtime.props/.targets`), with no `--native-aot` flag or env var needed.
  - New flag `--dotnet-version <8|10>` on `spacetime init`, `build`, `publish`, `dev` and `generate` (`crates/cli/src/common_args.rs`). Any other value fails with `Unsupported --dotnet-version {v}. Supported values: 8, 10.`
  - The `spacetime init --lang csharp` default is .NET 10, **except** on macOS or when only the .NET 8 SDK is installed (`resolve_default_dotnet_major`). It prints `Targeting .NET SDK {n}.`. On macOS it warns: "Warning: NativeAOT-LLVM does not support macOS hosts, so this C# project will target .NET 8. .NET 8 support will be deprecated soon." It writes `global.json` with `10.0.100` or `8.0.100`.
  - The existing .NET 8 paths (Mono/"wasi-experimental" JIT and opt-in `--native-aot`) remain.
- Before this range: .NET 8 only. `--native-aot` already existed at v2.6.0 (`init.rs` line ~197) as an opt-in .NET 8 AOT path. There was no .NET 10 or `--dotnet-version` anywhere in the CLI or runtime at v2.6.0.
- Headline idea: "C# modules, now on .NET 10."
- One-sentence description: New C# modules target .NET 10 by default and are compiled ahead-of-time to WebAssembly with NativeAOT-LLVM, while existing .NET 8 projects keep working unchanged.
- Visual idea: terminal. `spacetime init --lang csharp my-game` prints `Targeting .NET SDK 10.`, then a `.csproj` card flips from `net8.0` to `net10.0`, then an "AOT" compile animation (C# glyph → WASM chip).
- Rating note: MEDIUM rather than HIGH because there are no performance numbers to show and the audience is C# only. It could carry a full scene only if someone measures a speedup.
- Caveats (don't claim):
  - **No performance numbers.** The PR gives no benchmarks, so don't say "faster" or "Nx" unless measured.
  - Not on macOS hosts: `init` defaults to .NET 8 there and says NativeAOT-LLVM doesn't support macOS.
  - Don't say .NET 8 was dropped. It is still supported but warned as "will be deprecated soon".
  - Don't say "`init` now picks .NET 8 on macOS" as a later-version novelty: that behavior is from this release (#4915).
- Sources: #4915 (484f3b579e); `v2.7.0-hotfix3:crates/cli/src/subcommands/init.rs` (lines 209, 555-591, 1630-1665), `crates/cli/src/common_args.rs` 59-76; release note: "C# modules can now target .NET 10 and use NativeAOT-LLVM automatically…".

### Unreal: typed query builder in C++ and Blueprint: MEDIUM [in git v2.7.0]
- What changed (exact, verified): Unreal client subscriptions can use a generated typed query builder: `Conn->SubscriptionBuilder()->AddQuery([](const FQueryBuilder& Q){ return Q.From.ShopItems().Where([](const FShopItemsCols& Row){ return Row.RequiredLevel.Lte(5); }); })->Subscribe();`. There are also generated Blueprint nodes (source query, column, predicate, `Where`, `AddQuery`, `Subscribe`).
- Before this range: the v2.6.0 docs table (`docs/docs/00200-core-concepts/00400-subscriptions.md`) says Unreal has typed query builder "No" and uses "Query strings passed to `Subscribe(...)`". Rust, TS and C# already had client query builders.
- Headline idea: "Type-safe queries, right in Blueprint."
- One-sentence description: Unreal developers can build subscription queries from typed, autocompleting nodes and C++ helpers instead of hand-written SQL strings.
- Visual idea: Blueprint graph mock. A `From ShopItems` node wires to `Where (RequiredLevel <= 5)`, then to `AddQuery`, then to `Subscribe`. Next to it, a C++ snippet with the same query. Contrast with a red squiggly under a raw string `"SELECT * FROM shop_itemz"`.
- Caveats: raw SQL subscriptions still work. The PR notes the Unreal test harness is disabled in CI. Don't claim parity with every operator of other SDKs without checking.
- Sources: #4810 (95e61f415c); `v2.7.0-hotfix3:docs/docs/00200-core-concepts/00400-subscriptions.md` (Unreal C++ examples ~lines 170-180, 355-375); release note "Typed query builder for Unreal SDK".

### Svelte: sign in without reloading the page (`reconnect(builder)`): MEDIUM [published-only]
- What changed (exact, verified): the Svelte provider (`crates/bindings-typescript/src/svelte/SpacetimeDBProvider.ts`) now runs on the shared `ConnectionManager` and exposes `reconnect(builder)` on the context value (`connection_state.ts`). It tears down the current connection and connects with a new builder (e.g. a new token). `useTable`/`useReducer` re-bind automatically. Svelte also gains the exponential-backoff auto-reconnect that React and Solid already had.
- Before this range: Svelte used a module-level singleton connection. Swapping tokens needed `window.location.reload()` (PR #5375 text), and Svelte had no auto-reconnect.
- Headline idea: "Sign in. No reload."
- One-sentence description: Svelte apps can switch from an anonymous connection to a signed-in one without reloading the page or losing on-screen state.
- Visual idea: a browser mock of a Svelte app, a live board showing "Guest". Click "Sign in with Google": the connection line flickers and re-links, the name becomes "Alice", the board stays intact, and there's no white reload flash. Could combine with a React/Solid note ("already there, now in Svelte too").
- Caveats: Svelte only (React/Solid already had `ConnectionManager`). Small breaking change: `getConnection<MyConn>()` loses its type argument. Vue and Angular are not covered.
- Sources: #5375 (a14fb06ce4); `v2.7.0-hotfix3:crates/bindings-typescript/src/svelte/SpacetimeDBProvider.ts` lines 24-58; release note "Svelte reconnect and token swap".

### `spacetime sql --format json`: LOW [published-only]
- What changed (exact, verified): `spacetime sql` has `--format <text|json>` (default `text`; `text` also accepts aliases `default` and `txt`). With `json` it prints the raw HTTP API response JSON (statement results with schema and rows) instead of the table.
- Before this range: text table output only (no `format` arg at v2.6.0).
- Headline idea: "Script-ready SQL output."
- One-sentence description: `spacetime sql` can now print results as JSON, so scripts and tools can consume query output directly.
- Visual idea: the same query shown twice, first as a box-drawn table, then `--format json` morphing it into JSON piped into `jq`. Pairs well as a "plus" card next to the MCP scene ("built for tools and agents").
- Rating note: LOW on its own. It works best as a line inside the MCP scene ("built for tools and agents").
- Caveats: it's the raw API shape (schema plus row arrays), not a list of `{column: value}` objects. Don't mock pretty objects.
- Sources: #5459 (03e1b139a8); `v2.7.0-hotfix3:crates/cli/src/subcommands/sql.rs` lines 36-67, 190-193.

### TypeScript: split scheduled reducers across files (`onSchedule`): LOW [published-only]
- What changed (exact, verified): `spacetimedb.reducer(...)` and `spacetimedb.procedure(...)` accept an options object with `onSchedule: <scheduleTable>`, e.g. `export const repeatingTest = spacetimedb.reducer({ onSchedule: repeatingTestArgTable }, { arg: repeatingTestArg }, (ctx, { arg }) => {…})` (`modules/module-test-ts/src/index.ts` ~288). There is at most one scheduled function per schedule table. The old `table({ scheduled })` still works.
- Before this range: the schedule was declared on the table (`table({ scheduled: … })`), which forced the table file to import the reducer and created circular imports when splitting modules.
- Headline idea: "Schedules that stay tidy."
- One-sentence description: A TypeScript reducer can now declare which schedule table triggers it, so tables and reducers can live in separate files.
- Visual idea: two file tabs, `tables.ts` and `jobs.ts`. An import arrow from table to reducer gets cut, and `onSchedule: cleanupTimer` appears on the reducer.
- Caveats: TypeScript modules only. Not a new scheduling capability, just where it's declared.
- Sources: #5435 (7acce2ce4b); `v2.7.0-hotfix3:crates/bindings-typescript/src/server/reducers.ts` 23-49, `procedures.ts` 63-85.

### TypeScript: generated table handles use camelCase: LOW [published-only]
- What changed (exact, verified): generated TS client bindings expose `conn.db.loggedOutPlayer` / `tables.loggedOutPlayer` instead of `logged_out_player`. The snake_case names remain as deprecated aliases. Database canonical names are unchanged.
- Before this range: handles used the raw accessor spelling.
- Caveat: listed under "Breaking Changes" in the notes (code that enumerates handles sees both names). It's a DX consistency fix, not a feature. At most a list item.
- Sources: #5286 (84cfe5a920); release note "TypeScript: Generated table and view handles now use camelCase".

### Primary-key views everywhere (C++ modules; `Find()` in C# and Unreal clients): LOW
- What changed: C++ modules can declare primary keys on procedural views [#5354, in git v2.7.0]. Generated C# and Unreal client bindings get `Find()` on primary-key views, matching Rust and TS [#5494, published-only].
- Before: view PKs already existed for Rust, TS and C# modules (#5111, #5246, #5327, all released before this range; the 2.5 research covers procedural-view PKs). Client `Find()` on PK views existed for Rust and TS only.
- Caveat: parity work. Say "now in C++ / C# / Unreal too", not "new feature".
- Sources: #5354, #5494; release notes "Primary-key support for views in C++", "Primary-key view lookup for C# and Unreal clients".

### Small CLI touches: LOW
- `spacetime init --template` with no value prints "Available templates:", each `id - description`, "Create a project: spacetime init --template <id>" and "Browse all templates: https://spacetimedb.com/templates" [#5264, v2.6.1, in git v2.7.0]. Before, clap rejected the missing value. Interactive `init` already offered a template picker, so only the bare flag is new.
- CLI banner tagline changed from "Multiplayer at the speed of light" to **"Development at the speed of light"** (`crates/cli/src/main.rs` line 118) [#5464, published-only]. It could be a fun on-screen easter egg.
- `spacetime login` reports HTTP status errors before JSON parsing (no more `expected value at line 1 column 1` for bad `--auth-host`) [#5518, published-only].
- `spacetime start` continues if it can't check port availability (sandboxed/restricted environments) [#5564, published-only; closes #5556].
- Standalone server file logs no longer contain ANSI color codes, and `NO_COLOR` is respected [#5534, published-only].

### SDK API additions for integrators: LOW
- Rust SDK: capability traits (`CtxDbRead`, `CtxDbWrite`, `CtxWithSender`, …) [#5307], granular table traits (`TableLike`, …) [#4775], and generated `<Table>TableAccessor` marker types [#5055]. The motivation is generic integrations like `bevy_stdb`. [all published-only]
- C# SDK: `RemoteTableName` is public, for building dynamic SQL [#4714, published-only].
- Caveat: developer-facing plumbing. List only.

### Bug fixes worth a list line: LOW
- Procedures: `ctx.sender` / `ctx.connectionId` were empty inside procedures since 2.4 (#4636). Fixed in v2.6.1 [#5323, in git v2.7.0]. It's a real fix for 2.6.0 users: the regression was in 2.4.0.
- TS generated `Option<T>` fields become optional keys (`foo?: T | undefined`), so optional reducer/procedure args can be omitted [#4940, v2.6.1, in git]. The notes label it a small breaking change.
- TS modules/SDK: `Uuid` as a query-builder literal [#5075, in git]. Enum columns as PK or index no longer throw `SC.primaryKey is not a function` [#4389]. One-column prefix scan on a multi-column index no longer throws `serializeTerm is not a function` [#5428]. `Random.fill` works for empty and 32-bit typed arrays [#5115]. `Identity`/`ConnectionId`/`Timestamp`/`TimeDuration`/`Uuid` constructors coerce JSON numbers to bigint [#5041]. `Promise.withResolvers` removed for runtimes lacking it [#5384, in git].
- Unreal: overlapping subscriptions on one table no longer desync the cache (`Find()` returning empty while `Iter()` sees the row) [#5426].
- SQL DML (`INSERT`/`UPDATE`/`DELETE` via `spacetime sql`) resolves table accessor names, not just canonical names. The bug dates from 2.0, and reports started with 2.6.0 [#5478].
- HTTP reducer/procedure calls now always run `client_disconnected` (and clean up `st_client`), even when the HTTP client goes away mid-request or the call fails [#5498; repro #5496, which was not merged]. The bug looks long-standing; I didn't trace its origin. Backported in v2.6.1-hotfix3.
- Commitlog replay could dereference the wrong table when `st_table` changed. This has existed since v1.11.2/v2.2.0 [#5513]. A reliability fix, backported as a hotfix.
- Expired views are cleared in bounded batches, more aggressively when backlogged [#5503].
- BSATN decoding no longer pre-allocates from an untrusted length prefix [#5343, in git]. Don't frame it as a security vulnerability: the PR is about a proptest OOM.
- `X-Forwarded-For` with a single IP is parsed [#4839]. The PR's "HTTP 400" symptom is for axum 0.8, but the repo is on axum 0.7 at all these tags, so for users it was probably a silently ignored header. Keep it off-screen.
- View backing tables created by older versions are auto-migrated on startup [#5441]. This only repairs the schema change made by #5300 **inside this range**, so it's not news.

### Other LOW
- Nix flake works on aarch64-darwin (Apple Silicon) [#5173, published-only].

## Left out (not user-visible or not promo-worthy)
- #5399 Version bump 2.7.0: version only.
- #5331, #5358, #5400, #5436, #5495, #5547, #5553: Prometheus metrics (connection, idle timeout, memory, snapshot, websocket termination, view vs reducer metrics). Operator observability only.
- #5427 Remove `spacetimedb-jsonwebtoken`/`jwks` deps: reverted in the published 2.7.0 itself ("Revert 963bec1"), so the net change is zero.
- #5300 args column to view backing tables, #5443 view read sets by arg hash, #5449 empty arg hash for anonymous views: internal view machinery (prep for parameterized views and plan sharing).
- #5113 Move `RelationalDB` to `spacetimedb-engine`: internal refactor.
- #5440, #5471 HostController bootstrap/generation: internal, for the cloud control plane.
- #5497 read-only local `History`, #5488 commitlog trim logic, #5493 commitlog misc fixes: internal durability/replication plumbing with no user surface.
- #5466 `dst` crate lib, #5424 DST test for Engine: test infra.
- #5330, #5283, #5350, #5340, #5462, #5455, #5456, #5501, #5361, #5360: tests/smoketests/benchmarks.
- #5334, #5394, #5383, #5381, #5349, #5348, #5295, #5352, #5476, #5475, #5433, #5325, #5468, #5469, #5470, #5447, #5056, #5520, #5538, #5474, #5477: CI, release tooling, Discord notifications, build scripts.
- #5324, #5446, #5472, #5458, #5552: LLM benchmark infra.
- #5321, #5410, #5429, #5509, #5378: docs/skills/tutorial fixes (docs-only).
- #5434: regenerated case-conversion TS test bindings.

## Open doubts
1. **Tag choice**, per the range note: I used the published release (`v2.7.0-hotfix3`) rather than the literal `v2.7.0` tag. If the video must follow the literal tag, only #4888 lock, #4810 Unreal query builder, and the small items tagged [in git v2.7.0] remain, and MCP, .NET 10, the unique constraint etc. would fall into 2.8, contradicting the published 2.7.0 notes.
2. The hotfix tags `v2.7.0-hotfix1..2` weren't GitHub releases. `hotfix3` = `hotfix4` = the release. Nothing user-visible differs between hotfix3 and hotfix4 (same commit).
3. **MCP on Maincloud**: not available in this range (v2.8.1/v2.8.2 notes: "SpacetimeDB MCP is not yet available for Maincloud"; it became available in v2.10.0). Also not verified end to end with an actual agent client. The protocol is plain JSON over POST, so most HTTP MCP clients should work, but I didn't try it.
4. **Lock on Maincloud**: server enforcement is verified only in `crates/standalone` (sled `control_db`). Maincloud uses private control-plane code, so I couldn't confirm there. The CLI error text for a blocked `spacetime delete` is also unverified (probably the generic reqwest 403 message).
5. .NET 10 NativeAOT: no benchmark or size numbers exist in the PR. Any "faster" claim needs a measurement.
6. The unique-constraint error shows a numeric table id and `Debug`-formatted values. Run it once to capture the real output if the scene shows the failure case.
7. v2.7.1 has a "Restore JWT/JWKS dependencies" note (#5578). It isn't news for 2.7.0 users, since hotfix3 already contained the revert (the `Cargo.toml` JWT lines are identical at hotfix3 and v2.7.1).
