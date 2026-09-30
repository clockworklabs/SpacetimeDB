# v2.0 research (v1.12.0 → v2.0.0)

Scope notes (read first; the tag layout is unusual):
- `v1.12.0` is **not** an ancestor of `v2.0.0`. The `v1.12.0` tag sits on a side commit (`92fdc93b95 Revert "Organizations (#4087)"`) on top of `1e482104e3 Release v1.12.0`. The merge-base is `56d7cc8fa8 Bump version to v1.12.0 attempt #2 (#4164)`. Consequence: **Organizations (#4087) was in the 1.12 release notes but reverted in the shipped v1.12.0 binary**. It *is* in `v2.0.0` (not reverted on master), so for users it first appears in 2.0.
- `v2.0.0` is **not on master** either. It is the tip of the `shub/case-conversion` feature branch (commit `4d2fe47490`, 2026-02-19) = master up to #4355 plus the unmerged "Case conversion" PR (#4263, merged to master 2026-02-20). There is **no GitHub release object** for `v2.0.0` (`gh release view v2.0.0` → "release not found").
- The first public 2.x release is **`v2.0.1`** (GitHub release titled "SpacetimeDB 2.0", 2026-02-20; tag commit 2026-02-24). `v2.0.0` is not an ancestor of `v2.0.1` either; `v2.0.1` = master, which contains everything in `v2.0.0` plus 47 more commits (see "Lands only in v2.0.1" below). The 2.0 release notes and the migration guide (#4383) describe `v2.0.1`, not the `v2.0.0` tag.
- `git log v1.12.0..v2.0.0` = 285 commits, of which 134 are squash-merged PRs; the remainder are the individual commits of the case-conversion branch (#4263).
- Every claim below was checked with `git show "<tag>:<path>"` at `v1.12.0` and `v2.0.0` (and `v2.0.1` where noted). Each candidate is tagged **[in v2.0.0]** or **[v2.0.1 only]**.

## Releases in range
- `v1.12.0-hotfix1` — tag only (no GitHub release), 2026-02-11. One commit: "Fix release for GLIBC_2.38 issue" (#4268, build fix). The same PR is also in the 2.0 range.
- `v2.0.0-candidate` (2026-02-16), `v2.0.0` (2026-02-19), `v2.0.0-prerelease` (2026-02-20), `v2.0.0-rc1` (2026-02-20) — tags only, no GitHub releases.
- `v2.0.1` — GitHub release "SpacetimeDB 2.0", published 2026-02-20. This is the 2.0 launch post: TypeScript out of beta, framework integrations, Unreal/C++, performance, Maincloud free tier and pricing, procedures, SpacetimeAuth, views, typed query builder, event tables, `spacetime.json`, `spacetime dev`, dashboards, LLM tooling, collaborators/organizations, Postgres wire protocol. **Many of these headlines shipped in 1.x** (see the table below). Its "What's Changed" list covers `v1.12.0..v2.0.1`.
- For context, the `v1.12.0` release note says: "SpacetimeDB 2.0 will come with code breaking changes (you may need to change your modules and clients), but it will not require a migration of the existing data directory, and include backwards compatibility with existing clients and modules".

### 2.0 release-note headlines that predate the range (do not present these as new in 2.0)
| Headline in the 2.0 notes | Actually first shipped | Evidence |
|---|---|---|
| TypeScript/JavaScript modules | v1.6.0 "TypeScript Modules (Beta)" | GitHub release title. The "BETA" warning was still printed by `spacetime publish` at `v2.0.0` (`publish.rs:527`, "JavaScript / TypeScript support is currently in BETA."). It was removed only in `v2.0.1` (#4396). |
| React / Vue / Svelte integrations | React before 1.12; Vue (#4037) and Svelte (#4063) in v1.12.0 | `templates/react-ts`, `vue-ts`, `svelte-ts` and `spacetimedb/react`/`vue`/`svelte` exports exist at `v1.12.0`. |
| Unreal Engine SDK | #3223 (2025-09-19), first in v1.4.0 | `sdks/unreal` exists at `v1.12.0`. |
| Postgres wire protocol | #2702 (2025-09-10), first in v1.4.0 | `crates/pg` exists at `v1.12.0`. |
| `spacetime dev` | v1.7.0 | GitHub release title. What's new in 2.0 is that it runs your client too (see below). |
| View functions | v1.8.0 | GitHub release title. Views returning a query (`ctx.from.person.build()`) were already at `v1.12.0` (`crates/bindings-typescript/src/server/view.test-d.ts`). |
| Project collaborators | v1.9.0 | GitHub release title. |
| Procedures / HTTP calls | v1.10.0 | GitHub release title. At `v2.0.0` procedures are still documented as beta (`docs/.../00500-faq.md:93`). |
| Typed query builder | v1.11.0 (module side), v1.12.0 (clients, with `.build()`) | GitHub releases. |
| SpacetimeAuth, Auth0 and Clerk docs | before 1.12.0 | `docs/docs/00200-core-concepts/00500-authentication/00100-spacetimeauth/*`, `00200-Auth0.md` and `00300-Clerk.md` exist at `v1.12.0`. |
| AGENTS.md / CLAUDE.md / Cursor rules in new projects | before 1.12.0 | `crates/cli/src/subcommands/init.rs` at `v1.12.0`, around lines 1669–1671, documents writing `.cursor/rules/`, `CLAUDE.md` and `AGENTS.md`. |
| Free tier, pricing, Spacerace referrals, new dashboards/metrics UI, Team tier | Not in this repo (Maincloud/website) | Cannot be verified here. |

## Candidates

### Event tables — HIGH
- Range: in v2.0.0
- What changed (exact, verified): #4217 (plus #4251 for tests, migration validation and a bootstrap fix) adds a new table kind whose rows exist only inside the transaction that inserts them. Subscribers receive each inserted row as an `onInsert` event. The rows are never merged into the table's committed state.
  - Rust module: `#[spacetimedb::table(accessor = damage_event, public, event)]` (`event` keyword parsed in `crates/bindings-macro/src/table.rs:148`).
  - TypeScript module: `table({ public: true, event: true }, { ... })` (`event?: boolean` in `crates/bindings-typescript/src/lib/table.ts:185`).
  - C# module: `[SpacetimeDB.Table(Event = true)]` (`public bool Event` in `crates/bindings-csharp/Runtime/Attrs.cs:70`).
  - Clients (Rust, TS, C#): only insert callbacks are generated. `count()` is 0 and `iter()` is empty. C# uses `RemoteEventTableHandle`, and Rust has an `EventTable` trait.
  - Event tables must be subscribed to explicitly (excluded from `subscribe_to_all_tables` and `SELECT * FROM *`). Changing a table between event and non-event is rejected by migration validation.
- Before this range: did not exist. The 1.x way to tell other clients "something happened" was global reducer callbacks (`conn.reducers.onDealDamage(...)`), which 2.0 removes (see next item).
- Headline idea: "Fire events. Nothing to clean up." / "Broadcast moments, not rows."
- One-sentence description: Mark a table as an event table, and every row you insert becomes a real-time event that subscribed clients receive instantly, with nothing left in the table afterwards.
- Visual idea: split screen. On the left, server code `ctx.db.damageEvent.insert({ entity_id, damage: 42, source: "melee_attack" })`. On the right, three game clients each pop a floating "-42" as `onInsert` fires. The table row count under the code stays at "0 rows" throughout. Optional before/after: a 1.x table that fills up with rows next to a 2.0 event table that stays empty.
- Caveats (don't claim):
  - Don't say events are "never stored". Per #4217 the rows are still written to the commitlog; they just aren't kept in the table. The release note's "saves you storage costs" is loose, so avoid storage claims.
  - Event tables can't be used in subscription joins or views, and have no `on_delete`/`on_update` (#4217 "Deferred").
  - Not available for C++ modules (they are pinned to 1.12) or the Unreal SDK (#4217 "Deferred: C++ SDK support").
  - Clients must use the v2 protocol. v1 WebSocket subscriptions to event tables are rejected (#4217).
- Sources: #4217, #4251, #4322 (docs); `docs/docs/00200-core-concepts/00300-tables/00550-event-tables.md` at `v2.0.0`; migration guide "Option B: Event tables (recommended for most use cases)" (#4383, v2.0.1).

### Your reducer calls now answer back (and stop leaking args) — HIGH
- Range: in v2.0.0
- What changed (exact, verified): new v2 WebSocket protocol (`v2.bsatn.spacetimedb`, #4213) plus the updated TS (#4271), Rust (#4257) and C# (#4293) SDKs.
  - **TypeScript:** generated reducer functions now return `Promise<void>`. You can `await conn.reducers.dealDamage({...})`. The promise rejects with `SenderError` (the reducer threw a user error) or `InternalError` (`crates/bindings-typescript/src/sdk/db_connection_impl.ts:818-851` at `v2.0.0`; classes in `src/lib/errors.ts`).
  - **Rust:** codegen emits `<reducer>_then(args, |ctx, result| ...)` per-call callbacks (`crates/codegen/src/rust.rs:474-495`). The plain call still works fire-and-forget.
  - **Server:** "The reducer event information is no longer sent with transaction updates (because we don't want to broadcast reducer call information anymore)" (#4213). Only the caller gets a `ReducerResult`. Other clients see a generic transaction event (`Event::Transaction`; the rename from `UnknownTransaction` is #4377, v2.0.1).
  - Removed: global `conn.reducers.onX` / `removeOnX` callbacks, `light_mode`, `CallReducerFlags` / `NoSuccessNotify`.
- Before this range: `v1.12.0` TS `callReducer(...)` returned `void` with a hard-coded `requestId: 0` ("The TypeScript SDK doesn't currently track `request_id`s"). Reducer name and arguments were broadcast to every client subscribed to the affected rows.
- Headline idea: "Call it. Await it." / "Your args stay yours."
- One-sentence description: Calling a server function from TypeScript now returns a promise that tells you whether it succeeded, and other users no longer see the arguments you sent.
- Visual idea: code card `await conn.reducers.sendMessage({ text })` with a green check resolving, then a red `SenderError: Name must not be empty` path. A second panel shows two users: in "1.x", User B's screen displays User A's reducer name and args in a speech bubble; in "2.0" that bubble is replaced by just the updated row.
- Caveats (don't claim):
  - Awaitable reducer calls are TypeScript only. Rust uses `_then` callbacks; C# uses per-connection `Reducers.OnX` callbacks, which now fire only for your own calls (migration guide).
  - This is a breaking change: apps that relied on seeing other users' reducer calls must switch to event tables.
  - The server still accepts v1 clients (`v1.bsatn.spacetimedb` and `v1.json.spacetimedb` exist at `v2.0.0`), but modules built with 2.0 don't send reducer event info to them (#4213). JSON is not offered in v2.
- Sources: #4213, #4271, #4257, #4293, #4181; migration guide sections "Reducer Callbacks", "Light Mode", "CallReducerFlags" (v2.0.1).

### `spacetime.json`: one file, bare `spacetime publish` — and `spacetime dev` now runs your client too — HIGH
- Range: in v2.0.0
- What changed (exact, verified): #4199 (plus follow-ups #4332, #4351; #4350 is v2.0.1 only) adds a project config file `spacetime.json` (`CONFIG_FILENAME` in `crates/cli/src/spacetime_config.rs:12`).
  - It holds `generate` and `publish` settings (`server`, `module-path`, `database`, `children` for multi-database trees, `generate` targets) and `dev.run` (the client command). `spacetime generate` and `spacetime publish` with no arguments read it. Passing a database name filters to that target.
  - Layering: `spacetime.local.json`, `spacetime.<env>.json` and `spacetime.<env>.local.json` (for example `spacetime.dev.json`). `spacetime dev --env <name>` defaults to `dev`, and `--no-config` ignores the files.
  - `spacetime init` writes `spacetime.json` with `"server": "maincloud"` and `"module-path": "./spacetimedb"`, plus a `spacetime.local.json` holding the generated database name (`init.rs:589-630`). After `init`, a bare `spacetime publish` works because the database comes from the local file. `publish.rs:25` still requires a database from either the CLI or the config.
  - **`spacetime dev` now also starts your client dev server.** New flags at `v2.0.0`: `--run "<cmd>"` ("Command to run the client development server (overrides spacetime.json config)"), `--server-only`, `--no-config`, `--env`, `--skip-publish`. The client command is auto-detected from package.json / Cargo.toml / .csproj and saved to `spacetime.json` ("Detected client command and saved to ..."). The CLI prints `Starting client: npm run dev`.
- Before this range: no `spacetime.json` at `v1.12.0` (no occurrences in `crates/cli/src`). `spacetime dev` (since v1.7) watched, built, generated and published, but had no `--run`/`--server-only` and never launched the client. Every `generate`/`publish` needed flags (`--lang typescript --out-dir ... --module-path ... <db>`).
- Headline idea: "Two words. Whole app." / "Config once. Ship forever."
- One-sentence description: A checked-in `spacetime.json` remembers where your module lives and where it deploys, so `spacetime publish` needs no arguments and `spacetime dev` brings up the server module and your web app with one command.
- Visual idea: terminal before/after (taken straight from the release note). Before: `spacetime generate --lang typescript --out-dir src/module_bindings` and `spacetime publish --server maincloud --module-path spacetimedb my-database`. These collapse into `spacetime generate` and `spacetime publish`, with a small `spacetime.json` card sliding in. Then `spacetime dev` prints "Watching for changes…", "Publishing…", "Starting client: npm run dev", and a browser window with the app appears.
- Caveats (don't claim):
  - Don't say `spacetime dev` is new; it has existed since v1.7. What's new is that it runs the client, plus config-driven defaults.
  - A bare `spacetime publish` needs a database name from the config or local config (`init` writes it to `spacetime.local.json`). `publish` loads the layered files via `find_and_load_with_env` (`publish.rs:298`, `spacetime_config.rs:985-989` overlays `spacetime.local.json`) and prints "Using configuration from <path>". So bare `publish`/`generate` after `init` works at `v2.0.0`. #4358 ("Allow skipping DB if the config file is available", **v2.0.1 only**) extends this to `call`, `subscribe`, `sql`, `describe`, `logs` and `delete`. Don't show e.g. a bare `spacetime logs` as 2.0.0 behaviour.
  - Demo `spacetime dev` as of v2.0.1+: watch filtering (#4317) and the watch-path/`.env.local` fixes for templates (#4396) landed after the `v2.0.0` tag.
  - `--bin-path` and similar per-module flags error out when the config has more than one target (#4199).
- Sources: #4199, #4332, #4351 (and #4350, v2.0.1); `crates/cli/src/spacetime_config.rs`, `crates/cli/src/subcommands/dev.rs` (flags at lines 89–125, client start at ~1461), `init.rs:589-630` at `v2.0.0`; `docs/docs/00200-core-concepts/00100-databases/00200-spacetime-dev.md` "Client Development Server"; 2.0 release note "`spacetime.json` configuration".

### Start from your framework: 10 new project templates + Angular & TanStack bindings — HIGH
- Range: in v2.0.0; several templates fixed in v2.0.1
- What changed (exact, verified): `templates/` at `v2.0.0` adds `angular-ts`, `nextjs-ts`, `nuxt-ts`, `tanstack-ts`, `remix-ts`, `browser-ts` (plain `<script>` tag), `bun-ts`, `deno-ts`, `nodejs-ts` and `basic-cpp`.
  - They are usable with `spacetime init --template <id>` / `-t <id>` or `spacetime dev --template <id>`. They are embedded in the CLI (`crates/cli/build.rs`, `.template.json` per template). Descriptions include, for example, "Angular web app with TypeScript server" and "TanStack Start (React + TanStack Query/Router) with TypeScript server".
  - The TS SDK gains the subpath exports `spacetimedb/angular` (`provideSpacetimeDB`, `injectTable`, `injectReducer`, `injectSpacetimeDB`, `injectSpacetimeDBConnected`; #4139) and `spacetimedb/tanstack` (`SpacetimeDBQueryClient`, `useSpacetimeDBQuery`, `useSpacetimeDBSuspenseQuery`; #4107).
  - New quickstart docs: Next.js, Nuxt, Angular, TanStack, Remix, Browser, Bun, Deno, Node.js, C++.
  - React: the new `ConnectionManager` makes `SpacetimeDBProvider` survive React StrictMode double-mounts (#4028).
- Before this range: `v1.12.0` had `basic-cs`, `basic-rs`, `basic-ts`, `chat-console-cs`, `chat-console-rs`, `chat-react-ts`, `react-ts`, `svelte-ts` and `vue-ts`. SDK subpaths were `.`, `./sdk`, `./react`, `./server`, `./vue` and `./svelte`.
- Headline idea: "Your stack. Already wired." / "Pick a framework. Go live."
- One-sentence description: New starter templates for Next.js, Nuxt, Angular, TanStack Start, Remix, Bun, Deno, Node.js and plain browser scripts give you a working real-time app in your favorite framework with one command.
- Visual idea: a carousel or grid of framework logos (Angular, Next.js, Nuxt, TanStack, Remix, Bun, Deno, Node.js, plain JS, plus the already-supported React, Vue and Svelte) lighting up one by one. Then the terminal runs `spacetime dev --template nextjs-ts` and a live app appears.
- Caveats (don't claim):
  - React, Vue and Svelte were already supported in 1.12. Say "now also".
  - Several of the new templates were broken at the `v2.0.0` tag and fixed in `v2.0.1` (#4396: Angular env vars, TanStack `getRouter`, Remix/Next.js `moduleResolution`, browser-ts converted to Vite; also #4360, #4372, #4380, #4409). Demo them as of v2.0.1+.
  - Deno, Bun and Node.js are client/runtime templates, not UI frameworks. The server module in all of these is TypeScript.
  - `keynote-2` in `templates/` is the benchmark, not a starter (it has no `.template.json`, so it isn't offered by `init`).
- Sources: #4139, #4107, #4176, #4097, #4113, #4161, #4154, #4191, #4112, #4028; `crates/bindings-typescript/package.json` exports at both tags; `docs/docs/00100-intro/00200-quickstarts/*` at `v2.0.0`.

### TypeScript modules: export-based API — MEDIUM
- Range: in v2.0.0
- What changed (exact, verified): #4220, #4273, #4271, #4309. Functions are now declared as named exports whose export name is the function name. `schema()` takes an object of tables, and the schema is the default export. Compare `templates/basic-ts/spacetimedb/src/index.ts`:
  - `v1.12.0`: `export const spacetimedb = schema(table({ name: 'person' }, {...}));` and `spacetimedb.reducer('add', { name: t.string() }, (ctx, { name }) => {...});`
  - `v2.0.0`: `const spacetimedb = schema({ person: table({}, {...}) }); export default spacetimedb;` and `export const add = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {...});`
  - Other developer-experience wins in range: `console.log` pretty-prints objects (#4285, via `object-inspect`); `String.prototype.localeCompare` and other Intl functions work because ICU data is bundled (#4253); unresolved imports show a proper located error (#4330, #4334); all generated client types sit in `module_bindings/types.ts` (#4309); TS modules can declare hash indexes (#4233); `update()` is only offered on primary-key indexes (#4279).
- Before this range: string-named registration (`spacetimedb.reducer('add', ...)`); variadic `schema(t1, t2)`; `console.log({a:1})` printed `[object Object]`-style output.
- Headline idea: "Just export it."
- One-sentence description: In TypeScript modules you now declare server functions as ordinary named exports, and the tooling (logging, errors, autocompletion) behaves the way JavaScript developers expect.
- Visual idea: a before/after code diff of the basic template. The string `'add'` fades out, `export const add =` fades in, and a `console.log(player)` line shows pretty-printed output in the log pane.
- Caveats (don't claim):
  - Don't say "TypeScript leaves beta" based on the `v2.0.0` tag: `spacetime publish` still printed "JavaScript / TypeScript support is currently in BETA." at `v2.0.0`. It was removed in v2.0.1 (#4396). The 2.0 release note does announce it.
  - This is a breaking change: 1.x TS modules must be rewritten (migration checklist).
  - There are no measured before/after numbers for the TS performance PRs (#4187, #4186, #3957, #4128); see the performance item.
- Sources: #4220, #4273, #4271, #4285, #4253, #4330, #4334, #4309, #4233, #4279; the template diff above.

### Typed queries everywhere, no `.build()` — MEDIUM
- Range: in v2.0.0
- What changed (exact, verified): #4261 standardizes the query builder across Rust, TS and C#.
  - Builders are queries directly (`.build()`/`.Build()` still works but is not needed).
  - TS table refs are query builders: `tables.user.where(u => u.name.eq("Tyler"))`, with added `ne()`, chainable `.and()/.or()/.not()`, and a callback form `subscribe(ctx => ctx.from.person.where(...))`. React `useTable` accepts query-builder queries (Vue/Svelte `useTable` got the same in #4400, v2.0.1).
  - Rust views can return `impl Query<T>`. C# gets `IQuery<TRow>` and `Not()`.
  - C# **modules** get the typed query builder for views (#4159, #4333).
- Before this range: v1.11 added module-side builders and v1.12 client-side builders, both requiring `.build()` (1.12 release note: "We would like to remove the `.build()` for 2.0"). TS views could already return `ctx.from.person.build()`. C# modules had no typed query builder.
- Headline idea: "Queries your editor understands."
- One-sentence description: Subscriptions and views are written as typed, autocompleted expressions like `tables.user.where(u => u.name.eq("Tyler"))` in TypeScript, Rust and C#, so typos are caught before you run.
- Visual idea: an editor with autocomplete popping `.where`, `.eq` and `.gte`; a deliberate typo gets a red squiggle at compile time; the query then feeds a live list.
- Caveats (don't claim):
  - Typed queries are not new in 2.0 (v1.11/v1.12). Frame this as "simpler and consistent everywhere".
  - SQL strings are **still accepted** by `subscribe()` at `v2.0.0` (`subscription_builder_impl.ts:83-99`), even though the migration guide labels the TS SQL example "NO LONGER VALID". Don't claim SQL strings were removed.
  - The C# client removal of `Query<TRow>`/`.Build()` in favor of `IQuery<TRow>` is #4393 (v2.0.1).
- Sources: #4261, #4159, #4333, #4196, #4329; 1.12 release note.

### C++ server modules (beta) — MEDIUM
- Range: in v2.0.0, pinned to 1.12
- What changed (exact, verified): #3544 adds `crates/bindings-cpp` for C++20 modules compiled to WASM with Emscripten/CMake, using macros (`SPACETIMEDB_TABLE`, `SPACETIMEDB_REDUCER`, `SPACETIMEDB_STRUCT`, `SPACETIMEDB_INIT`, ...). `spacetime init --lang cpp` (also `c++`/`cxx`) and the `basic-cpp` template ("A basic C++ server template with only stubs for code"). C++ docs tabs (#4118, #4129, #4163) and a C++ Blackholio tutorial (#4169).
- Before this range: no `crates/bindings-cpp`. `init --lang` listed only "rust, csharp, typescript".
- Headline idea: "Now in C++ too."
- One-sentence description: You can now write your server logic in C++, alongside Rust, C# and TypeScript.
- Visual idea: a language switcher with tabs Rust | C# | TypeScript | **C++**; the C++ tab slides in showing `SPACETIMEDB_REDUCER(add, ReducerContext ctx, std::string name) { ctx.db[person].insert(Person{name}); return Ok(); }`.
- Caveats (don't claim):
  - **Beta, and pinned to v1.12.0 in 2.0**. The docs component `CppModuleVersionNotice` says: "C++ support is currently in beta and subject to change. SpacetimeDB C++ 2.0 is coming soon, but C++ server modules are currently pinned to v1.12.0." (#4328, #4348). So no 2.0 features (event tables, accessor names, etc.) in C++.
  - "Unreal Engine support" is **not** new (Unreal SDK since v1.4.0). #4328 also pinned the Unreal Blackholio tutorial to the 1.12 branch.
  - Don't quote the "~6x faster than C#, ~6x slower than Rust" line from the #3544 PR body; it's an unreviewed dev measurement.
- Sources: #3544, #4109, #4118, #4129, #4163, #4169, #4328, #4348; `docs/src/components/CppModuleVersionNotice.tsx` at `v2.0.0`.

### Secure by default — MEDIUM
- Range: in v2.0.0
A bundle of four verified changes:
- (a) **Reducer args are no longer broadcast** to other clients (#4213; see above).
- (b) **Scheduled reducers and procedures are private**: a non-owner calling one directly gets "no such reducer/procedure" (`module_host.rs:1541` and `1787`: `visibility.is_private() && !self.is_database_owner(caller_identity)` → `NoSuchReducer`/`NoSuchProcedure`). Codegen skips them (#4179).
- (c) **Private tables and functions are not code-generated by default**. `spacetime generate` prints "Skipping private tables during codegen: …", and the new `--include-private` flag restores them (#4241; `generate.rs:495`).
- (d) **Procedures can't make HTTP requests to private or special-purpose IP ranges**, checked after DNS resolution. Error text: "refusing to connect to private or special-purpose addresses" (#4243; `crates/core/src/host/instance_env.rs:979`).
- Before this range: reducer args were broadcast to subscribers of affected rows; scheduled reducers were callable by any client, so modules had to check `ctx.sender == ctx.identity()`; bindings were generated for private tables; procedure HTTP had no address filtering.
- Headline idea: "Private unless you say so."
- One-sentence description: SpacetimeDB 2.0 stops sharing what users send, locks scheduled jobs away from clients, keeps private tables out of client code, and stops server-side HTTP calls from reaching internal networks.
- Visual idea: four padlocks snap shut one after another on four mini-cards (a speech bubble "args", a clock "scheduled", a table "private", a globe with a blocked arrow to `10.0.0.1`).
- Caveats (don't claim):
  - (b) applies to modules built with 2.0 (RawModuleDefV10 visibility). The owner can still call them.
  - (d) also blocks loopback on a self-hosted/standalone server (only a test-only cargo feature allows it), so don't imply local `localhost` calls work.
  - Don't call it a security audit or certification.
- Sources: #4213, #4179, #4241, #4243; migration guide "Scheduled Functions Are Now Private", "Private Items Are Not Code-Generated By Default".

### 100k+ transactions per second (keynote benchmark) — MEDIUM
- Range: in v2.0.0 as a benchmark, not a feature
- What changed (exact, verified): #4072 adds `templates/keynote-2`, a benchmark suite comparing SpacetimeDB to Postgres, CockroachDB, SQLite, Supabase, PlanetScale, Convex and Bun+Drizzle, with a `npm run demo` animated comparison. Its README "Results Summary" states that SpacetimeDB reaches **107,850 TPS (~0% contention) and 103,590 TPS (~80% contention)**, with 50 concurrent connections on a fund-transfer transaction, versus 7,845 for SQLite+Node HTTP+Drizzle as the next best. Key finding: "~14x higher throughput". The SpacetimeDB module in the benchmark is **Rust** (`templates/keynote-2/spacetimedb/src/lib.rs`; README table: "Integrated platform (Rust)").
  - Engine-side TS performance PRs in range: #4187 (fewer allocations), #4186 (v2 JS ABI with caller-allocated buffers, "greatly improves performance"), #3957 (closure-tree serialization), #4128 (core-pinned V8 threads). None report numbers.
- Before this range: no keynote comparison suite in the repo.
- Headline idea: "100,000 transfers a second."
- One-sentence description: In the team's published benchmark, SpacetimeDB handled over 100,000 money-transfer transactions per second, and kept that pace even when most transactions fought over the same few accounts.
- Visual idea: animated horizontal bar race (SpacetimeDB vs SQLite/Postgres/CockroachDB …), then a contention slider ramping to 80% where the other bars collapse and SpacetimeDB's barely moves.
- Caveats (don't claim):
  - The 2.0 release note says "Well over 100k transactions per second for TypeScript modules and up to 170k transactions per second for Rust modules". **The repo does not support this split**: the only number in the repo (107,850) is for the **Rust** module, and "170k" appears nowhere in `templates/keynote-2/README.md`. Use only the README figures with their conditions (50 connections, transfer workload, specific hardware).
  - It's a vendor-run benchmark with default settings for competitors. Don't claim "fastest database". Don't name competitors without legal sign-off.
  - Don't claim "2.0 made it N× faster"; there are no before/after numbers.
- Sources: #4072; `templates/keynote-2/README.md` at `v2.0.0` (identical at `v2.0.1`); 2.0 release note "Incredible performance".

### Guarded 1.x → 2.0 upgrade — LOW
- What changed (exact, verified): `spacetime publish` detects a major version upgrade and requires typing `upgrade`. It prints "It looks like you're trying to do a major version upgrade from 1.0 to 2.0. We recommend first looking at the upgrade notes before committing to this upgrade: https://spacetimedb.com/docs/upgrade", then "WARNING: Once you publish you cannot revert back to version 1.0.", then "Please type 'upgrade' to accept this change:" (#4247; `publish.rs:249-268`). The Rust macro tells users who still write `name = ...` to use `accessor` (#4342). The server still accepts 1.x modules (`RawModuleDef::V9`, `crates/schema/src/def.rs:403`) and 1.x clients (v1 protocols).
- Headline idea: "Upgrade on your terms."
- Caveats: this is the flip side of breaking changes, so don't make it a hero scene. The "no data migration" promise from the 1.12 note is complicated by case conversion: table names can change casing, and `CaseConversionPolicy::None` via `#[spacetimedb::settings]` is **v2.0.1 only** (#4366).
- Sources: #4247, #4342, #4213.

### Organizations (`spacetime publish --organization`) — LOW
- Range: in v2.0.0
- What changed: `spacetime publish --organization <name|identity>` (alias `--org`), "The name or identity of an existing organization this database should be created under." Organization names resolve via the TLD (#4266).
- Before: the 1.12 release note announced it, but the shipped `v1.12.0` tag reverted it. So it's effectively new for CLI users in 2.0.
- Caveats: this is a Maincloud feature (the Team tier per the release note), and organization creation/management is not in this repo. Don't show a UI you can't verify.
- Sources: #4087 (ancestor of `v2.0.0`, reverted only on the `v1.12.0` tag), #4266; `publish.rs:187-192` at `v2.0.0`.

### Smaller CLI and install quality-of-life — LOW
- Install and upgrade fall back to a DigitalOcean mirror when the GitHub download fails: install scripts, `spacetime version install`, `spacetime upgrade` (#4265).
- `spacetime version list` is sorted by semver (#4250).
- `spacetime generate` no longer runs the module's `init` reducer (#4312).
- `spacetime dev` shows status code + reason instead of an empty error when fetching logs fails (#4230).
- TS procedure/reducer errors render cleanly: "The module instance encountered a fatal error: API returned status 401" instead of "js error Uncaught …" (#4347, **v2.0.1 only**; it is not an ancestor of `v2.0.0`).

### Breaking renames (not promo material) — LOW
- Rust `#[table(name = ...)]` → `accessor = ...` (#4264). C# `Name` → `Accessor` (#4306). `ctx.sender` → `ctx.sender()` (#4208, Rust). `connection_id` becomes a method (#4215). `with_module_name` → `with_database_name` (#4267). `update()` only on primary keys (#4279). Default snake_case canonical names (#4263 case conversion, #4294, #4323). SQL resolves both accessor and canonical names via the new system tables `st_table_accessor`/`st_column_accessor`/`st_index_accessor` (#4304).
- Useful only as a "migration guide + upgrade prompt" footnote.

## Lands only in v2.0.1 (outside the literal range, but part of the public "2.0" release)
- **Confirmed reads on by default** (#4390, #4419 "Confirmed reads default only for v2 connections"): updates and SQL results are only sent after the transaction is durable. Opt out with `.withConfirmedReads(false)` / `with_confirmed_reads(false)`. Potential MEDIUM ("Never see data that could vanish"), but it's not in the `v2.0.0` tag.
- **"TypeScript is out of beta"**: the CLI BETA warning was removed in #4396.
- `#[spacetimedb::settings]` and `CaseConversionPolicy` (#4366, #4368, #4382); TS canonical naming (#4371).
- `spacetime publish`/`generate` can skip the database argument when a config is present (#4358).
- `spacetime logout` works offline (#4361); `spacetime energy` removed and `spacetime init` "unstable" warning removed (#4396).
- Template fixes (#4360, #4372, #4380, #4396, #4409, #4414), `spacetime dev` watch filtering (#4317), and the `UnknownTransaction` → `Transaction` rename (#4377).
- The 2.0 migration guide (#4383) and doc versioning (#4381, #4411).

## Left out (not user-visible or not promo-worthy)
- #4128 Rework JobCores to core-pin V8 threads — internal scheduling, no measured user impact.
- #3957, #4186, #4187 TS serialization/ABI/hot-path perf — internal. Covered only as supporting context in the benchmark item; no numbers.
- #4190, #4194, #4216, #4246, #4288 RawModuleDefV10 plumbing — internal module-definition format.
- #3915 Reorganize TS SDK; #4258 reorganize generated TS types; #4203 pass contexts by reference; #4210 ProcedureCtx as class; #4177 identifiers refactor; #4181 RawIdentifier in websocket format — internal refactors.
- #4260 TS throw error objects from syscalls; #4283 remove fast-text-encoding polyfill; #4321 ArrayBuilder fix; #4336 hash index round-trip fix; #4286 V8 panic on disconnect; #4287 semijoin planner fix; #4301, #4302 view refresh after procedure commit; #4339 ModuleInfo from validity check — bug fixes too small to feature.
- #4140 and its revert #4292 (commitlog append) — net zero.
- #4252 version bump to 2.0; #4313 metadata version check — release mechanics.
- #4268 GLIBC_2.38 release fix — build fix.
- #3252 TS quickstart stores per-server/module auth tokens — quickstart-only fix.
- #4090 C# SDK network thread name — debugging nicety.
- #4221 durability panic context — logging. (#4356 TS client debug logging is v2.0.1.)
- #4032 LLM one-shot benchmark apps + Cursor rules — mostly `tools/llm-oneshot` benchmark tooling. The updated `docs/static/ai-rules/*.mdc` content is incremental (AI rule files already shipped by `init` in 1.12).
- #4072 keynote — kept only as the benchmark item.
- #4000 template smoketests; #4102 smoketests Python→Rust; #4184, #4185, #4209, #4223, #4242, #4245, #4272, #4280, #4282, #4341 CI; #4359 test clients (v2.0.1); #4307 TS client tests; #4352 bench tests; #4248 index benchmarks; #4202 nix; #4180 Rust toolchain bump; #4259 rolldown bump; #4198 bundle-size limit; #4192, #4193, #4211, #2745 gitignore/nuget scripts; #4178 `cargo ci dlls`; #4290 GREMLINS.md; #4355 regen warnings — CI/test/infra.
- #4118, #4129, #4163, #4169, #4109, #4119, #4145, #4174, #4196, #4205, #4314, #4316, #4322, #4329, #4335, #4343, #4344, #4345 (v2.0.1), #4346, #4348, #4349, #4354, #3950, #4044 — docs/README/tutorial updates. (#4174 is "Add context about maincloud publishing being free of charge"; a pricing claim, not verifiable here.)
- #4266 organization TLD resolution — folded into Organizations.
- #4306, #4264, #4323, #4294, #4263, #4304, #4342 — folded into "Breaking renames" / upgrade items.

## Open doubts
- **Which tag is "2.0"?** The literal `v2.0.0` tag is an off-master branch tip with no GitHub release; users got `v2.0.1`. I recommend the video describe the 2.0 release as shipped (v2.0.1) and use the "[v2.0.1 only]" items (confirmed reads by default, TS out of beta) only if the video is explicitly "2.0" rather than "tag v2.0.0".
- **Performance numbers**: the release note's "100k TPS TypeScript / 170k Rust" can't be reproduced from the repo. The in-repo keynote README shows 107,850 TPS for a **Rust** module. Ask the team for the source of 170k and of the TS figure before using either.
- **Out-of-repo claims** — Maincloud free tier, new pricing, Spacerace referrals, the new dashboards/metrics UI, the Team tier and SpacetimeAuth being free: none can be verified in this repo. Treat them as marketing-owned copy.
- **"No data migration needed" for 1.x → 2.0**: v1 protocols and V9 module defs are still accepted at `v2.0.0`. But the default canonical-name change (#4263) can alter table-name casing on republish, and the documented opt-out (`CaseConversionPolicy::None`) only exists from v2.0.1. Don't promise a zero-effort upgrade.
- **Event tables and storage**: rows are appended to the commitlog (#4217). The release note's "saves you storage costs" may still hold for table state and snapshots, but I couldn't confirm any billing effect.
- **C++ in 2.0**: the modules exist in the 2.0 CLI, but the docs pin them to the v1.12.0 track. I couldn't confirm from the repo whether a C++ module built with the 2.0 CLI publishes cleanly to a 2.0 server. #4328 only says `spacetime init` for `basic-cpp` builds.
