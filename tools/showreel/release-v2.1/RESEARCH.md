# v2.1 research (v2.0.0 → v2.1.0)

Scope notes (read first; the tag layout is unusual):
- `v2.0.0` is **not** an ancestor of `v2.1.0` (nor of `v2.0.1`). It is the tip of the `shub/case-conversion` feature branch (commit `4d2fe47490`). `git log v2.0.0..v2.1.0` = 181 commits, but case conversion (#4263) appears in it only as a squash-merge duplicate of content already in `v2.0.0`.
- What users actually got as "2.0.0" on npm/crates.io (published 2026-02-20 ~20:37 UTC) matches master at `a89634b4a5` (= tag `v2.0.0-prerelease`, #4366) or `19cc87ebfa` (#4358): the npm `spacetimedb@2.0.0` `src/` is byte-identical to `crates/bindings-typescript/src` at both commits, and the crates.io `spacetimedb-bindings-macro` 2.0.0 already contains `#[spacetimedb::settings]`. There is no `v2.0.0` GitHub release and no v2.0.0 CLI binary (404).
- The public "SpacetimeDB 2.0" GitHub release is tag **`v2.0.1`** (published 2026-02-20; its binaries were uploaded 2026-02-24). As the brief asks, `v2.0.1` is **in range** here. But `release-v2.0/RESEARCH.md` ("Lands only in v2.0.1") recommends that the 2.0 video use the same v2.0.1 items. **Candidates that land in v2.0.1 are tagged `[v2.0.1 — also claimed by release-v2.0]`; the orchestrator decides which video gets them.** Everything else below is from v2.0.2…v2.1.0.
- Commits between the `v2.0.0` tag and `a89634b4a5` (#4263, #4368, #4366, #4356, #4357, #4317, #4347, #4350, #4359, #4360, #4365, #4345) are not in the `v2.0.0` tag, but they are in the 2.0.0 packages published to npm/crates.io. They are listed under Left out as "already in the published 2.0.0 packages".
- `v2.0.4`, `v2.0.5` and `v2.1.0` each sit on an empty side commit ("Release 2.0.x") on top of master; they add no content (checked with `git show --stat`).
- Every claim below was checked against code at the tags (`git show "<tag>:<path>"`), not only PR text. Where the PR text and code differ, the code wins and the difference is noted.

## Releases in range
- `v2.0.1` — GitHub release "SpacetimeDB 2.0", published 2026-02-20 (binaries 2026-02-24). The 2.0 launch post (its body describes 2.0 as a whole). Code-wise it adds, over the 2.0.0 packages: confirmed reads on by default for v2 clients, DB name optional in more CLI commands, removal of the TS "BETA" publish warning and of `spacetime energy`, offline logout, template fixes and small SDK fixes.
- `v2.0.2` — 2026-02-26. Bug-fix/QoL: `spacetime dev` watcher honours `--module-path`, clearer "Module directory does not exist" error, templates, local fsync interval 500 ms → 10 ms, C# `ArrayPool` row iteration.
- `v2.0.3` — 2026-03-04. `spacetime logs --level`, React `useTable` `isReady` fix, TanStack typed rows, CLI path fixes (spaces, leading `..`), `-y` skips the 1.0→2.0 upgrade prompt, C++ module bindings moved to RawModuleDefV10 (#4461).
- `v2.0.4` — 2026-03-11. Daily CLI update notice, fuzzy-filterable template picker, login/logout UX, AgentSkills (`skills/`), TanStack SSR prefetch, Rust primary keys for query-builder views, `bool` shorthand in query builders, host-type (module language) persistence; also two module API breaks (TS/C# index `accessor` required).
- `v2.0.5` — 2026-03-13. HTTP procedure timeouts 500 ms/10 s → 30 s/180 s, repair for databases mis-tagged by the 2.0.4 host-type change, C# primary keys for query-builder views, view-subscription disconnect fix, Rust SDK debug-to-file mode, TS `Range`/`Bound` export.
- `v2.1.0` — 2026-03-24. Rust client SDK compiles to wasm (`browser` feature), Unreal SDK moved to the v2 WebSocket protocol (event tables, multi-module codegen), view-subscription fixes, `spacetime.json` inheritance fixes, C# keyword escaping, Rust `count()` on view table accessors.

## Candidates

### Rust client SDK in the browser — HIGH
- What changed (exact, verified): #4183 adds a Cargo feature named **`browser`** (not `web` as the PR text says) to `spacetimedb-sdk`. Verified on crates.io: `spacetimedb-sdk` 2.1.0 features = `allow_loopback_http_for_tests, browser, default`; 2.0.5 has only `allow_loopback_http_for_tests`. With `browser`, the SDK builds for `wasm32-unknown-unknown` (`cargo build --target wasm32-unknown-unknown --no-default-features --features browser`, `sdks/rust/tests/test.rs:34` at v2.1.0), uses `tokio-tungstenite-wasm`, `wasm-bindgen-futures`, `gloo-*`, `web-sys`. Browser-specific API at v2.1.0 (`sdks/rust/src/db_connection.rs`): `DbConnectionBuilder::build()` is `async` (line 921) and `DbConnection::run_background_task()` spawns the message loop with `wasm_bindgen_futures::spawn_local` (line 653). `sdks/rust/src/credentials.rs` gains a `web_mod` re-exporting `gloo_storage::{LocalStorage, SessionStorage, Storage}` and a `cookies::Cookie` builder for storing tokens.
- Before this range: the Rust client SDK was native-only (no `browser` feature in 2.0.5). Browser clients existed via the TypeScript SDK and via the C# SDK in Unity WebGL builds (`UNITY_WEBGL` paths in `sdks/csharp/src/WebSocket.cs` at v2.0.5).
- Headline idea: "Rust, now in the browser."
- One-sentence description: Game and app clients written in Rust can now be compiled to WebAssembly and connect to SpacetimeDB straight from a web page.
- Visual idea: a Rust client file (`Cargo.toml` with `features = ["browser"]`) morphs into a browser window; the same live table updates stream into the page. Split screen: native desktop window on the left, browser tab on the right, both receiving the same rows. Don't frame it as "the first way to reach the browser": say "Rust, too".
- Caveats (don't claim): no official template, quickstart or docs page for Rust-in-browser exists at v2.1.0 or v2.2.0 (grep of `docs/` and `templates/` finds nothing), so don't show "`spacetime init --template rust-web`" or similar. CI runs the wasm test clients **under Node.js, not a real browser** (`test.rs:66` comment), so don't claim "tested in every browser". API differs slightly from native (`build().await`, `run_background_task()`); don't show native-only calls like `run_threaded()` in a browser scene. Module (server) side is unchanged; this is client-only.
- Sources: #4183 (supersedes #4089); v2.1.0 release note "🦀 Rust client Wasm support"; `sdks/rust/Cargo.toml`, `sdks/rust/src/db_connection.rs`, `sdks/rust/src/credentials.rs` at v2.1.0; crates.io API for `spacetimedb-sdk` 2.0.5/2.1.0.

### AI calls from procedures no longer time out — HIGH
- What changed (exact, verified): #4630 (v2.0.5) changes `crates/core/src/host/instance_env.rs`: `HTTP_DEFAULT_TIMEOUT` 500 ms → **30 s**, `HTTP_MAX_TIMEOUT` 10 s → **180 s**. The effective timeout is `timeout.unwrap_or(HTTP_DEFAULT_TIMEOUT).min(HTTP_MAX_TIMEOUT)` (line 891 at v2.1.0). #4610 (v2.1.0) makes failed procedure HTTP requests report the whole error chain, e.g. `error sending request for url (…): error trying to connect: dns error: …` instead of just `error sending request for url (…)`.
- Before this range: at the 2.0.0 build (`a89634b4a5`) and v2.0.4 the constants were 500 ms / 10 s. The PR notes users hitting the 10 s ceiling calling LLM APIs, and silent failures from the 500 ms default when no timeout was set.
- Headline idea: "Let the AI think."
- One-sentence description: HTTP requests made from procedures now wait up to 30 seconds by default and up to 3 minutes when you ask, so calls to slow APIs like LLMs finish instead of timing out.
- Visual idea: reuse the 2.0 launch-post `ask_ai` procedure (`ctx.http.fetch('https://api.openai.com/…')`). Timeline bar: old run cut off at the 0.5 s tick with a red "timeout"; new run's bar stretches past 10 s to a green response at ~40 s, with a "30 s default / 180 s max" ruler. Optional second beat: an error toast that now says "dns error: failed to lookup address".
- Caveats (don't claim): the limit is per HTTP request, not a guarantee on total procedure runtime (no other procedure limit was checked). 180 s is a hard clamp, not "unlimited". Maincloud runs this server code, but no Maincloud-specific config was checked. It's a limit change plus a bug fix, not a new API: `ctx.http.fetch` and procedures predate the range.
- Sources: #4630 (table in PR body), #4610, #4608 (issue); `crates/core/src/host/instance_env.rs` at `a89634b4a5`, v2.0.4, v2.0.5, v2.1.0; v2.0.5 release note "Bump HTTP procedure timeouts (500ms/10s → 30s/180s)"; docs `docs/docs/00200-core-concepts/00200-functions/00400-procedures.md` (timeout note moved out of the C++ tab and corrected).

### Clients only see data that's safely on disk (confirmed reads by default) — HIGH `[v2.0.1 — also claimed by release-v2.0]`
- What changed (exact, verified):
  - v2.0.1 (#4390, narrowed by #4419): the server now defaults to confirmed reads. For WebSocket connections, `resolve_confirmed_reads_default` (`crates/client-api/src/routes/subscribe.rs:98-106` at v2.1.0) gives `false` for v1-protocol clients and `crate::DEFAULT_CONFIRMED_READS` (= `true`, `crates/client-api/src/lib.rs:33`) for v2 clients, i.e. every 2.x SDK. HTTP SQL uses `confirmed.unwrap_or(DEFAULT_CONFIRMED_READS)` (`routes/database.rs:534`); the Postgres wire server passes `confirmed: Some(true)` (`crates/pg/src/pg_server.rs:163`). Subscription updates and SQL results are sent only after the transaction is durable. Opt out per connection with `?confirmed=false` or `.withConfirmedReads(false)` / `.with_confirmed_reads(false)` / `.WithConfirmedReads(false)`.
  - The wait for durability then shrank, for local (standalone, non-replicated) durability in `crates/durability/src/imp/local.rs`: periodic flush+fsync every **500 ms** at v2.0.1 → **10 ms** at v2.0.2 (#4420, #4466) → from v2.0.4 no timer at all: the durability actor takes every queued transaction, commits them and calls `flush_and_sync()` right away (`run()` loop, lines 236-251 at v2.1.0; #4404 removed `sync_interval`).
- Before this range: at the 2.0.0 build (`a89634b4a5`) `confirmed` was a plain `bool` defaulting to `false` on both the WebSocket and SQL routes, so updates were sent before they were durable unless a client opted in with `withConfirmedReads(true)`, which already existed in all three SDKs. Opting in meant waiting for a 500 ms fsync tick.
- Headline idea: "Nothing you see gets lost."
- One-sentence description: By default, apps now only receive updates after they've been written to disk, so a server crash can't undo something a player already saw.
- Visual idea: before/after timeline with a crash. Before: a row "gold +100" reaches the client, the server crashes, restarts, the gold is gone (client and server disagree). After: the row waits a beat for a small "disk ✓" tick, then reaches the client; the crash comes, restarts, the gold is still there.
- Caveats (don't claim): it's a default, not a new capability (opt-in existed at 2.0.0), and it can be switched off. It adds latency: the fsync time. Don't quote "10 ms" as the current cost (the timer is gone since v2.0.4) and don't give any number for Maincloud; replicated durability wasn't checked. v1-protocol clients keep the old behaviour (that included the Unreal SDK until 2.1.0). The 2.0 migration guide (#4383/#4390, v2.0.1) lists it as a 2.0 change and the v2.0 research recommends it for the 2.0 video, so only one video should use it.
- Sources: #4390, #4419, #4420, #4466, #4404; files above at `a89634b4a5`, v2.0.1, v2.0.2, v2.0.4, v2.1.0; migration guide `docs/docs/00300-resources/00100-how-to/00600-migrating-to-2.0.md` ("Confirmed Reads Enabled by Default"); v2.0.2 note "Reduce local durability fsync interval to 10ms (down from misconfigured 500ms default)".

### Unreal and C++ catch up with 2.0 (event tables end to end) — HIGH
- What changed (exact, verified):
  - C++ modules (#4461, v2.0.3): module definition moved to RawModuleDefV10; event tables via the 4th arg of `SPACETIMEDB_TABLE` (e.g. `SPACETIMEDB_TABLE(ConsumeEntityEvent, consume_entity_event, Public, true)` in `demo/Blackholio/server-cpp/spacetimedb/src/lib.cpp:185` at v2.1.0); `SPACETIMEDB_SETTING_CASE_CONVERSION(...)` (`crates/bindings-cpp/include/spacetimedb/macros.h:550`); new `*_NAMED` macros for explicit canonical names; `FIELD_NamedMultiColumnIndex` renamed `FIELD_MultiColumnIndex` (API break).
  - Unreal SDK (#4497, v2.1.0): WebSocket protocol string `v1.bsatn.spacetimedb` (at 2.0.0 and v2.0.5) → `v2.bsatn.spacetimedb` (v2.1.0, `sdks/unreal/.../Private/Connection/Websocket.cpp`); Unreal codegen handles event tables (`table.is_event` in `crates/codegen/src/unrealcpp.rs` at v2.1.0; absent at v2.0.5); new `spacetime generate --module-prefix` "(only used with --lang unrealcpp)" for generating into multiple Unreal modules; Unreal Blackholio's consume-entity now uses an event table.
- Before this range: at 2.0.0 the Unreal SDK still spoke the v1 protocol and its codegen had no event-table support; C++ module bindings were on the V9 module def without case-conversion settings (#4359 notes the C++ bindings had not been updated to the new case-conversion scheme).
- Headline idea: "Unreal, fully on 2.0."
- One-sentence description: Unreal Engine clients and C++ modules now speak SpacetimeDB 2.0's protocol, so features like event tables work end to end in Unreal projects.
- Visual idea: Blackholio (Unreal) — a big circle eats a small one; a "consume" event pops as a floating label (`ConsumeEntityEvent`) and vanishes, with a side panel showing the C++ `SPACETIMEDB_TABLE(..., Public, true)` line. Or: badge row "Rust · C# · TypeScript · C++ · Unreal" all lighting up "2.0".
- Caveats (don't claim): HIGH assumes a games audience; for a web audience treat it as a card. Don't say "Unreal support is new" — the 2.0 launch post already announced Unreal/C++; this is catching the Unreal SDK up to the 2.0 protocol. #4497 is labelled api-break (Unreal generation now requires a valid `.uproject`; message handling changed), so it's not a drop-in update. Unreal macOS builds were broken until 2.2 (#4712, see v2.2 research), so don't show a Mac in a 2.1 Unreal scene.
- Sources: #4461, #4497, #4675 (demo C++/C# Blackholio event table), #4359; v2.1.0 release note "🎮 C++ Modules + Unreal SDK"; paths above at v2.0.5/v2.1.0.

### Give your AI coding agent SpacetimeDB skills — HIGH
- What changed (exact, verified): #4172 (merged 2026-03-03, first tagged in v2.0.4) adds `skills/` with 6 AgentSkills (`spacetimedb-cli`, `spacetimedb-concepts`, `spacetimedb-csharp`, `spacetimedb-rust`, `spacetimedb-typescript`, `spacetimedb-unity`; each `SKILL.md` has `license: Apache-2.0`, `metadata.version: "2.0"`). The PR's install command: `npx skills add clockworklabs/SpacetimeDB` (also `-s spacetimedb-rust`, `--list`). Each skill lists commonly hallucinated APIs and a "Common Mistakes" table.
- Before this range: no `skills/` directory (absent at `a89634b4a5` and v2.0.3). Static AI rules files (`docs/static/ai-rules/*.mdc`, referenced from `llms.txt`) already existed, so AI guidance itself is not new.
- Headline idea: "Your AI already knows SpacetimeDB."
- One-sentence description: One command teaches coding agents like Claude Code, Cursor or Copilot the correct SpacetimeDB APIs, so they stop inventing ones that don't exist.
- Visual idea: chat with an agent. Before: agent writes `#[spacetimedb::table]` / `ctx.db.player` and a red squiggle appears. Terminal: `npx skills add clockworklabs/SpacetimeDB`. After: the agent writes the correct `#[table]` / `ctx.db.player()` and the module publishes.
- Caveats (don't claim): not tied to a release (see Open doubts); putting it in the 2.1 video is an editorial choice. `npx skills` is a third-party CLI (agentskills.io standard), not part of SpacetimeDB. The command installs whatever is on the repo's default branch, not a version-matched set: on GitHub's default branch today (checked with `gh api repos/clockworklabs/SpacetimeDB/contents/skills`) `skills/` holds 11 differently named skills (`cli`, `concepts`, `cpp-server`, `csharp-client`, `csharp-server`, `mcp`, `rust-server`, `typescript-client`, `typescript-server`, `unity`, `unreal`), so don't show the 2.1-era names as current. The PR says 5 skills; there are 6. Not mentioned in any release note or in README/docs at v2.1.0. "Works with 40+ agents" is the PR's claim, not verified.
- Sources: #4172; `git ls-tree` of `skills/` at `a89634b4a5`, v2.0.3, v2.0.4, v2.1.0; GitHub contents API for the default branch; v2.0.4 "What's Changed" list.

### Find a template by typing — MEDIUM
- What changed (exact, verified): #4470 (v2.0.4) replaces the interactive template menu in `get_template_config_interactive` (`crates/cli/src/subcommands/init.rs`), which is used by both `spacetime init` and `spacetime dev` when no `--template`/`--lang` is given. New flow: `FuzzySelect` "Select a language (type to filter)" listing every language/framework group with counts, sorted alphabetically (at v2.1.0: `C# (2 templates)`, `C++/Rust (1 template)`, `Rust (2 templates)`, `TypeScript (4 templates)`, `TypeScript/Angular (1 template)`, `TypeScript/Bun`, `TypeScript/Next.js`, `TypeScript/Node.js`, `TypeScript/Nuxt`, `TypeScript/React`, `TypeScript/Remix`, `TypeScript/Svelte`, `TypeScript/TanStack`, `TypeScript/Vue.js` (1 each), then `Clone from GitHub (owner/repo or git URL)`, `None`); if a group has several templates, a second fuzzy menu "Templates available for <group> (type to filter, Esc to go back)" shows `<id> - <description>`.
- Before this range: at 2.0.0 the menu was a fixed "Select a client type for your project (you can add other clients later)" list of highlighted templates plus an "Other" entry where you had to type a template ID or GitHub repo.
- Headline idea: "Type React. Start building."
- One-sentence description: Starting a project now shows every template grouped by language and framework, and you just type to filter.
- Visual idea: terminal `spacetime dev`; the fuzzy list appears; user types `vue`; list collapses to `TypeScript/Vue.js (1 template)`; Enter; project scaffolds.
- Caveats (don't claim): the release note says "`spacetime dev` project initialization"; it also applies to interactive `spacetime init` (same function). The Astro template is not in this list until 2.2. Don't claim search over descriptions in the first menu (it filters the group labels).
- Sources: #4470 (screenshot in PR); `crates/cli/src/subcommands/init.rs:733-897` at v2.1.0 vs `:756-840` at `a89634b4a5`; templates' `.template.json` at v2.1.0; v2.0.4 release note "fuzzy search".

### Filter logs by severity — MEDIUM
- What changed (exact, verified): #4362 (v2.0.3) adds to `spacetime logs`: `--level <LEVEL>` / `-l <LEVEL>` ("Minimum log level to display"; values `trace`, `debug`, `info`, `warn`, `error`, `panic`) and `--level-exact` (requires `--level`; "Show only logs at exactly the specified level"). Works with text and JSON output.
- Before this range: no `level` arg in `crates/cli/src/subcommands/logs.rs` at v2.0.2.
- Headline idea: "Only the errors, please."
- One-sentence description: `spacetime logs` can now show just warnings and errors, or exactly one level, so the problem line isn't buried.
- Visual idea: a noisy scrolling log wall; type `spacetime logs my-game --level warn`; everything but yellow/red lines fades out. Then `--level error --level-exact`.
- Caveats (don't claim): filtering is client-side (the server still sends all lines), so it doesn't make logs download faster.
- Sources: #4362 (fixes #1972); `crates/cli/src/subcommands/logs.rs:52-71` at v2.1.0; v2.0.3 release note.

### The CLI tells you when an update is out — MEDIUM
- What changed (exact, verified): #4363 (v2.0.4) adds `crates/update/src/update_notice.rs`, run by the `spacetimedb-update` proxy before it execs the CLI (`proxy.rs:44`). It caches in `<config dir>/.update_check_cache`, checks the GitHub releases API at most every 24 h (`CHECK_INTERVAL`) with a 2 s timeout (`UPDATE_CHECK_TIMEOUT`; the PR text says 5 s), ignores all failures, and prints to stderr (yellow): `A new version of SpacetimeDB is available: v<latest> (current: v<current>)` then ``Run `spacetime version upgrade` to update.``
- Before this range: no update notice.
- Headline idea: "Never miss a release."
- One-sentence description: Once a day the `spacetime` command checks for a newer version and tells you how to upgrade.
- Visual idea: any command in a terminal; a yellow line appears above the output with the exact text; the user runs `spacetime version upgrade`.
- Caveats (don't claim): it only exists for installs that go through the official installer/proxy (`spacetimedb-update`), not e.g. a `cargo install`ed CLI. "current" is the proxy's own version (`CARGO_PKG_VERSION` of `crates/update`), which `spacetime version upgrade` self-updates; a user who pins an older CLI with `spacetime version use` won't be prompted. Users only start seeing it after they're on ≥2.0.4. The v2.0.4 note says "checks daily"; that matches the 24 h cache.
- Sources: #4363; `crates/update/src/update_notice.rs`, `crates/update/src/proxy.rs`, `crates/update/src/cli/upgrade.rs` at v2.1.0; v2.0.4 release note.

### Change your module's language, keep your data — MEDIUM
- What changed (exact, verified): #4549 (v2.0.4) persists the module kind (WASM vs JS) in `st_module` on update and honours it when loading, so a database can be republished with a module in another language (e.g. Rust → TypeScript → Rust) and still load after a restart. #4619 (v2.0.5) repairs databases whose `st_module` said WASM while holding a JS module (a problem that #4549 surfaced in 2.0.4). Smoketest `crates/smoketests/tests/smoketests/change_host_type.rs` at v2.1.0: publish Rust module, insert row; publish TS module with the same `person` table, insert; restart; publish Rust again; restart; all rows still there.
- Before this range: the module kind was hard-coded to WASM in `st_module` and not honoured at load, so updating a database across the WASM/JS boundary didn't work reliably (PR: "Fixes two issues that would prevent updating a database while also changing the host type").
- Headline idea: "Switch languages. Keep your data."
- One-sentence description: You can republish an existing database with a module rewritten in another language, like Rust to TypeScript, and your rows stay put.
- Visual idea: a table of player rows stays fixed in the centre while the module file beside it flips from `lib.rs` to `index.ts`; a server restart icon spins; rows remain.
- Caveats (don't claim): the new module must still pass the normal auto-migration rules for the schema (same tables/compatible types). Don't mention #4619 as a feature; it repairs a regression introduced by #4549 within this range. Exact pre-2.0.4 failure mode (fail at publish vs fail after restart) was not reproduced.
- Sources: #4549, #4619; smoketest above; v2.0.5 release note ("Attempt to repair databases with wrong host type").

### Views keep updating for everyone — MEDIUM
- What changed (exact, verified from PR text and file lists): four view-subscription fixes. #4607 (v2.0.5): when one connection of an identity disconnected, the server dropped that identity's views for all its connections, so other tabs/devices stopped receiving updates. #4648 (v2.1.0): a v2 client disconnecting or unsubscribing could stop sender-scoped view updates for other v2 clients (bug introduced with the v2 protocol, i.e. present in 2.0.0). #4646 (v2.1.0): anonymous views could be dropped while other clients were still subscribed. #4639 (v2.1.0): clients subscribed through views could miss delete updates when a one-shot scheduled reducer/procedure's schedule row was auto-deleted.
- Before this range: all four bugs were present in 2.0.0.
- Headline idea: "Every screen stays live."
- One-sentence description: Fixed several cases where one user leaving could silently stop live view updates for others.
- Visual idea: three phones subscribed to the same leaderboard view; one phone disconnects; the other two keep ticking.
- Caveats (don't claim): these are bug fixes, not a new feature; don't imply views were unusable. Only PR descriptions and changed-file lists were checked, not a repro.
- Sources: #4607, #4648, #4646, #4639; v2.1.0 release note ("Fix v2 client disconnects dropping subscriptions for other v2 clients").

### Query-builder views get primary keys (update callbacks) — LOW
- What changed (exact, verified): Rust (#4572, v2.0.4) and C# (#4626, v2.0.5) modules: a view that returns a query-builder query over a table with a primary key now has that primary key (return type encoded as the special product `{ __query__: T }`; backing table gets a PK index). Rust and C# client codegen then generate `on_update` / `OnUpdate` for such views and PK index bindings.
- Before this range: query-builder views had no primary key, so clients saw delete+insert instead of updates.
- Headline idea: "Views that know what changed."
- One-sentence description: Views built with the query builder keep their table's primary key, so Rust and C# clients get proper "row updated" callbacks.
- Visual idea: a view row's score changes; old version shows a flash of delete then insert, new version shows a smooth in-place update event.
- Caveats (don't claim): not TypeScript — the TS version (#4573) is unmerged, and TS codegen wasn't changed. Views compiled with older versions keep working without a PK.
- Sources: #4572, #4626 (same changes as #4614); `git log -S'__query__'`.

### Smoother `spacetime login` / `logout` — LOW
- What changed (exact, verified): #4367 (v2.0.4): `spacetime login` when already logged in logs out the previous session and continues (instead of "You are already logged in"), and prints `Logged in with identity <id>`; `spacetime logout` prints `Logged out (identity <id>).` or `You are not logged in.`, and the server-side invalidation is best effort with a 5 s timeout (`Warning: Failed to logout from auth server: …`).
- Before this range: login refused when already logged in; logout printed nothing.
- Caveats: #4579 (v2.0.4) fixes `spacetime login --token` falling through to web login, a regression introduced by #4367 in the same release, so it isn't news. The offline-logout fix (#4361) shipped in v2.0.1.
- Sources: #4367, #4579; `crates/cli/src/subcommands/login.rs`, `logout.rs` at v2.1.0.

### Other user-visible changes — LOW
- `[v2.0.1 — also claimed by release-v2.0]` `spacetime call`, `subscribe`, `sql`, `describe`, `logs` and `delete` can omit the database name when `spacetime.json` defines exactly one database (#4358; `crates/cli/src/subcommands/db_arg_resolution.rs`). `publish`/`generate` could already do this at 2.0.0.
- `[v2.0.1 — also claimed by release-v2.0]` `spacetime publish` no longer prints "JavaScript / TypeScript support is currently in BETA."; `spacetime init` drops its "unstable" warning; the unused `spacetime energy` command is removed; `spacetime dev` template/watch-path fixes (#4396).
- `[v2.0.1 — also claimed by release-v2.0]` `spacetime logout` works offline: prints a warning and still clears local credentials (#4361); `spacetime sql --interactive <db>` works again (#4402).
- `[v2.0.1]` SDK fixes: TS no longer shadows `Math.random()` in client code (#4375); TS client closes the connection when an already-applied subscription errors (#4378); Vue/Svelte `useTable` accept query-builder queries (#4400); Rust procedure HTTP calls with an empty response body no longer panic (#4386); Rust view functions keep their declared visibility (#4387).
- Query builders accept a bare boolean column: `ctx.from.user().r#where(|u| u.online)` instead of `.eq(true)` (#4547, v2.0.4; Rust, C#, TS; also removes C# `NullableCol`/`NullableIxCol` types).
- Rust procedural views get `count()` on table accessors, already available in C#/TS (#4638, v2.1.0).
- Rust: obtain an `AnonymousViewContext` from a `ViewContext` (#4671, v2.1.0).
- Rust client: `.with_debug_to_file("path.txt")` on the connection builder writes verbose SDK logs (#4566, v2.0.5; "explicitly not for production use").
- TS modules can import `Range`/`Bound` from `spacetimedb/server` for range queries, as the docs already showed (#4567, v2.0.5).
- TanStack Start: server-side prefetch of `spacetimeDBQuery` data that hydrates into a live subscription (#4519, v2.0.4; template `templates/tanstack-ts` shows `ensureQueryData`); `useSpacetimeDBQuery` rows now typed (#4488, v2.0.3).
- React `useTable` `isReady` no longer stuck on/reverting to `false` (#4499 v2.0.3, #4580 v2.1.0).
- TS SDK fixes: booleans in flat structs decoded as `1`/`0` (#4596), `toCamelCase` (#4523), table wire-name crash with case conversion (#4449, v2.0.2), queries use column names not accessors (#4627), clearer compile error for `and`/`or` in semijoins (#4605).
- C#: reserved keywords (`@params`, `@class`) as field/param names now generate valid code (#4535); row iteration rents from `ArrayPool` instead of allocating per row (#4385, v2.0.2); clearer view diagnostics (#4435).
- HTTP schema route: `/v1/database/:name/schema?version=10` (#4540) and it waits up to 10 s for a loading database instead of returning 500 (#4551).
- Postgres wire: pgwire 0.37 (#3910; PR claims ~3x faster row encoding — not measured here).
- `spacetime dev` polish: watcher honours `--module-path`/`module-path` (#4464), "Module directory does not exist: '<path>'. Check your --module-path flag or the module-path setting in spacetime.json." (#4467), prints `Client process exited. File watcher is still active.` (#4469), top-level `module-path` applies to `generate` entries (#4656; pre-existing bug, present at 2.0.0).
- `spacetime.json`: `generate` no longer inherited by children, module-source conflict rule enforced, `--num-replicas` per-database; new reference page (#4504).
- CLI fixes: publishing from directories with spaces (#4453), leading `..` kept in `--out-dir` (#4431), `-y` skips the 1.0→2.0 upgrade prompt on publish (#4511).
- Templates: "Press Enter to exit..." text in basic-rs/basic-cpp (#4468).

## Left out (not user-visible or not promo-worthy)
<!-- generated from the full PR list of v2.0.0..v2.1.0 minus the candidates above; reasons annotated -->
- #4678 Keynote-2 sqlite fixes — benchmark tooling, not shipped to users
- #4655 `{Multi,Unique}Map` -> `{/Unique}BTreeIndex` + `Btree` -> `BTree` — internal crate (spacetimedb-table), not module API
- #4667 docs: Fix incorrect Math.random() note in reducer context — docs only
- #4682 Make confirmed reads the default for the ts connector — benchmark tooling
- #4677 core: Keep a reordering window in durability worker — internal race fix
- #4681 Bump versions to 2.1.0 — version bump
- #4165 Updated Query with Indexes to be code-accurate — docs only
- #4683 CI - Disable PR approval check — CI / repo tooling, not user-visible
- #4680 Add more info to segment file errors — internal error context
- #4675 Add `ConsumeEntityEvent` to Blackholio C++ and C# modules — demo update (cited as a source under Unreal/C++)
- #4674 Add a metric for the number of module instances — internal metrics/logging
- #4570 Template README + template.json generation tool — repo tooling
- #4654 cleanup `TypedIndexPointIter` & ditch `Direct` variant — internal
- #4493 add `Deserialize::validate` for non-allocating validation — internal
- #4628 Remove legacy SQL code — dead-code removal
- #4670 fix(docs): Fix back to top button display — docs only
- #4643 fix: Replace unwrap with proper error handling in set_domains handler — rare server robustness fix
- #4633 Correct stale C++ quickstart links in README — docs only
- #4653 `impl Deserialize for Packed + SumTag` — internal
- #4650 commitlog: Resumption of sealed commitlog — ops/internal
- #4620 Update docs for primary key views — docs only
- #4616 Tidy up old code from the benchmark — benchmark tooling, not shipped to users
- #4623 Version bump to 2.0.5 — version bump
- #4598 Upgrade prometheus to 0.14.0 — internal dependency
- #4588 Fix Rust Chat App Tutorial not showing messages of other users live — docs/template
- #4615 CI: Use pull_request_target for PR approval check — CI / repo tooling, not user-visible
- #4611 CI - PR approval check skips for external PRs (properly this time) — CI / repo tooling, not user-visible
- #4604 CI - Skip PR approval check on external PRs — CI / repo tooling, not user-visible
- #4552 CI - Fix package job — CI / repo tooling, not user-visible
- #4600 Version bump 2.0.4 — version bump
- #4602 CI - Label check runs on `synchronize` events — CI / repo tooling, not user-visible
- #4595 CI - `rustfmt` instead of `cargo fmt` — CI / repo tooling, not user-visible
- #4594 CI - Label check always runs — CI / repo tooling, not user-visible
- #4376 CI - Stop running Python smoketests — CI / repo tooling, not user-visible
- #4578 CI - Simplify PR approval check — CI / repo tooling, not user-visible
- #4562 docs: Clarify HTTP endpoint auth is optional, not required — docs only
- #4569 docs: Audit HTTP API docs against code — docs only
- #4568 CI - `clockwork-labs-bot` needs 2 approvals — CI / repo tooling, not user-visible
- #4500 Overhaul README with up-to-date content — docs only
- #4564 Fix stale `--project-path` flag in templates — template fix
- #4560 Fix typos in comments and doc comments across crates — docs only
- #4545 docs: self-hosted prod/test/dev Azure VM guide with key rotation, Azure Key Vault workflows, and rsync data migration pattern — CI / repo tooling, not user-visible
- #4539 CI - Reduce when package job runs — CI / repo tooling, not user-visible
- #4537 docs: fix TS index definition `name` → `accessor` — docs only
- #4544 gitignore `*.local` files — repo hygiene
- #4543 Update edition in `.rustfmt.toml` and pre-commit hook — CI / repo tooling, not user-visible
- #4536 Don't put invalid `Cargo.toml` files in our repo — build tooling
- #4473 Adds code signing to tagged windows builds — VERIFIED ineffective in range: the Windows binaries of v2.0.3, v2.0.4 and v2.1.0 carry no Authenticode signature (first signed release is v2.2.0, via #4906)
- #4494 Bring typescript benchmark client to parity with rust — benchmark tooling, not shipped to users
- #4541 Make accessor required for table-level index defs in C# — API break shipped in 2.0.4; same note as #4525
- #4534 Fix 'unsafe attr without unsafe' error — fix for a regression introduced by #3802 inside the range
- #4525 Make `accessor` required for table-level index defs in typescript — API break shipped in the 2.0.4 patch (compile error for affected modules); not promo, but means 2.0.x→2.1 is not guaranteed drop-in
- #3802 Migrate to Rust 2024 — internal code change
- #4522 Use prepared statements for postgres keynote benchmark — benchmark tooling, not shipped to users
- #4524 Fix a misprint in the self-hosting docs — docs only
- #4513 [docs] Corrected `call` case and updated `out-dir` to match part 3 — docs only
- #4501 fix index truncate edge cases — replica/ops fix, not user-facing
- #4315 docs: document how to access module owner via init reducer — docs only
- #4508 Add missing TypeScript example in migration guide — docs only
- #4413 LLM benchmark tool updates — benchmark tooling, not shipped to users
- #4492 keynote-2: alpha -> 1.5, withConfirmedReads(true), remove warmup — benchmark tooling, not shipped to users
- #4489 Version upgrade 2.0.3 — version bump
- #4490 Fix missing word 'time' in ScheduleAt tutorial docs — docs only
- #4474 Add PlanetScale configuration details to keynote README & DEVELOP — benchmark tooling, not shipped to users
- #4391 C# smoketest for `IQuery` views — tests only
- #4463 Remove security warning from 00500-schedule-tables.md — docs only
- #4462 `cargo bump-versions` properly updates the smoketests lockfile — tests only
- #4477 fix(docs): Rename spacerace to referrals for nav item — docs only
- #4455 Bump versions to 2.0.2 — version bump
- #4370 Add schedule name correction for LLMs — docs only
- #4452 Correct unique indexes in cheat sheet to use `filter`, not `find` — docs only
- #4392 [Docs] [C#] Update docs with `List<T>` returns and `IEnumerable<T>` tests — docs only
- #4458 Fix spacerace button in docs — docs only
- #4454 Smoketest subscribe properly respects new confirmed behavior — tests only
- #4434 Correct "table" to "view" in 00250-zen-of-spacetimedb.md — docs only
- #4430 Fix AI assistant rules links in llms.txt — docs only
- #4281 CI - Fix smoketests running twice — CI / repo tooling, not user-visible
- #4426 Add ARM M-series CPU note in benchmark readme — benchmark tooling, not shipped to users
- #4384 Update `bump-versions` to support prereleases — version bump
- #4423 Remove source-config field from spacetime.json — minor bugfix
- #4422 Don't save source_config to spacetime.json — minor bugfix
- #4421 keynote-2: use rust client — benchmark tooling, not shipped to users
- #4418 Keynote fixes/refinements — benchmark tooling, not shipped to users
- #4416 LLM Benchmark docs updates from testings — benchmark tooling, not shipped to users
- #4414 C++ Quickstart - spacetime dev not working — template fix
- #4412 Fix keynote-2 module — benchmark tooling, not shipped to users
- #4388 LLM Benchmark Results - Feb 26 — benchmark tooling, not shipped to users
- #4415 [Docs] Updates to `00600-c-sharp.md` to work in 2.0 — docs only
- #4411 Update default doc version to 2.0.0 — docs site
- #4409 [TS] Add typescript dependency to typescript templates — template fix (removes a tsc warning)
- #4403 Bump versions to 2.0.1 — version bump
- #4407 Fix version upgrade check for prerelease versions — affects prerelease builds only
- #4399 CI - Fail properly if `psql` failed to install — CI / repo tooling, not user-visible
- #4398 Remove query builder's `.build()` from llm docs — docs only
- #4395 Docs: Add links to all quickstart guides on Getting Started page — docs only
- #4393 [C#] Removes `Query<TRow>` and `.Build()` in favor of `IQuery<TRow>` — API break shipped in 2.0.1 (launch week); not promo
- #4338 commitlog: Improve `committed_meta` — internal
- #4380 TS quickstart template fixes (nextjs + nodejs) — template fixes
- #4383 Docs: SpacetimeDB 2.0 migration guide — docs only
- #4373 Template packages -> workspace instead of 1.* — repo-internal template config
- #4381 Add doc versioning — docs site
- #4382 Move CaseConversionPolicy to public SpacetimeDB namespace — launch-week ergonomics
- #4377 Rename UnknownTransaction event to Transaction — small API rename in launch week (breaking); not promo
- #4372 fix: Additional fixes for templates — template fixes
- #4374 Change deno quickstart to use package.json instead of deno.json — template fix
- #4371 typescript: canonical naming for reducer and procedure. — internal naming correctness fix
- #4366 Add #[spacetimedb::settings] for module-level configuration — already in shipped 2.0.0 (npm and crates.io 2.0.0 contain it)
- #4360 Fix various TS templates — fixes templates broken at 2.0 launch; not promo
- #4263 Case conversion — squash-merge of the branch the v2.0.0 tag points at; already in the shipped 2.0.0 (npm/crates 2.0.0)
- #4368 [C#] Cononical Names and Casing Settings in ModuleDef — already in shipped 2.0.0 (merged before a89634b4a5, which matches npm/crates 2.0.0)
- #4365 gitignore AI agent config dirs — repo hygiene
- #4356 Add more debug logging to the typescript client — already in shipped 2.0.0
- #4357 Fix template `global.json` under Windows — already in shipped 2.0.0
- #4359 Make Rust test clients listen for reducer errors — tests only
- #4317 Fix `spacetime dev` watch filtering and improve quickstart copy-paste experience — already in shipped 2.0.0
- #4347 [TS] Improve how exceptions get rendered in messages — already in shipped 2.0.0
- #4350 Warn about publishing DBs from non-local/non-dev spacetime.json in dev — already in shipped 2.0.0
- #4345 Update benchmark docs — benchmark tooling, not shipped to users

## Open doubts
- **Which video gets v2.0.1?** v2.0.1 is inside this range per the brief, but it is also the public "SpacetimeDB 2.0" release, and `release-v2.0/RESEARCH.md` recommends its items (confirmed reads by default, TS out of beta, DB name optional, offline logout) for the 2.0 video. They are tagged `[v2.0.1 — also claimed by release-v2.0]` above; use each in only one video. If the 2.0 video takes them, the 2.1 top four stays: Rust in the browser, procedure timeouts, Unreal/C++ on 2.0, agent skills.
- **What "2.0.0" users had.** The `v2.0.0` tag is a feature-branch tip. The npm/crates 2.0.0 packages match `a89634b4a5`/`19cc87ebfa`. No v2.0.0 CLI binary is downloadable today (404), and the "SpacetimeDB 2.0" release assets are v2.0.1 builds from 2026-02-24. Whether a v2.0.0 CLI binary was ever attached between 02-20 and 02-24 could not be determined, so CLI users may have gone straight from 1.x to 2.0.1.
- **AgentSkills** aren't versioned with releases, so putting them in the "2.1" video is an editorial choice (first tagged in v2.0.4). No release note mentions them. The skill set on the default branch has since changed (11 skills, new names); recheck right before filming.
- **Update notice**: only shown to users whose installer proxy is ≥2.0.4. The comparison uses the proxy version, so it can be wrong for pinned/older CLI versions.
- **Unreal before 2.1**: not verified whether the 2.0.x Unreal SDK (v1 protocol) could receive event-table rows at all. The safe claim is only that 2.1 moved Unreal to the v2 protocol and added event-table codegen.
- **Windows code signing** (#4473, v2.0.4): the workflow landed, but the v2.0.3, v2.0.4 and v2.1.0 Windows zips contain unsigned `spacetimedb-cli.exe`/`spacetimedb-standalone.exe` (Authenticode data-directory size 0). Signing is real only from v2.2.0 (#4906).
- **Module API breaks inside patch releases** (#4525 TS, #4541 C#: index `accessor` required, v2.0.4; #4461 C++ macro rename; #4497 Unreal): don't describe 2.0.x→2.1 as "drop-in".
- **Rust-in-browser** is only verified to compile and pass the SDK suite under Node (wasm32). No official example to film; the designer would need a custom demo.
- The pgwire "3x" encoding speed-up comes from the dependency's release, not measured by us.
